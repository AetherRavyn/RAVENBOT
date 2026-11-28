//! Host-side agent-task scheduler.
//!
//! The runtime owns the tools and the database owns the transition rules. This
//! module is the only thing that turns a queued task into a real run: it claims
//! under dependencies/resources, creates the run on the task's thread, applies
//! the stored approval policy, and writes the result and event back.
//!
//! The loop is spawned by both the desktop shell and the headless daemon. The
//! database claim/fire rules make a second loop or a duplicate event harmless;
//! the fleet lock still means only one process owns the fleet at a time.

use std::sync::Arc;

use chrono::Utc;
use ravenbot_core::agent_task::{AgentTask, AgentTaskState};
use ravenbot_core::{Run, RunOutcome};
use ravenbot_db::agent_tasks::{AgentTaskQueries, AgentWaitQueries, ClaimBatch};
use ravenbot_db::Database;
use ravenbot_runtime::Runtime;
use serde_json::json;

use crate::HostEvents;

/// How many task runs this process will keep in flight. The explicit resource
/// locks remain the real serialization; this is only a cost/CPU cap.
const MAX_CONCURRENT_TASKS: u64 = 4;

/// The background heartbeat. It fires due timers, expires overdue waits, and
/// claims ready tasks until the concurrency cap is reached.
pub(crate) async fn run_loop(db: Database, runtime: Arc<Runtime>, events: HostEvents) {
    // A process that starts with rows left `starting`/`running` is looking at
    // its own previous lifetime. Reconcile before claiming anything new; the
    // startup reconciliation normally does this, but tests and alternate hosts
    // can start the loop directly.
    match AgentTaskQueries::recover_interrupted(
        db.pool(),
        "app quit while the task was running",
        Utc::now(),
    )
    .await
    {
        Ok(tasks) => {
            for task in tasks {
                fire_task(&db, &task).await;
                emit_changed(&events, &task);
            }
        }
        Err(error) => tracing::warn!(%error, "Could not recover interrupted agent tasks"),
    }
    let _ = AgentWaitQueries::recover_interrupted(db.pool(), "app quit while waiting").await;

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        if runtime.is_paused().await {
            continue;
        }
        let now = Utc::now();
        let _ = AgentWaitQueries::fire_timers(db.pool(), now).await;
        let _ = AgentWaitQueries::expire_overdue(db.pool(), now).await;
        if runtime.kill_switch().is_triggered().await {
            continue;
        }
        let active = AgentTaskQueries::active_count(db.pool()).await.unwrap_or(0);
        if active >= MAX_CONCURRENT_TASKS {
            continue;
        }
        for _ in active..MAX_CONCURRENT_TASKS {
            match AgentTaskQueries::claim_next(db.pool(), Utc::now()).await {
                Ok(ClaimBatch { claimed, failed }) => {
                    for task in failed {
                        fire_task(&db, &task).await;
                        emit_changed(&events, &task);
                    }
                    let Some(task) = claimed else {
                        break;
                    };
                    let db = db.clone();
                    let runtime = runtime.clone();
                    let events = events.clone();
                    tokio::spawn(async move {
                        execute_claimed_task(db, runtime, events, task).await;
                    });
                }
                Err(error) => {
                    tracing::warn!(%error, "Could not claim a background task");
                    break;
                }
            }
        }
    }
}

/// Run one claimed task and finalize it. A failure before the run exists is
/// written back as a failed task, never as a silent no-op.
pub(crate) async fn execute_claimed_task(
    db: Database,
    runtime: Arc<Runtime>,
    events: HostEvents,
    task: AgentTask,
) {
    if let Err(error) = run_claimed_task(&db, &runtime, &events, &task).await {
        let reason = format!("The task could not start: {error}");
        match AgentTaskQueries::finish(
            db.pool(),
            task.id,
            AgentTaskState::Failed,
            None,
            Some(&reason),
            Utc::now(),
        )
        .await
        {
            Ok(Some(finished)) => {
                fire_task(&db, &finished).await;
                emit_changed(&events, &finished);
            }
            Ok(None) => {}
            Err(error) => tracing::warn!(%error, "Could not record a task start failure"),
        }
    }
}

