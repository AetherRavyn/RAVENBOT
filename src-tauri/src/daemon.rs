//! Headless fleet daemon.
//!
//! This is the process the background service runs. It owns the database,
//! runtime, scheduler, webhook receiver, office loops, audit maintenance and
//! optional remote listener, and creates **no window**. That is what lets the
//! Linux user service keep working after logout when lingering is enabled.
//!
//! Exactly one fleet owner may run. The daemon and the desktop app share
//! `~/RAVENBOT/fleet.lock`; a desktop launch can take over from the daemon by
//! stopping the service.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use ravenbot_db::queries::{
    ApprovalQueries, BotQueries, QuestionQueries, WorkflowQueries, WorkflowTaskRecord,
};
use ravenbot_db::Database;
use ravenbot_scheduler::routine::RoutineManager;
use ravenbot_scheduler::{Scheduler, SchedulerConfig, WebhookConfig, WebhookServer};
use uuid::Uuid;

use crate::{
    bootstrap_runtime, execute_graph_for_host, execute_routine_instruction, fleet,
    reconcile_startup_state, remote_config_from_settings, remote_enabled_setting, GraphCondition,
    GraphTask, HostEvents,
};

pub async fn run() -> Result<(), String> {
    let _fleet = match fleet::FleetLock::acquire("daemon") {
        Ok(lock) => lock,
        Err(fleet::FleetLockError::Busy { owner }) => {
            tracing::warn!(
                owner = owner.as_deref().unwrap_or("unknown"),
                "Another RAVENBOT fleet owner is running; the daemon will not start"
            );
            return Ok(());
        }
        Err(error) => return Err(format!("Cannot acquire the fleet lock: {error}")),
    };

    crate::migrate_legacy_database();
    let db_path = ravenbot_core::default_db_path();
    if let Some(parent) = db_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| format!("Cannot create {}: {error}", parent.display()))?;
    }
    let db = Database::new(&db_path)
        .await
        .map_err(|error| format!("Cannot open {}: {error}", db_path.display()))?;

    match reconcile_startup_state(db.pool()).await {
        Ok(state) => tracing::info!(
            resumable_runs = state.resumable_runs,
            failed_runs = state.failed_runs,
            already_paused_runs = state.already_paused_runs,
            retried_workflow_runs = state.retried_workflow_runs,
            resumed_offices = state.resumed_offices,
            failed_workflow_runs = state.failed_workflow_runs,
            expired_approvals = state.expired_approvals,
            expired_questions = state.expired_questions,
            failed_queue_items = state.failed_queue_items,
            reset_channel_tasks = state.reset_channel_tasks,
            reset_agents = state.reset_agents,
            "Headless startup reconciliation complete"
        ),
        Err(error) => tracing::warn!(%error, "Headless startup reconciliation failed"),
    }

    let runtime = bootstrap_runtime(&db).await;

    // Remote access is the only UI when the daemon owns the fleet. When the
    // daemon runs for an account workspace, remote devices must sign in with a
    // local account too; a legacy single-user daemon keeps device-role-only
    // behavior because it has no workspace identity.
    let account_registry: Option<Arc<crate::accounts::RemoteAccountAuthenticator>> =
        match std::env::var("RAVENBOT_WORKSPACE_ID")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        {
            Some(workspace_id) => {
                let base = crate::accounts::base_root();
                let workspace_username = crate::accounts::account_by_id(&base, &workspace_id)
                    .await
                    .ok()
                    .flatten()
                    .map(|account| account.username)
                    .unwrap_or_default();
                Some(Arc::new(crate::accounts::RemoteAccountAuthenticator::new(
                    base,
                    workspace_id,
                    workspace_username,
                )))
            }
            None => None,
        };
    let authenticator: Option<Arc<dyn ravenbot_remote::AccountAuthenticator>> = account_registry
        .clone()
        .map(|registry| registry as Arc<dyn ravenbot_remote::AccountAuthenticator>);
    let provisioner: Option<Arc<dyn ravenbot_remote::AccountProvisioner>> =
        account_registry.map(|registry| registry as Arc<dyn ravenbot_remote::AccountProvisioner>);

    let remote = if remote_enabled_setting(db.pool()).await {
        let config = remote_config_from_settings(db.pool()).await;
        match ravenbot_remote::start_with_services(
            db.pool().clone(),
            runtime.clone(),
            config,
            authenticator,
            provisioner,
        )
        .await
        {
            Ok(server) => {
                tracing::info!(url = %server.url(), "Headless remote access started");
                Some(server)
            }
            Err(error) => {
                tracing::warn!(%error, "Headless remote access did not start");
                None
            }
        }
    } else {
        None
    };

    let routine_manager = Arc::new(RoutineManager::new(db.pool().clone()));
    let scheduler = Arc::new(Scheduler::new(routine_manager, SchedulerConfig::default()));
    let exec_db = db.clone();
    let exec_runtime = runtime.clone();
    let executor: ravenbot_scheduler::scheduler::RoutineExecutor = Arc::new(move |routine| {
        let db = exec_db.clone();
        let runtime = exec_runtime.clone();
        Box::pin(async move {
            execute_routine_instruction(&db, &runtime, &HostEvents::Headless, &routine).await
        })
    });
    scheduler.set_executor(executor.clone()).await;
    let scheduler_task = {
        let scheduler = scheduler.clone();
        tokio::spawn(async move { scheduler.start().await })
    };
    let secret_pool = db.pool().clone();
    tokio::spawn(async move {
        match ravenbot_db::secrets::migrate_plaintext_at_rest(&secret_pool).await {
            Ok(0) => {}
            Ok(count) => tracing::info!(
                count,
                "Migrated legacy plaintext secrets into protected storage"
            ),
            Err(error) => tracing::warn!(%error, "Legacy secret migration did not complete"),
        }
    });

    // Workflow webhooks feed the same headless runner as scheduled workflows.
    let (workflow_tx, workflow_rx) = tokio::sync::mpsc::unbounded_channel::<Uuid>();
    let workflow_trigger: ravenbot_scheduler::webhook::WorkflowTrigger =
        Arc::new(move |workflow_id| {
            let _ = workflow_tx.send(workflow_id);
        });
    let webhook = match WebhookServer::start(
        db.pool().clone(),
        executor,
        Some(workflow_trigger),
        WebhookConfig::default(),
    )
    .await
    {
        Ok(server) => {
            tracing::info!(url = %server.url(), "Headless webhook receiver listening");
            Some(server)
        }
        Err(error) => {
            tracing::warn!(%error, "Headless webhook receiver disabled");
            None
        }
    };

    let queue_task = tokio::spawn(crate::office_queue_loop(
        db.clone(),
        runtime.clone(),
        HostEvents::Headless,
    ));
    let autonomy_task = tokio::spawn(crate::office_autonomy_loop(
        db.clone(),
        runtime.clone(),
        HostEvents::Headless,
    ));
    // 1:1 thread goals are not office-scoped, so they need their own
    // heartbeat. Running it here is what keeps an unattended goal advancing
    // when the headless daemon owns the fleet instead of the desktop window.
    let thread_goal_task = tokio::spawn(crate::thread_goal_loop(
        db.clone(),
        runtime.clone(),
        HostEvents::Headless,
    ));
    let mcp_event_task = tokio::spawn(crate::mcp_events::run_loop(
        db.clone(),
        runtime.clone(),
        HostEvents::Headless,
    ));
    let audit_task = tokio::spawn(crate::audit_maintenance_loop(db.pool().clone()));
    let otel_task = tokio::spawn(crate::otel_export_loop(db.pool().clone()));
    let credentials_task = tokio::spawn(provider_credentials_loop(db.clone(), runtime.clone()));
    let attention_task = tokio::spawn(attention_loop(db.clone()));
    let telegram_task = tokio::spawn(crate::telegram::run_loop(
        db.clone(),
        runtime.clone(),
        HostEvents::Headless,
    ));
    let slack_task = tokio::spawn(crate::slack::run_loop(
        db.clone(),
        runtime.clone(),
        HostEvents::Headless,
    ));
    let discord_task = tokio::spawn(crate::discord::run_loop(
        db.clone(),
        runtime.clone(),
        HostEvents::Headless,
    ));
    let _chat_webhook = match crate::chat_webhook::ChatWebhookServer::start(
        db.clone(),
        runtime.clone(),
        HostEvents::Headless,
        crate::chat_webhook::ChatWebhookConfig::default(),
    )
    .await
    {
        Ok(server) => {
            let (whatsapp_url, teams_url) = server.local_urls();
            tracing::info!(%whatsapp_url, %teams_url, "Chat webhook receiver listening");
            Some(server)
        }
        Err(error) => {
            tracing::warn!(%error, "Chat webhook receiver disabled");
            None
        }
    };
    let workflow_task = tokio::spawn(workflow_loop(db.clone(), runtime.clone(), workflow_rx));

    tracing::info!("RAVENBOT headless daemon running; no window");
    wait_for_shutdown().await;
    tracing::info!("Shutting down the headless daemon");

    if let Some(server) = remote {
        server.shutdown().await;
    }
    scheduler.stop().await;
    scheduler_task.abort();
    queue_task.abort();
    autonomy_task.abort();
    thread_goal_task.abort();
    mcp_event_task.abort();
    audit_task.abort();
    otel_task.abort();
    credentials_task.abort();
    attention_task.abort();
    telegram_task.abort();
    slack_task.abort();
    discord_task.abort();
    workflow_task.abort();
    drop(webhook);
    Ok(())
}

