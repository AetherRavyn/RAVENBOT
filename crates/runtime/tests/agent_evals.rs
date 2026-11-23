//! Deterministic agent-task evals.
//!
//! These are end-to-end scenarios with a scripted provider and a temporary
//! database. They are intentionally separate from unit tests: each one asserts
//! a user-visible agent behavior rather than a helper's return value.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use ravenbot_core::{Bot, Message, MessageContent, Run, RunOutcome, Thread};
use ravenbot_models::{
    DeltaCallback, Message as ModelMessage, ModelError, ModelProviderTrait, ModelResponse,
    StreamChunk, ToolCall, ToolDefinition, Usage,
};
use ravenbot_runtime::executor::GraphExecutor;
use ravenbot_runtime::graph::{ConditionOperator, LoopStop, NodeState, TaskGraph};
use ravenbot_runtime::Runtime;

enum Step {
    Response(ModelResponse),
    Truncate(String),
}

struct ScriptedProvider {
    steps: Mutex<VecDeque<Step>>,
    calls: AtomicUsize,
}

impl ScriptedProvider {
    fn new(steps: Vec<Step>) -> Self {
        Self {
            steps: Mutex::new(steps.into()),
            calls: AtomicUsize::new(0),
        }
    }

    fn response(content: Option<&str>, tool_calls: Vec<ToolCall>) -> Step {
        Step::Response(ModelResponse {
            content: content.map(str::to_string),
            tool_calls,
            usage: Usage {
                input_tokens: 10,
                output_tokens: 5,
            },
            reasoning: None,
        })
    }
}

#[async_trait::async_trait]
impl ModelProviderTrait for ScriptedProvider {
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
    ) -> Result<ModelResponse, ModelError> {
        unreachable!("the eval path uses complete_stream")
    }

    async fn complete_stream(
        &self,
        _messages: &[ModelMessage],
        _tools: &[ToolDefinition],
        _temperature: f32,
        _max_tokens: u32,
        on_delta: DeltaCallback,
        _enable_reasoning: bool,
    ) -> Result<ModelResponse, ModelError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let step = self
            .steps
            .lock()
            .expect("script lock")
            .pop_front()
            .ok_or_else(|| ModelError::Provider("script exhausted".to_string()))?;
        match step {
            Step::Response(response) => {
                if let Some(content) = &response.content {
                    on_delta(StreamChunk::Text(content));
                }
                Ok(response)
            }
            Step::Truncate(partial) => {
                on_delta(StreamChunk::Text(&partial));
                Err(ModelError::IncompleteStream {
                    partial: partial.to_string(),
                })
            }
        }
    }

    async fn health_check(&self) -> Result<bool, ModelError> {
        Ok(true)
    }
}

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("ravenbot-eval-{name}-{}.db", uuid::Uuid::new_v4()))
}

async fn seed(name: &str) -> (ravenbot_db::Database, Bot, Thread) {
    let path = temp_path(name);
    let db = ravenbot_db::Database::new(&path).await.expect("temp db");
    let bot = Bot::new(name, "eval bot");
    ravenbot_db::queries::BotQueries::insert(db.pool(), &bot)
        .await
        .unwrap();
    let thread = Thread::new(bot.id, format!("{name} thread"));
    ravenbot_db::queries::ThreadQueries::create(db.pool(), &thread)
        .await
        .unwrap();
    let message = Message::user(thread.id, "remember this");
    ravenbot_db::queries::MessageQueries::insert(db.pool(), &message)
        .await
        .unwrap();
    (db, bot, thread)
}

async fn run_with_provider(
    db: &ravenbot_db::Database,
    bot: &Bot,
    thread: &Thread,
    steps: Vec<Step>,
) -> (Run, Result<(), ravenbot_runtime::RuntimeError>) {
    let runtime = Arc::new(Runtime::new(db.clone()));
    runtime
        .set_provider_override(Some(
            Arc::new(ScriptedProvider::new(steps)) as Arc<dyn ModelProviderTrait>
        ))
        .await;
    runtime.set_auto_allow_approvals(true);
    let mut run = Run::new(bot.id, thread.id);
    ravenbot_db::queries::RunQueries::insert(db.pool(), &run)
        .await
        .unwrap();
    let result = runtime.execute_run(&mut run).await;
    (run, result)
}