async fn run_claimed_task(
    db: &Database,
    runtime: &Arc<Runtime>,
    events: &HostEvents,
    task: &AgentTask,
) -> Result<(), String> {
    let target = ravenbot_db::queries::BotQueries::get(db.pool(), task.target_bot_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "The target agent no longer exists.".to_string())?;
    let Some(thread_id) = task.thread_id else {
        return Err("The task has no thread.".to_string());
    };
    let thread = ravenbot_db::queries::ThreadQueries::get(db.pool(), thread_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "The task thread no longer exists.".to_string())?;

    let mut run = Run::new(target.id, thread.id);
    ravenbot_db::queries::RunQueries::insert(db.pool(), &run)
        .await
        .map_err(|error| error.to_string())?;
    if let Some(role) = task.actor_role.as_deref() {
        let _ = ravenbot_db::queries::RunQueries::set_actor(
            db.pool(),
            run.id,
            task.actor_id.as_deref().unwrap_or("task"),
            task.actor_name.as_deref().unwrap_or("task"),
            role,
            task.actor_workspace_id.as_deref().unwrap_or("workspace"),
        )
        .await;
    }
    let Some(running) =
        AgentTaskQueries::mark_running(db.pool(), task.id, thread.id, run.id, Utc::now())
            .await
            .map_err(|error| error.to_string())?
    else {
        // The owner closed the task between the claim and the run. Keep the
        // run row honest rather than leaving it in planning forever.
        run.complete(RunOutcome::Cancelled {
            reason: Some("task closed while starting".to_string()),
        });
        let _ = ravenbot_db::queries::RunQueries::update(db.pool(), &run).await;
        return Ok(());
    };

    fire_task(db, &running).await;
    emit_changed(events, &running);

    runtime.set_thread_emitter(thread.id, events.stream_emitter());
    runtime.set_delegation_depth(run.id, task.depth);
    if task.auto_allow {
        runtime.allow_approvals_for_run(run.id, true);
    } else if let Some(policy) = task.approval_policy.as_deref() {
        match policy {
            "allow_all" => runtime.allow_approvals_for_run(run.id, true),
            "review" => runtime.review_approvals_for_run(run.id, true),
            _ => {}
        }
    }

    let result = runtime.execute_run(&mut run).await;
    runtime.set_run_approval_policy(run.id, None);
    runtime.clear_delegation_depth(run.id);
    runtime.set_thread_emitter(thread.id, None);

    let (state, result_text, error_text) = match (&run.outcome, result) {
        (Some(RunOutcome::Success { result }), _) => {
            (AgentTaskState::Completed, Some(result.clone()), None)
        }
        (Some(RunOutcome::PartialSuccess { result, errors }), _) => {
            let mut text = result.clone();
            if !errors.is_empty() {
                text.push_str("\n\nWarnings:\n");
                for error in errors.iter().take(8) {
                    text.push_str("- ");
                    text.push_str(error);
                    text.push('\n');
                }
            }
            (AgentTaskState::Completed, Some(text), None)
        }
        (Some(RunOutcome::Failure { error }), _) => {
            (AgentTaskState::Failed, None, Some(error.clone()))
        }
        (Some(RunOutcome::Cancelled { reason }), _) => (
            AgentTaskState::Cancelled,
            None,
            reason
                .clone()
                .or_else(|| Some("task run cancelled".to_string())),
        ),
        (None, Err(error)) => (AgentTaskState::Failed, None, Some(error.to_string())),
        (None, Ok(())) => (
            AgentTaskState::Failed,
            None,
            Some("the task run ended without an outcome".to_string()),
        ),
    };
    let result_text = result_text.map(|text| clip(&text, 8_000));
    let error_text = error_text.map(|text| clip(&text, 4_000));
    let finished = AgentTaskQueries::finish(
        db.pool(),
        task.id,
        state,
        result_text.as_deref(),
        error_text.as_deref(),
        Utc::now(),
    )
    .await
    .map_err(|error| error.to_string())?;
    if let Some(finished) = finished {
        fire_task(db, &finished).await;
        emit_changed(events, &finished);
    }
    Ok(())
}

async fn fire_task(db: &Database, task: &AgentTask) {
    let payload = task.event_payload("state", None);
    if let Err(error) =
        AgentWaitQueries::fire_matching(db.pool(), task.owner_bot_id, "task", &payload, Utc::now())
            .await
    {
        tracing::warn!(%error, task_id = %task.id, "Could not fire task waits");
    }
}