/// Scheduled and webhook-triggered workflows, without a frontend. The desktop
/// uses the same graph executor; this loop only supplies the headless host.
async fn workflow_loop(
    db: Database,
    runtime: Arc<ravenbot_runtime::Runtime>,
    mut triggered: tokio::sync::mpsc::UnboundedReceiver<Uuid>,
) {
    use tokio::time::MissedTickBehavior;

    let mut ticker = tokio::time::interval(Duration::from_secs(60));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                run_due_workflow_once(&db, &runtime).await;
            }
            Some(workflow_id) = triggered.recv() => {
                let workflow_id = workflow_id.to_string();
                match WorkflowQueries::get(db.pool(), &workflow_id).await {
                    Ok(Some(workflow)) => {
                        let Some(office_id) = workflow.office_id.as_deref() else {
                            tracing::warn!(%workflow_id, "Triggered workflow has no office");
                            continue;
                        };
                        match WorkflowQueries::begin_run(
                            db.pool(),
                            &workflow.id,
                            workflow.version,
                            None,
                            &workflow.goal,
                        )
                        .await
                        {
                            Ok(run_id) => {
                                run_workflow_job(
                                    &db,
                                    &runtime,
                                    WorkflowSource::Triggered,
                                    &run_id,
                                    &workflow.id,
                                    &workflow.name,
                                    &workflow.goal,
                                    &workflow.tasks,
                                    Some(office_id),
                                )
                                .await;
                            }
                            Err(error) => tracing::warn!(%error, "Could not start a triggered workflow"),
                        }
                    }
                    Ok(None) => tracing::warn!(%workflow_id, "Triggered workflow no longer exists"),
                    Err(error) => tracing::warn!(%error, "Could not read a triggered workflow"),
                }
            }
            else => break,
        }
    }
}