async fn last_assistant(db: &ravenbot_db::Database, thread: Thread) -> String {
    let messages = ravenbot_db::queries::MessageQueries::list_by_thread(db.pool(), thread.id)
        .await
        .unwrap();
    messages
        .iter()
        .rev()
        .find(|message| message.role == ravenbot_core::MessageRole::Assistant)
        .and_then(|message| match &message.content {
            MessageContent::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn eval_tool_loop_runs_a_tool_and_answers() {
    let (db, bot, thread) = seed("tool-loop").await;
    let steps = vec![
        ScriptedProvider::response(
            None,
            vec![ToolCall {
                name: "memory_save".to_string(),
                arguments: serde_json::json!({
                    "content": "Rust is memory-safe",
                    "importance": 0.6
                }),
                id: "call-1".to_string(),
            }],
        ),
        ScriptedProvider::response(Some("Saved that fact."), Vec::new()),
    ];
    let (run, result) = run_with_provider(&db, &bot, &thread, steps).await;
    result.expect("run succeeds");
    assert!(matches!(run.outcome, Some(RunOutcome::Success { .. })));
    assert!(last_assistant(&db, thread.clone())
        .await
        .contains("Saved that fact"));
    let messages = ravenbot_db::queries::MessageQueries::list_by_thread(db.pool(), thread.id)
        .await
        .unwrap();
    let traced = messages.iter().any(|message| match &message.content {
        MessageContent::Text { tools, .. } => tools.iter().any(|tool| tool.name == "memory_save"),
        _ => false,
    });
    assert!(traced, "the tool trace must be present");
}

#[tokio::test]
async fn eval_truncated_stream_is_partial_success() {
    let (db, bot, thread) = seed("truncated").await;
    let steps = vec![Step::Truncate("half an answer".to_string())];
    let (run, result) = run_with_provider(&db, &bot, &thread, steps).await;
    result.expect("a partial stream is a recovered result");
    assert!(matches!(
        run.outcome,
        Some(RunOutcome::PartialSuccess { .. })
    ));
    assert!(last_assistant(&db, thread.clone())
        .await
        .contains("half an answer"));
    assert!(last_assistant(&db, thread)
        .await
        .contains("ended before completion"));
}

#[tokio::test]
async fn eval_workflow_group_loops_until_approved() {
    let path = temp_path("group");
    let db = ravenbot_db::Database::new(&path).await.expect("temp db");
    let bot = Bot::new("Group Eval", "group eval");
    ravenbot_db::queries::BotQueries::insert(db.pool(), &bot)
        .await
        .unwrap();
    let runtime = Arc::new(Runtime::new(db.clone()));
    runtime
        .set_provider_override(Some(Arc::new(ScriptedProvider::new(vec![
            ScriptedProvider::response(Some("draft one"), Vec::new()),
            ScriptedProvider::response(Some("REVISE"), Vec::new()),
            ScriptedProvider::response(Some("draft two"), Vec::new()),
            ScriptedProvider::response(Some("APPROVED"), Vec::new()),
        ])) as Arc<dyn ModelProviderTrait>))
        .await;
    runtime.set_auto_allow_approvals(true);

    let mut graph = TaskGraph::new("group eval");
    let first = graph.add_node(bot.id, "draft");
    let terminal = graph.add_node(bot.id, "review");
    graph.add_edge(first, terminal);
    graph
        .add_loop_group(
            "review-loop",
            vec![first, terminal],
            terminal,
            3,
            Some(LoopStop {
                operator: ConditionOperator::Contains,
                value: "APPROVED".to_string(),
            }),
        )
        .unwrap();
    let graph = Arc::new(tokio::sync::Mutex::new(graph));
    let blackboard = GraphExecutor::new(runtime, db.clone())
        .execute(graph.clone())
        .await
        .expect("graph runs");
    assert_eq!(blackboard.get(&terminal.to_string()), Some("APPROVED"));
    let snapshot = graph.lock().await.clone();
    assert_eq!(snapshot.loop_groups[0].iterations, 2);
    assert!(snapshot.loop_groups[0].finished);
    assert!(matches!(snapshot.nodes[&first].state, NodeState::Done(_)));
}

#[tokio::test]
async fn eval_destructive_shell_is_blocked_without_an_approval_card() {
    let (db, bot, thread) = seed("blocked-shell").await;
    let runtime = Arc::new(Runtime::new(db.clone()));
    runtime
        .set_provider_override(Some(Arc::new(ScriptedProvider::new(vec![
            ScriptedProvider::response(
                None,
                vec![ToolCall {
                    name: "shell_exec".to_string(),
                    arguments: serde_json::json!({"command": "rm -rf /"}),
                    id: "call-destructive".to_string(),
                }],
            ),
            ScriptedProvider::response(Some("Blocked by the safety policy."), Vec::new()),
        ])) as Arc<dyn ModelProviderTrait>))
        .await;
    let mut run = Run::new(bot.id, thread.id);
    ravenbot_db::queries::RunQueries::insert(db.pool(), &run)
        .await
        .unwrap();

    let result = runtime.execute_run(&mut run).await;
    result.expect("the run continues after a blocked tool");
    assert_eq!(run.tool_cost, 0.0, "a blocked call must not be charged");
    let pending: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM approvals WHERE status = 'pending'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(
        pending, 0,
        "a blocked command must not create an approval card"
    );
    assert!(last_assistant(&db, thread)
        .await
        .contains("Blocked by the safety policy"));
}

#[tokio::test]
async fn eval_denied_approval_does_not_run_the_tool() {
    let _ = std::fs::remove_file("/tmp/ravenbot-eval-denied");
    let (db, mut bot, thread) = seed("denied-approval").await;
    // This eval is about an explicit Ask bot; the product default is Full.
    bot.approval_mode = ravenbot_core::ApprovalMode::Ask;
    ravenbot_db::queries::BotQueries::update(db.pool(), &bot)
        .await
        .unwrap();
    ravenbot_db::queries::AppSettingsQueries::set(
        db.pool(),
        "tool_prices",
        r#"{"shell_exec":1.0}"#,
    )
    .await
    .unwrap();
    let runtime = Arc::new(Runtime::new(db.clone()));
    runtime
        .set_provider_override(Some(Arc::new(ScriptedProvider::new(vec![
            ScriptedProvider::response(
                None,
                vec![ToolCall {
                    name: "shell_exec".to_string(),
                    arguments: serde_json::json!({"command": "touch /tmp/ravenbot-eval-denied"}),
                    id: "call-shell".to_string(),
                }],
            ),
            ScriptedProvider::response(Some("I stopped at the approval."), Vec::new()),
        ])) as Arc<dyn ModelProviderTrait>))
        .await;
    let mut run = Run::new(bot.id, thread.id);
    ravenbot_db::queries::RunQueries::insert(db.pool(), &run)
        .await
        .unwrap();
    let task_runtime = runtime.clone();
    let handle = tokio::spawn(async move {
        let result = task_runtime.execute_run(&mut run).await;
        (result, run)
    });

    let approval_id = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Ok(Some((id,))) = sqlx::query_as::<_, (String,)>(
                "SELECT id FROM approvals WHERE status = 'pending' LIMIT 1",
            )
            .fetch_optional(db.pool())
            .await
            {
                return id;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("approval card");
    let approval_id = uuid::Uuid::parse_str(&approval_id).unwrap();
    assert!(ravenbot_db::queries::ApprovalQueries::decide(
        db.pool(),
        approval_id,
        false,
        Some("denied by eval"),
    )
    .await
    .unwrap());
    let (result, run) = handle.await.expect("task joins");
    result.expect("run finishes after denial");
    assert!(matches!(run.outcome, Some(RunOutcome::Success { .. })));
    assert!(!std::path::Path::new("/tmp/ravenbot-eval-denied").exists());
    assert_eq!(run.tool_cost, 0.0, "a denied call must not be charged");
    assert!(last_assistant(&db, thread)
        .await
        .contains("stopped at the approval"));
}