fn emit_changed(events: &HostEvents, task: &AgentTask) {
    events.emit(
        "agent-task-changed",
        json!({
            "task_id": task.id,
            "thread_id": task.thread_id,
            "owner_bot_id": task.owner_bot_id,
            "target_bot_id": task.target_bot_id,
            "title": task.title,
            "state": task.state.as_str(),
            "result": task.result,
            "error": task.error,
        }),
    );
}

fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push_str("… [truncated]");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use ravenbot_core::agent_task::{AgentTaskState, AgentWaitState};
    use ravenbot_core::{Bot, Message, Run, Thread};
    use ravenbot_db::agent_tasks::{
        AgentTaskQueries, AgentWaitQueries, NewAgentTask, NewAgentWait,
    };
    use ravenbot_db::Database;
    use ravenbot_models::{
        DeltaCallback, Message as ModelMessage, ModelProviderTrait, ModelResponse, StreamChunk,
        ToolDefinition, Usage,
    };
    use serde_json::json;
    use uuid::Uuid;

    struct FixedProvider;

    #[async_trait::async_trait]
    impl ModelProviderTrait for FixedProvider {
        fn provider_type(&self) -> ravenbot_core::ModelProvider {
            ravenbot_core::ModelProvider::OpenRouter
        }
        fn with_model(self: Box<Self>, _model: String) -> Box<dyn ModelProviderTrait> {
            self
        }
        async fn complete(
            &self,
            _messages: &[ModelMessage],
            _tools: &[ToolDefinition],
            _temperature: f32,
            _max_tokens: u32,
        ) -> Result<ModelResponse, ravenbot_models::ModelError> {
            Ok(response())
        }
        async fn complete_stream(
            &self,
            _messages: &[ModelMessage],
            _tools: &[ToolDefinition],
            _temperature: f32,
            _max_tokens: u32,
            on_delta: DeltaCallback,
            _reasoning: bool,
        ) -> Result<ModelResponse, ravenbot_models::ModelError> {
            let response = response();
            if let Some(text) = response.content.as_deref() {
                on_delta(StreamChunk::Text(text));
            }
            Ok(response)
        }
        async fn health_check(&self) -> Result<bool, ravenbot_models::ModelError> {
            Ok(true)
        }
    }

    fn response() -> ModelResponse {
        ModelResponse {
            content: Some("The task finished with a concrete result.".to_string()),
            tool_calls: Vec::new(),
            usage: Usage {
                input_tokens: 5,
                output_tokens: 5,
            },
            reasoning: None,
        }
    }

    async fn temp_db() -> (Database, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!("ravenbot-task-{}.db", Uuid::new_v4()));
        let db = Database::new(&path).await.expect("temp db");
        (db, path)
    }

    async fn seed_queued_task(db: &Database) -> (AgentTask, Uuid, Uuid, Thread) {
        let owner = Bot::new("Owner", "starts tasks");
        let target = Bot::new("Target", "runs tasks");
        ravenbot_db::queries::BotQueries::insert(db.pool(), &owner)
            .await
            .unwrap();
        ravenbot_db::queries::BotQueries::insert(db.pool(), &target)
            .await
            .unwrap();
        let thread = Thread::new_task(target.id, "Task: check the build");
        ravenbot_db::queries::ThreadQueries::create(db.pool(), &thread)
            .await
            .unwrap();
        let message = Message::user(thread.id, "Do the task.");
        ravenbot_db::queries::MessageQueries::insert(db.pool(), &message)
            .await
            .unwrap();
        let task = AgentTaskQueries::create(
            db.pool(),
            NewAgentTask {
                id: Uuid::new_v4(),
                owner_bot_id: owner.id,
                target_bot_id: target.id,
                owner_thread_id: None,
                owner_run_id: None,
                actor_id: Some("operator-1"),
                actor_name: Some("Operator"),
                actor_role: Some("operator"),
                actor_workspace_id: Some("workspace-1"),
                thread_id: Some(thread.id),
                office_id: None,
                title: "Check the build",
                instruction: "Do the task.",
                expected_result: "A result.",
                resources: &[],
                depends_on: &[],
                workspace: None,
                depth: 1,
                auto_allow: true,
                approval_policy: Some("allow_all"),
            },
        )
        .await
        .expect("task");
        (task, owner.id, target.id, thread)
    }

    #[tokio::test]
    async fn a_claimed_task_runs_to_completion_and_fires_its_wait() {
        let (db, path) = temp_db().await;
        let runtime = Arc::new(Runtime::new(db.clone()));
        runtime
            .set_provider_override(Some(Arc::new(FixedProvider) as Arc<dyn ModelProviderTrait>))
            .await;
        let (task, owner_id, _target_id, _thread) = seed_queued_task(&db).await;
        let wait = AgentWaitQueries::create(
            db.pool(),
            NewAgentWait {
                bot_id: owner_id,
                run_id: Uuid::new_v4(),
                thread_id: Uuid::new_v4(),
                event_kind: "task",
                filter: json!({"task_id": task.id.to_string()}),
                deadline: Utc::now() + chrono::Duration::seconds(30),
            },
        )
        .await
        .expect("wait");
        let claimed = AgentTaskQueries::claim_next(db.pool(), Utc::now())
            .await
            .unwrap()
            .claimed
            .expect("claimed");
        execute_claimed_task(db.clone(), runtime.clone(), HostEvents::Headless, claimed).await;

        let stored = AgentTaskQueries::get(db.pool(), task.id)
            .await
            .unwrap()
            .expect("task");
        assert_eq!(stored.state, AgentTaskState::Completed, "{stored:?}");
        assert!(
            stored
                .result
                .as_deref()
                .is_some_and(|result| !result.is_empty()),
            "{stored:?}"
        );
        assert!(stored.run_id.is_some());
        let run = ravenbot_db::queries::RunQueries::get(db.pool(), stored.run_id.unwrap())
            .await
            .unwrap()
            .expect("run");
        assert_eq!(run.state, ravenbot_core::RunState::Completed);
        let actor = ravenbot_db::queries::RunQueries::actor_for(db.pool(), run.id)
            .await
            .unwrap()
            .expect("task run actor");
        assert_eq!(actor.role, "operator");
        assert_eq!(actor.workspace_id, "workspace-1");
        let fired = AgentWaitQueries::get(db.pool(), wait.id)
            .await
            .unwrap()
            .expect("wait");
        assert_eq!(fired.state, AgentWaitState::Fired);
        assert_eq!(fired.payload.as_ref().unwrap()["state"], "completed");

        drop(runtime);
        drop(db);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }

    #[tokio::test]
    async fn startup_reconciliation_fails_background_tasks_and_expires_waits() {
        let (db, path) = temp_db().await;
        let (task, owner_id, _target_id, thread) = seed_queued_task(&db).await;
        let claimed = AgentTaskQueries::claim_next(db.pool(), Utc::now())
            .await
            .unwrap()
            .claimed
            .expect("claimed");
        let run = Run::new(task.target_bot_id, thread.id);
        ravenbot_db::queries::RunQueries::insert(db.pool(), &run)
            .await
            .unwrap();
        AgentTaskQueries::mark_running(db.pool(), claimed.id, thread.id, run.id, Utc::now())
            .await
            .unwrap();
        let wait = AgentWaitQueries::create(
            db.pool(),
            NewAgentWait {
                bot_id: owner_id,
                run_id: Uuid::new_v4(),
                thread_id: Uuid::new_v4(),
                event_kind: "task",
                filter: json!({"task_id": task.id.to_string()}),
                deadline: Utc::now() + chrono::Duration::seconds(30),
            },
        )
        .await
        .expect("wait");

        let state = crate::reconcile_startup_state(db.pool())
            .await
            .expect("reconcile");
        assert!(state.interrupted_tasks >= 1);
        assert!(state.expired_waits >= 1);
        let stored = AgentTaskQueries::get(db.pool(), task.id)
            .await
            .unwrap()
            .expect("task");
        assert_eq!(stored.state, AgentTaskState::Failed);
        assert!(stored
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("app quit"));
        let stored_wait = AgentWaitQueries::get(db.pool(), wait.id)
            .await
            .unwrap()
            .expect("wait");
        assert_eq!(stored_wait.state, AgentWaitState::Expired);
        let stored_run = ravenbot_db::queries::RunQueries::get(db.pool(), run.id)
            .await
            .unwrap()
            .expect("run");
        assert_eq!(stored_run.state, ravenbot_core::RunState::Failed);

        drop(db);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }
}