/// A locked keychain at logout/boot must not require a daemon restart once the
/// user logs in. Keys remain in the platform store; this only re-reads them.
async fn provider_credentials_loop(db: Database, runtime: Arc<ravenbot_runtime::Runtime>) {
    let mut ticker = tokio::time::interval(Duration::from_secs(120));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        crate::hydrate_provider_credentials(&runtime, &db).await;
        tracing::debug!("Refreshed provider credentials for the headless daemon");
    }
}

/// Notify a headless fleet's owner about new parked work. The remote page is
/// the full inbox; this is the push channel: OS notification when a session is
/// available, plus the configured HTTPS webhook and SMTP alert.
async fn attention_loop(db: Database) {
    let mut known: HashSet<Uuid> = HashSet::new();
    let mut ticker = tokio::time::interval(Duration::from_secs(15));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        let approvals = ApprovalQueries::list_pending_all(db.pool(), 50)
            .await
            .unwrap_or_default();
        let questions = QuestionQueries::list_pending_all(db.pool(), 50)
            .await
            .unwrap_or_default();
        let current: HashSet<Uuid> = approvals
            .iter()
            .map(|approval| approval.id)
            .chain(questions.iter().map(|question| question.id))
            .collect();

        for approval in approvals {
            if known.contains(&approval.id) {
                continue;
            }
            let bot_name = BotQueries::get(db.pool(), approval.bot_id)
                .await
                .ok()
                .flatten()
                .map(|bot| bot.name);
            notify_headless(
                "Approval needed",
                &format!("{}: {}", approval.tool_label, approval.tool_name),
            );
            let _ = crate::deliver_attention_webhook_for_pool(
                db.pool(),
                "approval",
                "pending",
                None,
                bot_name.as_deref(),
            )
            .await;
            send_email_alert(&db, "approval", "pending", bot_name.as_deref()).await;
        }
        for question in questions {
            if known.contains(&question.id) {
                continue;
            }
            let bot_name = BotQueries::get(db.pool(), question.bot_id)
                .await
                .ok()
                .flatten()
                .map(|bot| bot.name);
            notify_headless("Question waiting", &question.header);
            let _ = crate::deliver_attention_webhook_for_pool(
                db.pool(),
                "question",
                "pending",
                None,
                bot_name.as_deref(),
            )
            .await;
            send_email_alert(&db, "question", "pending", bot_name.as_deref()).await;
        }
        known = current;
    }
}

fn notify_headless(title: &str, body: &str) {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        let title = title.to_string();
        let body = body.to_string();
        // A detached helper thread waits for the notifier, so a missing
        // desktop session cannot leave a zombie or block the attention loop.
        std::thread::spawn(move || {
            #[cfg(target_os = "linux")]
            let _ = std::process::Command::new("notify-send")
                .args(["-a", "RAVENBOT", "-u", "normal", &title, &body])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            #[cfg(target_os = "macos")]
            {
                let escape = |value: &str| value.replace('\\', "\\\\").replace('"', "\\\"");
                let script = format!(
                    "display notification \"{}\" with title \"{}\"",
                    escape(&body),
                    escape(&title)
                );
                let _ = std::process::Command::new("osascript")
                    .args(["-e", &script])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            }
        });
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (title, body);
    }
}

async fn send_email_alert(db: &Database, kind: &str, state: &str, bot_name: Option<&str>) {
    let config = match crate::load_attention_email_config(db.pool()).await {
        Ok(config)
            if !config.host.is_empty() && !config.from.is_empty() && !config.to.is_empty() =>
        {
            config
        }
        _ => return,
    };
    let kind = kind.to_string();
    let state = state.to_string();
    let bot = bot_name.map(str::to_string);
    let _ = tokio::task::spawn_blocking(move || {
        crate::send_attention_email_blocking(&config, &kind, &state, None, bot.as_deref())
    })
    .await;
}

async fn run_due_workflow_once(db: &Database, runtime: &Arc<ravenbot_runtime::Runtime>) -> bool {
    let now = chrono::Utc::now().to_rfc3339();
    match WorkflowQueries::claim_due(db.pool(), &now).await {
        Ok(Some(job)) => {
            run_workflow_job(
                db,
                runtime,
                WorkflowSource::Scheduled,
                &job.run_id,
                &job.workflow_id,
                &job.name,
                &job.goal,
                &job.tasks,
                Some(&job.office_id),
            )
            .await;
            true
        }
        Ok(None) => false,
        Err(error) => {
            tracing::warn!(%error, "Could not claim a due workflow");
            false
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WorkflowSource {
    Scheduled,
    Triggered,
}

#[allow(clippy::too_many_arguments)]
async fn run_workflow_job(
    db: &Database,
    runtime: &Arc<ravenbot_runtime::Runtime>,
    source: WorkflowSource,
    run_id: &str,
    workflow_id: &str,
    name: &str,
    goal: &str,
    tasks: &[WorkflowTaskRecord],
    office_id: Option<&str>,
) {
    let office_id = match office_id.and_then(|id| Uuid::parse_str(id).ok()) {
        Some(id) => id,
        None => {
            let error = "Workflow has no valid office.".to_string();
            finish_workflow(db, run_id, "failed", None, Some(&error)).await;
            if source == WorkflowSource::Scheduled {
                mark_schedule_error(db, workflow_id, &error).await;
            }
            return;
        }
    };
    let graph_tasks = match tasks
        .iter()
        .map(|task| graph_task(task, goal))
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(tasks) => tasks,
        Err(error) => {
            finish_workflow(db, run_id, "failed", None, Some(&error)).await;
            if source == WorkflowSource::Scheduled {
                mark_schedule_error(db, workflow_id, &error).await;
            }
            return;
        }
    };

    match execute_graph_for_host(
        db,
        runtime,
        &HostEvents::Headless,
        Uuid::nil(),
        goal.to_string(),
        graph_tasks,
        Some(office_id),
        None,
    )
    .await
    {
        Ok(result) => {
            let failed = result.status == "failed";
            let detail = failed.then(|| result.failures.join("; "));
            let thread_id = result.thread_id.map(|id| id.to_string());
            finish_workflow(
                db,
                run_id,
                if failed { "failed" } else { "completed" },
                thread_id.as_deref(),
                detail.as_deref(),
            )
            .await;
            if failed {
                if source == WorkflowSource::Scheduled {
                    mark_schedule_error(
                        db,
                        workflow_id,
                        detail.as_deref().unwrap_or("Workflow failed"),
                    )
                    .await;
                }
                tracing::warn!(workflow = %name, "Headless workflow failed");
            } else {
                tracing::info!(workflow = %name, "Headless workflow completed");
            }
        }
        Err(error) => {
            finish_workflow(db, run_id, "failed", None, Some(&error)).await;
            if source == WorkflowSource::Scheduled {
                mark_schedule_error(db, workflow_id, &error).await;
            }
            tracing::warn!(workflow = %name, %error, "Headless workflow failed");
        }
    }
}

fn graph_task(task: &WorkflowTaskRecord, goal: &str) -> Result<GraphTask, String> {
    Ok(GraphTask {
        bot_id: Uuid::parse_str(task.bot_id.trim())
            .map_err(|_| format!("Workflow task has an invalid bot id: {}", task.bot_id))?,
        instruction: format!("Goal: {}\n\nYour task: {}", goal, task.label),
        depends_on: task.depends_on.clone(),
        condition: task.condition.as_ref().map(|condition| GraphCondition {
            source: condition.source,
            operator: condition.operator.clone(),
            value: condition.value.clone(),
        }),
        data_from: task.data_from.clone(),
        retries: task.retries,
        output_schema: task.output_schema.clone(),
        loop_iterations: task.loop_iterations,
        loop_until: task.loop_until.clone(),
        loop_group: task.loop_group.clone(),
        loop_group_terminal: task.loop_group_terminal,
        loop_group_iterations: task.loop_group_iterations,
        loop_group_stop: task.loop_group_stop.clone(),
        node_type: Some(task.node_type.clone()),
    })
}

async fn finish_workflow(
    db: &Database,
    run_id: &str,
    state: &str,
    thread_id: Option<&str>,
    detail: Option<&str>,
) {
    if let Err(error) =
        WorkflowQueries::finish_run(db.pool(), run_id, state, thread_id, detail).await
    {
        tracing::warn!(%error, "Could not finish a headless workflow run");
    }
}

async fn mark_schedule_error(db: &Database, workflow_id: &str, error: &str) {
    let detail = error.chars().take(2000).collect::<String>();
    if let Err(error) =
        WorkflowQueries::mark_schedule_error(db.pool(), workflow_id, Some(&detail)).await
    {
        tracing::warn!(%error, "Could not record a workflow schedule error");
    }
}

#[cfg(unix)]
async fn wait_for_shutdown() {
    use tokio::signal::unix::{signal, SignalKind};
    let mut terminate = match signal(SignalKind::terminate()) {
        Ok(signal) => signal,
        Err(_) => {
            let _ = tokio::signal::ctrl_c().await;
            return;
        }
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = terminate.recv() => {}
    }
}

#[cfg(not(unix))]
async fn wait_for_shutdown() {
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use ravenbot_core::{Bot, ChatRoom, ChatRoomMember};
    use ravenbot_db::queries::{BotQueries, ChatRoomQueries};

    fn task(
        bot_id: Uuid,
        label: &str,
        node_type: &str,
        depends_on: Vec<usize>,
    ) -> WorkflowTaskRecord {
        WorkflowTaskRecord {
            bot_id: bot_id.to_string(),
            label: label.to_string(),
            node_type: node_type.to_string(),
            depends_on,
            condition: None,
            data_from: None,
            retries: 0,
            output_schema: None,
            loop_iterations: 1,
            loop_until: None,
            loop_group: None,
            loop_group_terminal: false,
            loop_group_iterations: 1,
            loop_group_stop: None,
        }
    }

    #[tokio::test]
    async fn a_due_workflow_runs_and_finishes_without_a_frontend() {
        let path =
            std::env::temp_dir().join(format!("ravenbot-headless-workflow-{}.db", Uuid::new_v4()));
        let db = Database::new(&path).await.expect("db");
        let bot = Bot::new("Workflow Bot", "headless workflow");
        BotQueries::insert(db.pool(), &bot).await.expect("bot");
        let room = ChatRoom::new("Headless Office", "workflow test", "custom");
        ChatRoomQueries::create(db.pool(), &room)
            .await
            .expect("room");
        ChatRoomQueries::add_member(
            db.pool(),
            &ChatRoomMember {
                chatroom_id: room.id,
                bot_id: bot.id,
                rank: "Lead".to_string(),
                specialty: "Generalist".to_string(),
                joined_at: chrono::Utc::now(),
            },
        )
        .await
        .expect("member");

        let workflow_id = Uuid::new_v4().to_string();
        let tasks = vec![
            task(bot.id, "Start", "trigger", vec![]),
            task(bot.id, "Finish", "output", vec![0]),
        ];
        WorkflowQueries::upsert(
            db.pool(),
            &workflow_id,
            "Headless smoke",
            "Produce a result",
            &serde_json::to_string(&tasks).expect("tasks json"),
            Some(room.id),
        )
        .await
        .expect("workflow");
        let due = (chrono::Utc::now() - chrono::Duration::minutes(5)).to_rfc3339();
        WorkflowQueries::set_schedule(db.pool(), &workflow_id, 1, true, Some(&due))
            .await
            .expect("schedule");

        let runtime = Arc::new(ravenbot_runtime::Runtime::new(db.clone()));
        let claimed = WorkflowQueries::claim_due(db.pool(), &chrono::Utc::now().to_rfc3339())
            .await
            .expect("claim")
            .expect("due job");
        tokio::time::timeout(
            Duration::from_secs(30),
            run_workflow_job(
                &db,
                &runtime,
                WorkflowSource::Scheduled,
                &claimed.run_id,
                &claimed.workflow_id,
                &claimed.name,
                &claimed.goal,
                &claimed.tasks,
                Some(&claimed.office_id),
            ),
        )
        .await
        .expect("workflow completes in time");

        let state: String = sqlx::query_scalar("SELECT state FROM workflow_runs WHERE id = ?")
            .bind(&claimed.run_id)
            .fetch_one(db.pool())
            .await
            .expect("run row");
        assert!(
            state == "completed" || state == "failed",
            "a terminal run state is required, got {state}"
        );

        // The webhook path starts a run from the current workflow version and
        // then enters the same headless executor.
        let workflow = WorkflowQueries::get(db.pool(), &workflow_id)
            .await
            .expect("workflow")
            .expect("workflow exists");
        let triggered_run = WorkflowQueries::begin_run(
            db.pool(),
            &workflow.id,
            workflow.version,
            None,
            &workflow.goal,
        )
        .await
        .expect("begin triggered run");
        tokio::time::timeout(
            Duration::from_secs(30),
            run_workflow_job(
                &db,
                &runtime,
                WorkflowSource::Triggered,
                &triggered_run,
                &workflow.id,
                &workflow.name,
                &workflow.goal,
                &workflow.tasks,
                workflow.office_id.as_deref(),
            ),
        )
        .await
        .expect("triggered workflow completes in time");
        let triggered_state: String =
            sqlx::query_scalar("SELECT state FROM workflow_runs WHERE id = ?")
                .bind(&triggered_run)
                .fetch_one(db.pool())
                .await
                .expect("triggered run row");
        assert!(
            triggered_state == "completed" || triggered_state == "failed",
            "a terminal triggered state is required, got {triggered_state}"
        );
        let _ = std::fs::remove_file(&path);
    }
}
