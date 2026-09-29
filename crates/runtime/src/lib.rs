//! RAVENBOT agent runtime and orchestration engine
//!
//! This crate implements the directed task graph executor for parallel
//! multi-agent orchestration with checkpoint/resume capabilities.

pub mod graph;
pub mod executor;
pub mod orchestrator;
pub mod state;
pub mod workspace;

use ravenbot_core::{Run, RunState};
use ravenbot_db::Database;
use ravenbot_models::{ProviderManager, Message, ToolDefinition, DeltaCallback, ModelProviderTrait};
use ravenbot_skills::{SkillRegistry, SkillContext, SkillKind};
use ravenbot_plugins::{PluginRegistry, store::PluginStore};
use ravenbot_mcp::McpRegistry;
use ravenbot_sandbox::KillSwitch;
use ravenbot_memory::{MemoryStore, MemoryRetriever, SelfReviewer, OfficeMemoryStore, LearningEngine, embedding::LocalEmbedding};
use ravenbot_governance::{BudgetManager, AuditLogger, PromptVersionControl};
use std::sync::Arc;
use std::collections::{HashMap, HashSet};
use tokio::sync::Mutex;
use thiserror::Error;
use uuid::Uuid;
use serde::Serialize;
use base64::engine::general_purpose;

/// Live events streamed to the UI during a run.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StreamEvent {
    /// A new assistant token delta arrived
    Delta { bot_id: Uuid, thread_id: Uuid, content: String },
    /// Clear streamed text (a new model round begins, e.g. after tool use)
    Clear { bot_id: Uuid, thread_id: Uuid },
    /// A tool/skill execution started
    ToolStarted { thread_id: Uuid, bot_id: Uuid, name: String },
    /// A tool/skill execution finished
    ToolFinished { thread_id: Uuid, bot_id: Uuid, name: String },
    /// Web sources arrived from a search tool (live citation chips)
    Sources { thread_id: Uuid, sources: Vec<ravenbot_core::Source> },
    /// A tool produced an image (e.g. a screenshot) — render inline live.
    Image { thread_id: Uuid, name: String, data_url: String },
    /// A tool call is parked waiting for user approval (Allow/Deny card)
    ApprovalRequested {
        bot_id: Uuid,
        thread_id: Uuid,
        approval: ravenbot_core::ApprovalRequest,
    },
    /// A parked approval was decided (card flips to Allowed/Denied)
    ApprovalDecided {
        thread_id: Uuid,
        approval_id: Uuid,
        allowed: bool,
    },
    /// The agent asked the user a question (human-in-the-loop card)
    QuestionAsked {
        bot_id: Uuid,
        thread_id: Uuid,
        question: ravenbot_core::QuestionRequest,
    },
    /// A parked question was answered
    QuestionAnswered {
        thread_id: Uuid,
        question_id: Uuid,
        answer: String,
    },
    /// Live bot status for the run lifecycle (thinking / running_tool / done)
    Status { bot_id: Uuid, thread_id: Uuid, state: String },
    /// Real token/cost usage for a completed run
    Usage { thread_id: Uuid, tokens: u64, cost: f64 },
}

/// Emitter callback for stream events. Must be cheap and non-blocking.
pub type StreamEmitter = Arc<dyn Fn(StreamEvent) + Send + Sync>;

/// The thread a stream event belongs to (every variant carries one), used to
/// route the event to that thread's emitter.
fn stream_event_thread_id(event: &StreamEvent) -> Uuid {
    match event {
        StreamEvent::Delta { thread_id, .. }
        | StreamEvent::Clear { thread_id, .. }
        | StreamEvent::ToolStarted { thread_id, .. }
        | StreamEvent::ToolFinished { thread_id, .. }
        | StreamEvent::Sources { thread_id, .. }
        | StreamEvent::Image { thread_id, .. }
        | StreamEvent::ApprovalRequested { thread_id, .. }
        | StreamEvent::ApprovalDecided { thread_id, .. }
        | StreamEvent::QuestionAsked { thread_id, .. }
        | StreamEvent::QuestionAnswered { thread_id, .. }
        | StreamEvent::Status { thread_id, .. }
        | StreamEvent::Usage { thread_id, .. } => *thread_id,
    }
}

/// Keep every protected item, then fill the remaining capacity with the rest,
/// so explicit/core items can never be truncated away. Protected items are
/// returned first. Pure helper so the policy is unit-testable.
fn cap_preserving_protected<T, F>(
    items: Vec<T>,
    protected: &HashSet<String>,
    cap: usize,
    id: F,
) -> Vec<T>
where
    F: Fn(&T) -> String,
{
    let (mut protected_items, mut extra): (Vec<T>, Vec<T>) =
        items.into_iter().partition(|item| protected.contains(&id(item)));
    let room = cap.saturating_sub(protected_items.len());
    extra.truncate(room);
    protected_items.extend(extra);
    protected_items
}

/// Host-control policy for this session:
/// - `opt_in_required` — supported, but the agent must explicitly opt in
/// - `blocked` — fail-closed (Wayland without an explicit override)
fn host_control_policy() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        let wayland = std::env::var("XDG_SESSION_TYPE")
            .map(|v| v.eq_ignore_ascii_case("wayland"))
            .unwrap_or(false)
            || std::env::var("WAYLAND_DISPLAY")
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false);
        if wayland {
            let override_on = std::env::var("RAVENBOT_ALLOW_WAYLAND_CONTROL")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false);
            return if override_on { "opt_in_required" } else { "blocked" };
        }
    }
    "opt_in_required"
}

/// The workspace an office (or a bare bot) works in when none is configured.
///
/// Delegates to `ravenbot_core::office_workspace`, so the folder is the same
/// one `~/RAVENBOT/projects/<slug>` and it is stable for a given name: calling
/// this twice returns the same directory, and a directory RAVENBOT did not
/// create is stepped over rather than adopted. `RAVENBOT_PROJECTS_DIR`
/// relocates the whole `projects` subtree.
pub fn default_project_dir(name: &str) -> std::path::PathBuf {
    ravenbot_core::office_workspace(name)
}

/// Expand a leading `~` to the user's home directory.
fn expand_home(path: &str) -> std::path::PathBuf {
    ravenbot_core::expand_home(path)
}

/// Runtime errors
#[derive(Error, Debug)]
pub enum RuntimeError {
    #[error("Database error: {0}")]
    Database(#[from] ravenbot_db::DbError),
    #[error("SQL error: {0}")]
    Sql(#[from] sqlx::Error),
    #[error("Model error: {0}")]
    Model(String),
    #[error("Task failed: {0}")]
    TaskFailed(String),
    #[error("Graph error: {0}")]
    Graph(String),
    #[error("Skill error: {0}")]
    Skill(String),
    #[error("Kill switch active: {0}")]
    KillSwitchActive(String),
    #[error("Budget exceeded: {0}")]
    BudgetExceeded(String),
    #[error("Tool denied by user: {0}")]
    ToolDenied(String),
    #[error("Waiting on approval: {0}")]
    WaitingOnApproval(String),
}

/// The main runtime for executing bot runs
pub struct Runtime {
    db: Database,
    provider_manager: Arc<Mutex<ProviderManager>>,
    skill_registry: Arc<SkillRegistry>,
    plugin_registry: Arc<PluginRegistry>,
    mcp_registry: Arc<McpRegistry>,
    kill_switch: Arc<KillSwitch>,
    memory_store: Arc<MemoryStore>,
    memory_retriever: Arc<MemoryRetriever>,
    self_reviewer: Arc<SelfReviewer>,
    office_memory: Arc<OfficeMemoryStore>,
    learning: Arc<LearningEngine>,
    budget_manager: Arc<BudgetManager>,
    audit_logger: Arc<AuditLogger>,
    version_control: Arc<PromptVersionControl>,
    /// Fallback emitter for runs without a thread-specific one (office nodes).
    default_stream_emitter: std::sync::RwLock<Option<StreamEmitter>>,
    /// Per-thread emitters, so concurrent runs (two chats, or a chat while an
    /// office runs) never clobber each other's stream.
    stream_emitters: std::sync::RwLock<HashMap<Uuid, StreamEmitter>>,
    /// Injectable provider override (tests/dev tooling): when set, execute_run
    /// uses it instead of creating a provider from the bot's config.
    provider_override: Arc<Mutex<Option<Arc<dyn ModelProviderTrait>>>>,
    /// Delegation depth per run (recursion guard for inter-bot delegation)
    delegation_depth: std::sync::RwLock<HashMap<Uuid, u32>>,
    /// Headless mode: no UI is watching, so approval gates auto-allow (with
    /// audit) instead of parking forever. Set for CLI runs, office graph
    /// nodes, and routine execution.
    auto_allow_approvals: std::sync::atomic::AtomicBool,
    /// Per-run headless auto-allow (office graph nodes / routines). Unlike the
    /// process-global flag above, this cannot leak into other concurrent runs:
    /// parallel office nodes used to flip the global flag on/off around each
    /// node and race each other (silently auto-approving tools in a UI run, or
    /// blocking a headless node).
    auto_allow_runs: std::sync::Mutex<HashSet<Uuid>>,
    /// Cooperative cancellation flags by run id. `cancel_run` marks the run;
    /// the execution loop observes it between steps and stops cleanly.
    cancel_flags: std::sync::Mutex<HashSet<Uuid>>,
    /// Live cancellation tokens for runs executing on an external engine.
    engine_cancels: std::sync::Mutex<HashMap<Uuid, ravenbot_engines::CancelToken>>,
    /// Runs requested to pause at the next tool-round boundary (resumable).
    pause_flags: std::sync::Mutex<HashSet<Uuid>>,
}

impl Runtime {
    /// Create a new runtime
    pub fn new(db: Database) -> Self {
        let embedding_provider = Box::new(LocalEmbedding::new(128));
        let memory_store = Arc::new(MemoryStore::new(db.pool().clone(), embedding_provider));
        let memory_retriever = Arc::new(MemoryRetriever::new(
            MemoryStore::new(db.pool().clone(), Box::new(LocalEmbedding::new(128)))
        ));
        let self_reviewer = Arc::new(SelfReviewer::new(db.pool().clone(),
            MemoryStore::new(db.pool().clone(), Box::new(LocalEmbedding::new(128)))
        ));

        let plugin_registry = Arc::new(PluginRegistry::new(db.pool().clone()));
        let mcp_registry = Arc::new(McpRegistry::new(db.pool().clone()));
        // Ensure plugin + mcp tables exist (clean, only user-added)
        let pool_clone = db.pool().clone();
        let pool_clone2 = db.pool().clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let store = PluginStore::new(pool_clone);
                let _ = store.ensure_tables().await;
            });
            let mcp = McpRegistry::new(pool_clone2);
            handle.spawn(async move { let _ = mcp.ensure_tables().await; });
        }
        let office_memory = Arc::new(OfficeMemoryStore::new(db.pool().clone(), Box::new(LocalEmbedding::new(128))));
        let learning = Arc::new(LearningEngine::new(db.pool().clone()));
        Self {
            db: db.clone(),
            provider_manager: Arc::new(Mutex::new(ProviderManager::new())),
            skill_registry: Arc::new(SkillRegistry::new_builtin()),
            plugin_registry,
            mcp_registry,
            kill_switch: Arc::new(KillSwitch::new()),
            memory_store,
            memory_retriever,
            self_reviewer,
            office_memory,
            learning,
            budget_manager: Arc::new(BudgetManager::new(db.pool().clone())),
            audit_logger: Arc::new(AuditLogger::new(db.pool().clone())),
            version_control: Arc::new(PromptVersionControl::new(db.pool().clone())),
            stream_emitters: std::sync::RwLock::new(HashMap::new()),
            default_stream_emitter: std::sync::RwLock::new(None),
            provider_override: Arc::new(Mutex::new(None)),
            delegation_depth: std::sync::RwLock::new(HashMap::new()),
            auto_allow_approvals: std::sync::atomic::AtomicBool::new(false),
            auto_allow_runs: std::sync::Mutex::new(HashSet::new()),
            cancel_flags: std::sync::Mutex::new(HashSet::new()),
            engine_cancels: std::sync::Mutex::new(HashMap::new()),
            pause_flags: std::sync::Mutex::new(HashSet::new()),
        }
    }

    /// Request cooperative cancellation of a run. The run stops at the next
    /// step boundary (before a model round, tool call, or approval wait).
    pub fn request_cancel(&self, run_id: Uuid) {
        if let Ok(mut flags) = self.cancel_flags.lock() {
            flags.insert(run_id);
        }
        // Interrupt a live external-engine process immediately.
        if let Ok(tokens) = self.engine_cancels.lock() {
            if let Some(token) = tokens.get(&run_id) {
                token.cancel();
            }
        }
    }

    /// Whether cancellation was requested for a run.
    fn is_cancelled(&self, run_id: Uuid) -> bool {
        self.cancel_flags
            .lock()
            .map(|flags| flags.contains(&run_id))
            .unwrap_or(false)
    }

    /// Clear a run's cancellation flag (called when a run finishes).
    fn clear_cancel(&self, run_id: Uuid) {
        if let Ok(mut flags) = self.cancel_flags.lock() {
            flags.remove(&run_id);
        }
        // A run that ends for any reason is no longer pausable.
        if let Ok(mut flags) = self.pause_flags.lock() {
            flags.remove(&run_id);
        }
    }

    /// Request a resumable pause at the next tool-round boundary.
    pub fn request_pause(&self, run_id: Uuid) {
        if let Ok(mut flags) = self.pause_flags.lock() {
            flags.insert(run_id);
        }
    }

    fn is_pause_requested(&self, run_id: Uuid) -> bool {
        self.pause_flags
            .lock()
            .map(|flags| flags.contains(&run_id))
            .unwrap_or(false)
    }

    /// Build the ordered provider chain: primary (or test override) followed by
    /// the bot's configured fallback when it differs.
    async fn provider_chain(
        &self,
        bot: &ravenbot_core::Bot,
    ) -> Result<Vec<Arc<dyn ModelProviderTrait>>, RuntimeError> {
        if let Some(p) = self.provider_override.lock().await.clone() {
            return Ok(vec![p]);
        }
        let manager = self.provider_manager.lock().await;
        let primary = Arc::from(
            manager
                .create_provider_from_str_with_model(
                    &bot.config.model_provider,
                    Some(&bot.config.model_id),
                )
                .map_err(|e| RuntimeError::Model(e.to_string()))?,
        );
        let mut chain = vec![primary];
        if let Some(fallback) = bot
            .config
            .fallback_provider
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if !fallback.eq_ignore_ascii_case(&bot.config.model_provider) {
                match manager.create_provider_from_str_with_model(
                    fallback,
                    bot.config.fallback_model.as_deref(),
                ) {
                    Ok(p) => chain.push(Arc::from(p)),
                    Err(e) => tracing::warn!(
                        fallback,
                        error = %e,
                        "Fallback provider could not be created; continuing with primary only"
                    ),
                }
            }
        }
        Ok(chain)
    }

    /// Does the bot's model support native function calling? Precedence:
    /// custom provider's declared flag → built-in model catalog metadata →
    /// Local provider (never) → default TRUE, so every known-capable path
    /// behaves exactly as before.
    async fn model_supports_tools(
        &self,
        bot: &ravenbot_core::Bot,
        provider_type: &ravenbot_core::ModelProvider,
    ) -> bool {
        if let Some(spec) = self
            .provider_manager
            .lock()
            .await
            .custom_spec(bot.config.model_provider.trim())
        {
            return spec.supports_tools;
        }
        if matches!(provider_type, ravenbot_core::ModelProvider::Local) {
            return false;
        }
        let model = bot.config.model_id.trim().to_lowercase();
        if !model.is_empty() {
            if let Some(cfg) = ravenbot_core::builtin_models().into_iter().find(|m| {
                let id = m.model_id.to_lowercase();
                id == model || model.ends_with(&format!("/{}", id))
            }) {
                return cfg.supports_tools;
            }
        }
        true
    }

    /// Call the model, trying the primary then the fallback. Each provider gets
    /// one automatic retry for transient failures. Returns the response and the
    /// index of the provider that answered (so later rounds reuse it).
    #[allow(clippy::too_many_arguments)]
    async fn call_model(
        &self,
        chain: &[Arc<dyn ModelProviderTrait>],
        start_idx: usize,
        messages: &[ravenbot_models::Message],
        tools: &[ravenbot_models::ToolDefinition],
        temperature: f32,
        max_tokens: u32,
        on_delta: ravenbot_models::DeltaCallback,
        is_think: bool,
    ) -> Result<(ravenbot_models::ModelResponse, usize), RuntimeError> {
        let mut last_err: Option<String> = None;
        for (offset, provider) in chain.iter().enumerate().skip(start_idx.min(chain.len().saturating_sub(1))) {
            let first = provider
                .complete_stream(messages, tools, temperature, max_tokens, on_delta.clone(), is_think)
                .await;
            match first {
                Ok(resp) => return Ok((resp, offset)),
                Err(e) => {
                    let msg = e.to_string();
                    if is_retryable_model_error(&msg) && !self.kill_switch.is_triggered().await {
                        tracing::warn!(provider = offset, error = %msg, "Transient model failure; retrying once");
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        match provider
                            .complete_stream(messages, tools, temperature, max_tokens, on_delta.clone(), is_think)
                            .await
                        {
                            Ok(resp) => return Ok((resp, offset)),
                            Err(e2) => last_err = Some(format!("{} (retry failed: {})", msg, e2)),
                        }
                    } else {
                        last_err = Some(msg);
                    }
                }
            }
        }
        Err(RuntimeError::Model(
            last_err.unwrap_or_else(|| "no model provider available".to_string()),
        ))
    }

    /// Run a single-shot text completion as a specific bot (no tools). Public
    /// so the shell can ask a bot to e.g. propose an office org.
    pub async fn complete_as_bot(
        &self,
        bot_id: Uuid,
        system: &str,
        prompt: &str,
        max_tokens: u32,
    ) -> Result<String, RuntimeError> {
        let bot = ravenbot_db::queries::BotQueries::get(self.db.pool(), bot_id)
            .await?
            .ok_or_else(|| RuntimeError::TaskFailed("Bot not found".to_string()))?;
        self.generate_text(&bot, system, prompt, max_tokens).await
    }

    /// Single-shot text completion on a bot's provider (no tools, no
    /// streaming). Used by the office orchestrator for planning and synthesis.
    async fn generate_text(
        &self,
        bot: &ravenbot_core::Bot,
        system: &str,
        prompt: &str,
        max_tokens: u32,
    ) -> Result<String, RuntimeError> {
        let chain = self.provider_chain(bot).await?;
        let provider = chain
            .into_iter()
            .next()
            .ok_or_else(|| RuntimeError::Model("no provider available".to_string()))?;
        let messages = vec![
            Message::text("system", system),
            Message::text("user", prompt),
        ];
        let noop: DeltaCallback = Arc::new(|_| {});
        let resp = provider
            .complete_stream(&messages, &[], 0.3, max_tokens, noop, false)
            .await
            .map_err(|e| RuntimeError::Model(e.to_string()))?;
        Ok(resp.content.unwrap_or_default())
    }

    /// Answer a standalone social turn directly, without tool assembly, memory
    /// retrieval, skill discovery, or reasoning scaffolding. Returns `true`
    /// when the turn was completed here; `false` means the caller should use
    /// the normal agent pipeline.
    async fn execute_simple_conversational_turn(
        &self,
        run: &mut Run,
        bot: &ravenbot_core::Bot,
        messages: &[ravenbot_core::Message],
        last_user_message: &str,
    ) -> Result<bool, RuntimeError> {
        if !is_simple_conversational_turn(&bot.name, last_user_message, messages) {
            return Ok(false);
        }
        if self.is_cancelled(run.id) {
            run.complete(ravenbot_core::RunOutcome::Cancelled {
                reason: Some("User cancelled".to_string()),
            });
            ravenbot_db::queries::RunQueries::update(self.db.pool(), run).await?;
            self.clear_cancel(run.id);
            return Ok(true);
        }

        let providers = self.provider_chain(bot).await?;
        let persona = bot.config.custom_prompt.clone().unwrap_or_else(|| {
            format!(
                "You are {}, a helpful AI assistant. Answer directly and briefly.",
                bot.name
            )
        });
        let mut model_messages = vec![Message::text(
            "system",
            format!(
                "{persona}\n\n[Fast conversational reply]\nAnswer only this short social message directly and briefly. Do not use tools, describe tools, or start a task loop."
            ),
        )];
        model_messages.extend(direct_conversation_history(messages, last_user_message));

        let fast_max_tokens = bot.config.max_tokens.unwrap_or(4096).min(256);
        let (response, _) = self
            .call_model(
                &providers,
                0,
                &model_messages,
                &[],
                bot.config.temperature.unwrap_or(0.7),
                fast_max_tokens,
                self.delta_emitter(bot.id, run.thread_id),
                false,
            )
            .await?;
        if !response.tool_calls.is_empty() {
            // Should not happen without tool definitions, but never silently
            // convert a tool request into plain text.
            return Ok(false);
        }
        let Some(content) = response.content.filter(|text| !text.trim().is_empty()) else {
            // An empty lightweight reply can be transient. Fall through to the
            // full agent loop instead of failing a message that might succeed
            // with complete context.
            tracing::warn!("Empty lightweight conversational response; using full agent loop");
            return Ok(false);
        };

        run.add_usage(
            response.usage.input_tokens + response.usage.output_tokens,
            response.usage.cost(0.003, 0.015),
        );
        let _ = self
            .budget_manager
            .record_usage(
                bot.id,
                response.usage.input_tokens + response.usage.output_tokens,
                response.usage.cost(0.003, 0.015),
            )
            .await;

        let final_content = match response.reasoning.filter(|reasoning| !reasoning.trim().is_empty()) {
            Some(reasoning) if !content.contains("<think>") => {
                format!("<think>\n{}\n</think>\n\n{}", reasoning.trim(), content)
            }
            _ => content,
        };
        let assistant_msg = ravenbot_core::Message::assistant_with_sources(
            run.thread_id,
            final_content,
            Vec::new(),
        );
        ravenbot_db::queries::MessageQueries::insert(self.db.pool(), &assistant_msg).await?;

        self.emit(StreamEvent::Usage {
            thread_id: run.thread_id,
            tokens: run.tokens_consumed,
            cost: run.cost_estimate,
        });
        self.emit(StreamEvent::Status {
            bot_id: bot.id,
            thread_id: run.thread_id,
            state: "done".to_string(),
        });
        run.complete(ravenbot_core::RunOutcome::Success {
            result: "Simple conversational response generated".to_string(),
        });
        ravenbot_db::queries::RunQueries::update(self.db.pool(), run).await?;
        Ok(true)
    }

    /// Ask a lead bot to plan an office task. Returns `None` when the model
    /// fails or returns an unusable plan, so callers can fall back.
    pub async fn plan_office(
        &self,
        lead_bot_id: Uuid,
        office_goal: Option<&str>,
        office_policy: Option<&str>,
        members: &[orchestrator::OfficeMember],
        request: &str,
    ) -> Option<orchestrator::OfficePlan> {
        let bot = ravenbot_db::queries::BotQueries::get(self.db.pool(), lead_bot_id)
            .await
            .ok()
            .flatten()?;
        let prompt = orchestrator::plan_prompt(office_goal, office_policy, members, request);
        let text = self
            .generate_text(&bot, "You are a precise task planner. Output only JSON.", &prompt, 1200)
            .await
            .ok()?;
        match orchestrator::parse_plan(&text) {
            Some(plan) => Some(plan),
            None => {
                tracing::warn!("Office plan was unparseable; falling back to fan-out");
                None
            }
        }
    }

    /// Ask a lead bot to synthesize the final answer from task results.
    /// Falls back to a plain concatenation when the model fails.
    pub async fn synthesize_office(
        &self,
        lead_bot_id: Uuid,
        request: &str,
        results: &[(String, String)],
    ) -> String {
        let fallback = || {
            results
                .iter()
                .map(|(label, out)| format!("### {}\n{}", label, out.trim()))
                .collect::<Vec<_>>()
                .join("\n\n")
        };
        let Ok(Some(bot)) = ravenbot_db::queries::BotQueries::get(self.db.pool(), lead_bot_id).await
        else {
            return fallback();
        };
        let prompt = orchestrator::synthesis_prompt(request, results);
        match self
            .generate_text(
                &bot,
                "You are the lead of an AI team, writing the final answer for the user.",
                &prompt,
                2000,
            )
            .await
        {
            Ok(text) if !text.trim().is_empty() => text,
            _ => fallback(),
        }
    }

    /// Max model↔tool rounds per run. Configurable via bot config key
    /// `max_tool_rounds` or env `RAVENBOT_MAX_TOOL_ROUNDS` (default 12).
    fn max_tool_rounds(&self, bot: &ravenbot_core::Bot) -> u32 {
        bot.config
            .max_tool_rounds
            .or_else(|| {
                std::env::var("RAVENBOT_MAX_TOOL_ROUNDS")
                    .ok()
                    .and_then(|v| v.trim().parse::<u32>().ok())
            })
            .filter(|v| *v > 0)
            .unwrap_or(12)
    }

    /// Enable/disable headless auto-allow for approval gates.
    pub fn set_auto_allow_approvals(&self, allow: bool) {
        self.auto_allow_approvals
            .store(allow, std::sync::atomic::Ordering::SeqCst);
    }

    fn auto_allow(&self) -> bool {
        self.auto_allow_approvals.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Enable/disable headless auto-allow for one specific run. Use this for
    /// concurrent runs (office graph nodes, routines) instead of the global
    /// `set_auto_allow_approvals`, which leaks across runs.
    pub fn allow_approvals_for_run(&self, run_id: Uuid, allow: bool) {
        if let Ok(mut set) = self.auto_allow_runs.lock() {
            if allow {
                set.insert(run_id);
            } else {
                set.remove(&run_id);
            }
        }
    }

    /// Whether this particular run should auto-allow approval gates.
    fn auto_allow_for(&self, run_id: Uuid) -> bool {
        if self.auto_allow() {
            return true;
        }
        self.auto_allow_runs
            .lock()
            .map(|set| set.contains(&run_id))
            .unwrap_or(false)
    }

    /// Install (or remove, with `None`) a provider override used by execute_run
    /// instead of the bot's configured provider. Test/dev hook.
    pub async fn set_provider_override(&self, provider: Option<Arc<dyn ModelProviderTrait>>) {
        *self.provider_override.lock().await = provider;
    }

    /// Install (or remove, with `None`) the fallback live stream emitter used
    /// by runs that did not register a thread-specific emitter (office nodes).
    pub fn set_stream_emitter(&self, emitter: Option<StreamEmitter>) {
        *self.default_stream_emitter.write().expect("stream emitter lock poisoned") = emitter;
    }

    /// Install (or remove, with `None`) the stream emitter for one thread.
    /// Concurrent runs each use their own thread, so their streams stay isolated.
    pub fn set_thread_emitter(&self, thread_id: Uuid, emitter: Option<StreamEmitter>) {
        let mut map = self.stream_emitters.write().expect("stream emitter lock poisoned");
        match emitter {
            Some(e) => {
                map.insert(thread_id, e);
            }
            None => {
                map.remove(&thread_id);
            }
        }
    }

    /// Resolve the emitter to use for a thread: a thread-specific one if
    /// registered, otherwise the fallback.
    fn emitter_for(&self, thread_id: Uuid) -> Option<StreamEmitter> {
        if let Some(e) = self
            .stream_emitters
            .read()
            .expect("stream emitter lock poisoned")
            .get(&thread_id)
            .cloned()
        {
            return Some(e);
        }
        self.default_stream_emitter
            .read()
            .expect("stream emitter lock poisoned")
            .clone()
    }

    fn emit(&self, event: StreamEvent) {
        // Persist the agent's state as it is announced.
        //
        // Hooked here rather than at the fifteen places that emit a `Status`,
        // because the two must not be able to disagree: a status that the
        // window shows and a row in the database that says something else is
        // the kind of drift nobody notices until they restart the app. Every
        // meaningful transition already announces itself, so this catches all
        // of them and cannot miss a new one.
        if let StreamEvent::Status { bot_id, state, .. } = &event {
            let status = match state.as_str() {
                "thinking" | "running_tool" | "emulating_tools" => {
                    // The stream distinguishes a tool round from a model round;
                    // the stored column does not need to.
                    if state == "running_tool" {
                        ravenbot_core::BotStatus::RunningTool
                    } else {
                        ravenbot_core::BotStatus::Thinking
                    }
                }
                "waiting_on_user" => ravenbot_core::BotStatus::WaitingOnUser,
                "paused" => ravenbot_core::BotStatus::Paused,
                // "done" and anything else means the agent is available again.
                _ => ravenbot_core::BotStatus::Idle,
            };
            self.mark_bot_status(*bot_id, status);
        }
        if let Some(emitter) = self.emitter_for(stream_event_thread_id(&event)) {
            emitter(event);
        }
    }

    /// Callback that forwards streamed model text to the UI for one run.
    fn delta_emitter(&self, bot_id: Uuid, thread_id: Uuid) -> DeltaCallback {
        let emitter_snapshot: Option<StreamEmitter> = self.emitter_for(thread_id);
        Arc::new(move |content: &str| {
            if let Some(emitter) = &emitter_snapshot {
                emitter(StreamEvent::Delta {
                    bot_id,
                    thread_id,
                    content: content.to_string(),
                });
            }
        })
    }

    /// Get the provider manager
    pub fn provider_manager(&self) -> &Arc<Mutex<ProviderManager>> {
        &self.provider_manager
    }

    /// Get the skill registry
    pub fn skill_registry(&self) -> &Arc<SkillRegistry> {
        &self.skill_registry
    }

    /// Get the plugin registry (1000+ native)
    pub fn plugin_registry(&self) -> &Arc<PluginRegistry> {
        &self.plugin_registry
    }

    /// Get the MCP registry (60+ servers as native)
    pub fn mcp_registry(&self) -> &Arc<McpRegistry> {
        &self.mcp_registry
    }

    /// Get the kill switch
    pub fn kill_switch(&self) -> &Arc<KillSwitch> {
        &self.kill_switch
    }

    /// Get the memory store
    pub fn memory_store(&self) -> &Arc<MemoryStore> {
        &self.memory_store
    }

    /// Get the memory retriever
    pub fn memory_retriever(&self) -> &Arc<MemoryRetriever> {
        &self.memory_retriever
    }

    /// Get the self reviewer
    pub fn self_reviewer(&self) -> &Arc<SelfReviewer> {
        &self.self_reviewer
    }

    /// Get office memory
    pub fn office_memory(&self) -> &Arc<OfficeMemoryStore> {
        &self.office_memory
    }

    /// Get learning engine (makes agents smarter daily)
    pub fn learning(&self) -> &Arc<LearningEngine> {
        &self.learning
    }

    /// Get the budget manager
    pub fn budget_manager(&self) -> &Arc<BudgetManager> {
        &self.budget_manager
    }

    /// Get the audit logger
    pub fn audit_logger(&self) -> &Arc<AuditLogger> {
        &self.audit_logger
    }

    /// Get the version control
    pub fn version_control(&self) -> &Arc<PromptVersionControl> {
        &self.version_control
    }

    /// Runtime-native delegation: actually run the target bot and return its
    /// answer (the registry stub only reported "delegation_initiated").
    async fn exec_delegation(
        &self,
        parent_run: &Run,
        args: serde_json::Value,
    ) -> ravenbot_skills::SkillResult {
        let bot_id_str = args
            .get("bot_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let instruction = args
            .get("instruction")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .trim()
            .to_string();
        if instruction.is_empty() {
            return ravenbot_skills::SkillResult::failure("Missing 'instruction' field");
        }
        // The tool schema advertises a `context` field, so a caller reasonably
        // puts the background there. It used to be bound to `_context_text` and
        // dropped on the floor, so the target received the instruction with no
        // idea who was asking or why. Prepended here, and reported back.
        let context = args
            .get("context")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .trim()
            .to_string();

        // Depth guard: prevent recursive delegation loops
        let depth = {
            let map = self.delegation_depth.read().expect("delegation depth lock");
            map.get(&parent_run.id).copied().unwrap_or(0)
        };
        const MAX_DELEGATION_DEPTH: u32 = 3;
        if depth >= MAX_DELEGATION_DEPTH {
            return ravenbot_skills::SkillResult::failure(format!(
                "Delegation too deep (depth {} > {}); resolve this task directly instead of delegating again",
                depth, MAX_DELEGATION_DEPTH
            ));
        }

        // Resolve the target bot: by id, or by name
        let target = match Uuid::parse_str(bot_id_str) {
            Ok(id) => ravenbot_db::queries::BotQueries::get(self.db.pool(), id)
                .await
                .ok()
                .flatten(),
            Err(_) => {
                let bots = ravenbot_db::queries::BotQueries::list(self.db.pool())
                    .await
                    .unwrap_or_default();
                bots.into_iter()
                    .find(|b| b.name.eq_ignore_ascii_case(bot_id_str))
            }
        };
        let Some(target) = target else {
            return ravenbot_skills::SkillResult::failure(format!(
                "Delegation target bot not found: {}",
                bot_id_str
            ));
        };

        if self.kill_switch.is_triggered().await {
            return ravenbot_skills::SkillResult::failure("Kill switch active — delegation paused");
        }

        // Who is allowed to hand work to whom.
        //
        // `delegate_to` was written to the database, round-tripped, and never
        // read, and target resolution fell back to a case-insensitive name
        // search across every bot in the database — so any agent could hand work
        // to any other, including one in a different office. A lead may dispatch
        // within its own roster; anyone else may only use their own list.
        let caller = ravenbot_db::queries::BotQueries::get(self.db.pool(), parent_run.bot_id)
            .await
            .ok()
            .flatten();
        let office = self.office_of(parent_run.thread_id).await;
        let roster: Vec<Uuid> = match &office {
            Some(room) => ravenbot_db::queries::ChatRoomQueries::list_members(self.db.pool(), room.id)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|m| m.bot_id)
                .collect(),
            None => Vec::new(),
        };
        if let Some(caller) = &caller {
            if let Err(denial) = ravenbot_core::delegation_permitted(
                caller,
                target.id,
                &target.name,
                &roster,
            ) {
                return ravenbot_skills::SkillResult::failure(denial.explain());
            }
        }

        // Run the instruction through the real runtime in a fresh thread
        let thread = ravenbot_core::Thread::new(
            target.id,
            format!("Delegation: {}", instruction.chars().take(30).collect::<String>()),
        );
        if let Err(e) = ravenbot_db::queries::ThreadQueries::create(self.db.pool(), &thread).await {
            return ravenbot_skills::SkillResult::failure(e.to_string());
        }

        // A delegated agent must stay in the office.
        //
        // Without this the child thread has no `chatroom_threads` row, so
        // `resolve_working_dirs` walks the whole priority chain and lands on
        // `default_project_dir(bot.name)` — the target is moved out of the
        // office into a folder of its own, with no goal, no policy, no roster
        // and no office memory, and the work it returns has been done in the
        // wrong place. The office's own folders are stamped on the thread
        // exactly as the graph executor stamps them for a parallel node.
        if let Some(office) = self.office_of(parent_run.thread_id).await {
            if let Err(e) = self.link_thread_to_office(&thread, &office).await {
                return ravenbot_skills::SkillResult::failure(e);
            }
        }

        // The target sees who is asking and why, not a bare instruction.
        let prompt = if context.is_empty() {
            instruction.clone()
        } else {
            format!("{context}\n\n---\n\nTask:\n{instruction}")
        };
        let user_msg = ravenbot_core::Message::user(thread.id, &prompt);
        if let Err(e) = ravenbot_db::queries::MessageQueries::insert(self.db.pool(), &user_msg).await {
            return ravenbot_skills::SkillResult::failure(e.to_string());
        }
        let mut child_run = Run::new(target.id, thread.id);
        if let Err(e) = ravenbot_db::queries::RunQueries::insert(self.db.pool(), &child_run).await {
            return ravenbot_skills::SkillResult::failure(e.to_string());
        }

        {
            let mut map = self.delegation_depth.write().expect("delegation depth lock");
            map.insert(child_run.id, depth + 1);
        }

        // Box the recursive call (delegation → run → tool → delegation…)
        let exec_result = Box::pin(self.execute_run(&mut child_run)).await;

        let response_text = match ravenbot_db::queries::MessageQueries::list_by_thread(self.db.pool(), thread.id).await {
            Ok(messages) => messages
                .iter()
                .rev()
                .find(|m| matches!(m.role, ravenbot_core::MessageRole::Assistant))
                .and_then(|m| match &m.content {
                    ravenbot_core::MessageContent::Text { text, .. } => Some(text.clone()),
                    _ => None,
                })
                .unwrap_or_default(),
            Err(_) => String::new(),
        };

        match exec_result {
            Ok(()) => ravenbot_skills::SkillResult::success(serde_json::json!({
                "status": "completed",
                "target_bot": target.name,
                "target_bot_id": target.id.to_string(),
                "thread_id": thread.id.to_string(),
                "result": response_text
            })),
            Err(e) => ravenbot_skills::SkillResult::failure(format!(
                "Delegation to '{}' failed: {}",
                target.name, e
            )),
        }
    }

    /// Record a tool call the agent's own grant refused.
    ///
    /// A refusal that leaves no trace is indistinguishable from a tool that was
    /// never called, and the first question anyone would ask about an agent
    /// "broke" this way is what did it.
    async fn audit_denied_tool(
        &self,
        bot: &ravenbot_core::Bot,
        run: &Run,
        tool: &str,
        denial: &ravenbot_core::Denial,
    ) {
        if let Err(e) = self
            .audit_logger
            .log_tool_call(
                bot.id,
                Some(run.id),
                Some(run.thread_id),
                tool,
                serde_json::json!({ "denied": true, "reason": denial.explain() }),
            )
            .await
        {
            tracing::warn!(error = %e, "Could not audit a denied tool call");
        }
    }

    /// The office a thread belongs to, if any.
    async fn office_of(&self, thread_id: Uuid) -> Option<ravenbot_core::ChatRoom> {
        let chatroom_id: Option<String> =
            sqlx::query_scalar("SELECT chatroom_id FROM chatroom_threads WHERE thread_id = ?")
                .bind(thread_id.to_string())
                .fetch_optional(self.db.pool())
                .await
                .ok()
                .flatten();
        let cid = Uuid::parse_str(&chatroom_id?).ok()?;
        ravenbot_db::queries::ChatRoomQueries::get(self.db.pool(), cid)
            .await
            .ok()
            .flatten()
    }

    /// Put a newly created thread inside `office`: linked to the room, and
    /// carrying the room's project folders so the runtime confines it there.
    ///
    /// This is the same treatment the graph executor gives a node's child
    /// thread. Anything that runs work *for* an office — a parallel node, or a
    /// delegated agent — has to be in the room, or it silently works somewhere
    /// else and reports back having done so.
    async fn link_thread_to_office(
        &self,
        thread: &ravenbot_core::Thread,
        office: &ravenbot_core::ChatRoom,
    ) -> Result<(), String> {
        sqlx::query("INSERT OR REPLACE INTO chatroom_threads (chatroom_id, thread_id, created_at) VALUES (?, ?, ?)")
            .bind(office.id.to_string())
            .bind(thread.id.to_string())
            .bind(chrono::Utc::now().to_rfc3339())
            .execute(self.db.pool())
            .await
            .map_err(|e| format!("Could not link the delegated thread to the office: {e}"))?;

        if !office.project_folders.is_empty() {
            let folders = serde_json::to_string(&office.project_folders)
                .map_err(|e| e.to_string())?;
            sqlx::query("UPDATE threads SET project_folders = ? WHERE id = ?")
                .bind(folders)
                .bind(thread.id.to_string())
                .execute(self.db.pool())
                .await
                .map_err(|e| format!("Could not set the delegated thread's workspace: {e}"))?;
        }
        Ok(())
    }

    /// Resolve a tool name to its approval risk level.
    /// Order: assembled per-run skills -> global registry -> MCP tools are
    /// High (third-party code, fail-closed) -> runtime-native memory/delegate
    /// -> unknown names are High (fail-closed).
    async fn resolve_tool_risk(
        &self,
        tool_name: &str,
        tool_skills: &[std::sync::Arc<dyn ravenbot_skills::Skill>],
    ) -> ravenbot_skills::SkillRisk {
        #[allow(unused_imports)]
        use ravenbot_skills::Skill;
        if tool_name == "memory_save" {
            return ravenbot_skills::SkillRisk::Low;
        }
        if tool_name == "memory_recall" {
            return ravenbot_skills::SkillRisk::ReadOnly;
        }
        if tool_name == "delegate" {
            return ravenbot_skills::SkillRisk::High;
        }
        if let Some(skill) = tool_skills.iter().find(|s| s.id() == tool_name) {
            return skill.risk();
        }
        if let Some(skill) = self.skill_registry.get(tool_name) {
            return skill.risk();
        }
        // MCP + unknown: fail closed.
        ravenbot_skills::SkillRisk::High
    }

    /// Human label for an approval card header ("Run a command", ...).
    fn tool_label_for(tool_name: &str) -> String {
        let bare = tool_name
            .trim_start_matches("mcp__")
            .replace("__", " ")
            .replace('_', " ");
        match tool_name {
            "shell_exec" => "Run a command".to_string(),
            "file_read" => "Read a file".to_string(),
            "file_write" => "Write a file".to_string(),
            "file_tree" => "List files".to_string(),
            "code_search" => "Search code".to_string(),
            "code_edit" => "Edit code".to_string(),
            "git" => "Run git".to_string(),
            "docker" => "Run docker".to_string(),
            "browser_navigate" => "Open a web page".to_string(),
            "http_request" => "Call an API".to_string(),
            "db_query" => "Query the database".to_string(),
            "delegate" => "Delegate to another bot".to_string(),
            "computer_control" => "Control your computer".to_string(),
            "memory_save" => "Save a memory".to_string(),
            "image_gen" => "Generate an image".to_string(),
            _ => format!("Use {}", bare),
        }
    }

    /// Park the run on a pending approval and block until the user decides.
    /// Returns Ok(true) = allowed, Ok(false) = denied.
    /// Fail-closed: anything abnormal (missing row, stale decision, timeout)
    /// denies the tool rather than running it silent.
    async fn request_approval(
        &self,
        run: &mut Run,
        bot: &ravenbot_core::Bot,
        tool_name: &str,
        arguments: &serde_json::Value,
        risk: ravenbot_skills::SkillRisk,
    ) -> Result<bool, RuntimeError> {
        use ravenbot_skills::SkillRisk;
        let risk_str = match risk {
            SkillRisk::ReadOnly => "read_only",
            SkillRisk::Low => "low",
            SkillRisk::High => "high",
        };
        let req = ravenbot_core::ApprovalRequest::pending(
            bot.id,
            run.thread_id,
            run.id,
            tool_name,
            Self::tool_label_for(tool_name),
            arguments.clone(),
            risk_str,
        );
        ravenbot_db::queries::ApprovalQueries::create(self.db.pool(), &req)
            .await
            .map_err(RuntimeError::Sql)?;

        // Headless (CLI / graph node / routine): nobody can answer, so
        // auto-allow with a clear audit trail instead of parking forever.
        if self.auto_allow_for(run.id) {
            let _ = ravenbot_db::queries::ApprovalQueries::decide(
                self.db.pool(), req.id, true, Some("auto-allowed: headless run"),
            ).await;
            self.emit(StreamEvent::ApprovalDecided {
                thread_id: run.thread_id,
                approval_id: req.id,
                allowed: true,
            });
            return Ok(true);
        }

        // Park the run visibly: status event so the sidebar leaves
        // "running_tool" and the composer can block on the decision.
        self.emit(StreamEvent::Status {
            bot_id: bot.id,
            thread_id: run.thread_id,
            state: "waiting_on_user".to_string(),
        });
        self.emit(StreamEvent::ApprovalRequested {
            bot_id: bot.id,
            thread_id: run.thread_id,
            approval: req.clone(),
        });
        let _ = self.audit_logger.log_tool_call(
            bot.id,
            Some(run.id),
            Some(run.thread_id),
            &format!("approval_requested:{}", tool_name),
            arguments.clone(),
        ).await;

        // Poll the row: short interval, bounded wait (10 min). The UI decides
        // via decide_approval; expiry denies.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(600);
        loop {
            if self.kill_switch.is_triggered().await {
                let _ = ravenbot_db::queries::ApprovalQueries::decide(
                    self.db.pool(), req.id, false, Some("run paused"),
                ).await;
                self.emit(StreamEvent::ApprovalDecided {
                    thread_id: run.thread_id,
                    approval_id: req.id,
                    allowed: false,
                });
                return Err(RuntimeError::KillSwitchActive(
                    "Kill switch triggered while waiting on approval".to_string(),
                ));
            }
            let current = ravenbot_db::queries::ApprovalQueries::get(self.db.pool(), req.id)
                .await
                .map_err(RuntimeError::Sql)?;
            match current.map(|r| r.status) {
                Some(ravenbot_core::ApprovalStatus::Allowed) => {
                    self.emit(StreamEvent::ApprovalDecided {
                        thread_id: run.thread_id,
                        approval_id: req.id,
                        allowed: true,
                    });
                    self.emit(StreamEvent::Status {
                        bot_id: bot.id,
                        thread_id: run.thread_id,
                        state: "running_tool".to_string(),
                    });
                    return Ok(true);
                }
                Some(ravenbot_core::ApprovalStatus::Denied)
                | Some(ravenbot_core::ApprovalStatus::Expired)
                | None => {
                    self.emit(StreamEvent::ApprovalDecided {
                        thread_id: run.thread_id,
                        approval_id: req.id,
                        allowed: false,
                    });
                    return Ok(false);
                }
                _ => {}
            }
            if std::time::Instant::now() >= deadline {
                let _ = ravenbot_db::queries::ApprovalQueries::decide(
                    self.db.pool(), req.id, false, Some("approval timed out"),
                ).await;
                self.emit(StreamEvent::ApprovalDecided {
                    thread_id: run.thread_id,
                    approval_id: req.id,
                    allowed: false,
                });
                return Ok(false);
            }
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        }
    }

    /// Dispatch one tool call to its executor: runtime-native tools first
    /// (memory/todo/vision/delegation/ask_user), then per-bot assembled
    /// skills, the global registry, and finally dynamic MCP resolution.
    async fn execute_tool_call(
        &self,
        bot: &ravenbot_core::Bot,
        run: &Run,
        tool_skills: &[Arc<dyn ravenbot_skills::Skill>],
        skill_context: &SkillContext,
        name: &str,
        args: &serde_json::Value,
    ) -> serde_json::Value {
        // The capability gate.
        //
        // This is the check that did not exist. A skill declared what it needed
        // via `required_permissions()`, a bot carried a `permissions` list, and
        // the two were never compared — so the list was decorative metadata and
        // the only thing standing between a tool and the filesystem was asking
        // the user, which is a different question. A refusal here is the
        // agent's own grant, not the operator's patience, and it does not
        // consume an approval.
        let needed: Vec<ravenbot_core::Permission> = tool_skills
            .iter()
            .find(|s| s.id() == name)
            .map(|s| s.required_permissions())
            .or_else(|| self.skill_registry.get(name).map(|s| s.required_permissions()))
            .unwrap_or_default();
        if !needed.is_empty() {
            if let Err(denial) = ravenbot_core::tool_permitted(bot, name, &needed) {
                self.audit_denied_tool(bot, run, name, &denial).await;
                return serde_json::json!({ "error": denial.explain() });
            }
        }

        let result: Result<ravenbot_skills::SkillResult, ravenbot_skills::SkillError> =
            if name == "ask_user" {
                return self.ask_user(run, bot, args.clone()).await;
            } else if name == "delegate" {
                Ok(self.exec_delegation(run, args.clone()).await)
            } else if name == "memory_save" {
                Ok(self.exec_memory_save(bot.id, args.clone()).await)
            } else if name == "memory_recall" {
                Ok(self.exec_memory_recall(bot.id, args.clone()).await)
            } else if name == "todo" {
                Ok(self.exec_todo(bot.id, args.clone()).await)
            } else if name == "analyze_image" {
                Ok(self.exec_analyze_image(bot.id, args.clone()).await)
            } else if name == "computer_control" {
                // Host-control safety gate: per-agent opt-in + platform policy.
                // Wayland is fail-closed unless explicitly overridden.
                if !bot.config.host_control {
                    Ok(ravenbot_skills::SkillResult::failure(
                        "Host control is OFF for this agent. Enable 'Control this computer' in the agent's settings first.",
                    ))
                } else if host_control_policy() == "blocked" {
                    Ok(ravenbot_skills::SkillResult::failure(
                        "Host control is blocked on this Wayland session (safety gate). Use the agent's isolated desktop instead, or set RAVENBOT_ALLOW_WAYLAND_CONTROL=1 to override.",
                    ))
                } else if let Some(skill) = tool_skills
                    .iter()
                    .find(|s| s.id() == name)
                    .cloned()
                    .or_else(|| self.skill_registry.get(name))
                {
                    skill.execute(skill_context, args.clone()).await
                } else {
                    Ok(ravenbot_skills::SkillResult::failure(
                        "computer_control skill is unavailable",
                    ))
                }
            } else if let Some(skill) = tool_skills.iter().find(|s| s.id() == name) {
                skill.execute(skill_context, args.clone()).await
            } else if let Some(skill) = self.skill_registry.get(name) {
                skill.execute(skill_context, args.clone()).await
            } else if let Ok(Some((cfg, env))) = self.mcp_registry.resolve_tool(name).await {
                let client = ravenbot_mcp::client::McpClient::with_env(cfg, env);
                match client.call_tool(name, args.clone()).await {
                    Ok(v) => Ok(ravenbot_skills::SkillResult::success(v)),
                    Err(e) => Err(ravenbot_skills::SkillError::Execution(e)),
                }
            } else {
                self.skill_registry.execute(name, skill_context, args.clone()).await
            };

        match result {
            Ok(r) => serde_json::to_value(r).unwrap_or_default(),
            Err(e) => serde_json::json!({ "error": e.to_string() }),
        }
    }

    /// Human-in-the-loop `ask_user`: park the run on a question card and wait
    /// for the user's answer (or a timeout). Headless runs skip it rather than
    /// blocking forever. Returns a tool-result JSON with the answer.
    async fn ask_user(
        &self,
        run: &Run,
        bot: &ravenbot_core::Bot,
        args: serde_json::Value,
    ) -> serde_json::Value {
        let question = args
            .get("question")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if question.is_empty() {
            return serde_json::json!({ "error": "ask_user requires a non-empty 'question'" });
        }
        let header = args
            .get("header")
            .and_then(|v| v.as_str())
            .unwrap_or("Question")
            .to_string();
        let options: Vec<String> = args
            .get("options")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let allow_custom = args
            .get("allow_custom")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        // Headless (CLI / routines / graph nodes): nobody can answer.
        if self.auto_allow_for(run.id) {
            return serde_json::json!({
                "answer": null,
                "skipped": true,
                "note": "No user is available to answer (headless run). Proceed using your best judgement.",
            });
        }

        let req = ravenbot_core::QuestionRequest::pending(
            bot.id,
            run.thread_id,
            run.id,
            header,
            question,
            options,
            allow_custom,
        );
        if let Err(e) = ravenbot_db::queries::QuestionQueries::create(self.db.pool(), &req).await {
            return serde_json::json!({ "error": format!("failed to record question: {}", e) });
        }
        self.emit(StreamEvent::QuestionAsked {
            bot_id: bot.id,
            thread_id: run.thread_id,
            question: req.clone(),
        });
        let _ = self.audit_logger.log_tool_call(
            bot.id,
            Some(run.id),
            Some(run.thread_id),
            "ask_user",
            serde_json::json!({ "question": req.question }),
        ).await;

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(600);
        loop {
            if self.kill_switch.is_triggered().await {
                let _ = ravenbot_db::queries::QuestionQueries::expire(self.db.pool(), req.id).await;
                return serde_json::json!({ "error": "run interrupted before the user answered" });
            }
            match ravenbot_db::queries::QuestionQueries::get(self.db.pool(), req.id).await {
                Ok(Some(current))
                    if current.status == ravenbot_core::QuestionStatus::Answered =>
                {
                    self.emit(StreamEvent::QuestionAnswered {
                        thread_id: run.thread_id,
                        question_id: req.id,
                        answer: current.answer.clone().unwrap_or_default(),
                    });
                    return serde_json::json!({
                        "answer": current.answer,
                        "question": current.question,
                    });
                }
                Ok(Some(current))
                    if current.status == ravenbot_core::QuestionStatus::Expired =>
                {
                    return serde_json::json!({
                        "answer": null,
                        "timed_out": true,
                        "note": "The user did not answer in time. Proceed using your best judgement.",
                    });
                }
                Ok(None) => {
                    return serde_json::json!({ "error": "question record disappeared" });
                }
                _ => {}
            }
            if std::time::Instant::now() >= deadline {
                let _ = ravenbot_db::queries::QuestionQueries::expire(self.db.pool(), req.id).await;
                return serde_json::json!({
                    "answer": null,
                    "timed_out": true,
                    "note": "The user did not answer in time. Proceed using your best judgement.",
                });
            }
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        }
    }

    /// Trigger the kill switch (also interrupts any live engine processes)
    pub async fn trigger_kill_switch(&self, reason: impl Into<String>) {
        self.kill_switch.trigger(reason).await;
        if let Ok(tokens) = self.engine_cancels.lock() {
            for token in tokens.values() {
                token.cancel();
            }
        }
    }

    /// Runtime-native memory_save: real vector-store persistence
    /// (the registry stub only echoed the arguments back)
    async fn exec_memory_save(
        &self,
        bot_id: Uuid,
        args: serde_json::Value,
    ) -> ravenbot_skills::SkillResult {
        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .trim()
            .to_string();
        if content.is_empty() {
            return ravenbot_skills::SkillResult::failure("Missing 'content' field");
        }
        let importance = args
            .get("importance")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.7) as f32;

        match self.memory_store.add(bot_id, &content, importance).await {
            Ok(fact) => ravenbot_skills::SkillResult::success(serde_json::json!({
                "saved": fact.content,
                "fact_id": fact.id.to_string(),
                "importance": fact.importance,
                "note": "persisted to the bot's vector memory and will be recalled by RAG"
            })),
            Err(e) => ravenbot_skills::SkillResult::failure(e),
        }
    }

    /// Runtime-native memory_recall: real semantic similarity search
    async fn exec_memory_recall(
        &self,
        bot_id: Uuid,
        args: serde_json::Value,
    ) -> ravenbot_skills::SkillResult {
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .trim()
            .to_string();
        if query.is_empty() {
            return ravenbot_skills::SkillResult::failure("Missing 'query' field");
        }
        let limit = args
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(5)
            .clamp(1, 10) as usize;

        match self.memory_store.retrieve(bot_id, &query, limit, 0.1).await {
            Ok(matches) => {
                let memories: Vec<serde_json::Value> = matches
                    .into_iter()
                    .map(|(fact, score)| {
                        serde_json::json!({
                            "content": fact.content,
                            "similarity": (score * 1000.0).round() / 1000.0,
                            "importance": fact.importance
                        })
                    })
                    .collect();
                ravenbot_skills::SkillResult::success(serde_json::json!({
                    "query": query,
                    "memories": memories,
                    "note": "semantic search over the bot's vector memory"
                }))
            }
            Err(e) => ravenbot_skills::SkillResult::failure(e),
        }
    }

    /// Runtime-native todo: DB-backed per-bot list (survives restarts —
    /// the old registry stub kept everything in a process static).
    async fn exec_todo(
        &self,
        bot_id: Uuid,
        args: serde_json::Value,
    ) -> ravenbot_skills::SkillResult {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("list");
        let pool = self.db.pool();
        match action {
            "add" => {
                let task = args.get("task").and_then(|v| v.as_str()).unwrap_or_default().trim().to_string();
                if task.is_empty() {
                    return ravenbot_skills::SkillResult::failure("Missing 'task' field");
                }
                let id = Uuid::new_v4();
                let now = chrono::Utc::now().to_rfc3339();
                if let Err(e) = sqlx::query(
                    "INSERT INTO bot_todos (id, bot_id, task, done, created_at, updated_at) VALUES (?, ?, ?, 0, ?, ?)",
                )
                .bind(id.to_string()).bind(bot_id.to_string()).bind(&task).bind(&now).bind(&now)
                .execute(pool).await
                {
                    return ravenbot_skills::SkillResult::failure(e.to_string());
                }
                let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM bot_todos WHERE bot_id = ? AND done = 0")
                    .bind(bot_id.to_string()).fetch_one(pool).await.unwrap_or((0,));
                ravenbot_skills::SkillResult::success(serde_json::json!({"added": task, "id": id.to_string(), "open": count}))
            }
            "list" => {
                let rows: Vec<(String, String, i64)> = sqlx::query_as(
                    "SELECT id, task, done FROM bot_todos WHERE bot_id = ? ORDER BY created_at ASC",
                )
                .bind(bot_id.to_string()).fetch_all(pool).await.unwrap_or_default();
                let todos: Vec<serde_json::Value> = rows.into_iter().map(|(id, task, done)| {
                    serde_json::json!({"id": id, "task": task, "done": done != 0})
                }).collect();
                ravenbot_skills::SkillResult::success(serde_json::json!({"todos": todos}))
            }
            "done" => {
                let id = args.get("id").and_then(|v| v.as_str()).unwrap_or_default();
                if id.is_empty() {
                    return ravenbot_skills::SkillResult::failure("Missing 'id' field");
                }
                let res = sqlx::query("UPDATE bot_todos SET done = 1, updated_at = ? WHERE id = ? AND bot_id = ?")
                    .bind(chrono::Utc::now().to_rfc3339()).bind(id).bind(bot_id.to_string())
                    .execute(pool).await;
                match res {
                    Ok(r) if r.rows_affected() > 0 => ravenbot_skills::SkillResult::success(serde_json::json!({"done": id})),
                    Ok(_) => ravenbot_skills::SkillResult::failure("Todo not found"),
                    Err(e) => ravenbot_skills::SkillResult::failure(e.to_string()),
                }
            }
            "clear" => {
                if let Err(e) = sqlx::query("DELETE FROM bot_todos WHERE bot_id = ? AND done = 1")
                    .bind(bot_id.to_string()).execute(pool).await
                {
                    return ravenbot_skills::SkillResult::failure(e.to_string());
                }
                ravenbot_skills::SkillResult::success(serde_json::json!({"cleared": true}))
            }
            _ => ravenbot_skills::SkillResult::failure(format!("Unknown action: {}", action)),
        }
    }

    /// Runtime-native analyze_image: sends the image to the bot's own
    /// vision provider with a describe prompt. Stub returned canned text
    /// before; now it actually sees.
    async fn exec_analyze_image(
        &self,
        bot_id: Uuid,
        args: serde_json::Value,
    ) -> ravenbot_skills::SkillResult {
        use base64::Engine;
        let image_data = args.get("image_data").and_then(|v| v.as_str()).unwrap_or_default();
        if image_data.is_empty() {
            return ravenbot_skills::SkillResult::failure("Missing 'image_data' field");
        }
        let image_bytes = match general_purpose::STANDARD.decode(image_data) {
            Ok(b) => b,
            Err(e) => return ravenbot_skills::SkillResult::failure(format!("Invalid base64: {}", e)),
        };
        // Downscale to keep prompt size reasonable (≤1568px longest edge).
        let resized = match ravenbot_vision::resize_for_vision(&image_bytes, 1568) {
            Ok(b) => b,
            Err(_) => image_bytes,
        };
        let b64 = general_purpose::STANDARD.encode(&resized);

        let question = args.get("question").and_then(|v| v.as_str())
            .unwrap_or("Describe this image concisely — what it shows, any text, and notable UI elements.");

        // Build a vision message.
        let msg = ravenbot_models::Message::text("user", question).with_images(vec![
            ravenbot_models::MessageImage { data: b64, mime: "image/png".to_string() },
        ]);

        // Use the bot's configured provider.
        let bot = match ravenbot_db::queries::BotQueries::get(self.db.pool(), bot_id).await {
            Ok(Some(b)) => b,
            _ => return ravenbot_skills::SkillResult::failure("Bot not found"),
        };
        let provider: Arc<dyn ravenbot_models::ModelProviderTrait> = {
            let manager = self.provider_manager.lock().await;
            match manager.create_provider_from_str_with_model(
                &bot.config.model_provider,
                Some(&bot.config.model_id),
            ) {
                Ok(p) => Arc::from(p),
                Err(e) => return ravenbot_skills::SkillResult::failure(e.to_string()),
            }
        };

        match provider.complete(&[msg], &[], 0.3, 500).await {
            Ok(resp) => {
                let text = resp.content.unwrap_or_default();
                ravenbot_skills::SkillResult::success(serde_json::json!({
                    "description": text,
                    "provider": bot.config.model_provider,
                    "model": bot.config.model_id,
                    "note": "analyzed by the bot's own vision provider"
                }))
            }
            Err(e) => ravenbot_skills::SkillResult::failure(format!("Vision analysis failed: {}", e)),
        }
    }

    /// Release the kill switch
    pub async fn release_kill_switch(&self) {
        self.kill_switch.release().await;
    }

    /// Check if kill switch is active
    pub async fn is_paused(&self) -> bool {
        self.kill_switch.is_triggered().await
    }

    /// Project folders this run may work in, in priority order:
    /// per-bot override → thread folders → office project folders → channel
    /// working folder → an auto-created default folder.
    ///
    /// The per-bot override comes first on purpose. `threads.project_folders`
    /// is stamped by the office graph executor, so it is the office's folders
    /// being pushed down to a child run rather than a choice anyone made; if
    /// it outranked the override, a per-bot working folder would be silently
    /// dead inside every office run.
    ///
    /// The result is never empty. A bot always has a folder it can write in,
    /// so file tools fail with "no such directory" rather than a
    /// permission error the agent cannot act on.
    async fn resolve_working_dirs(
        &self,
        bot: &ravenbot_core::Bot,
        thread_id: Uuid,
    ) -> Vec<std::path::PathBuf> {
        let mut dirs: Vec<String> = Vec::new();

        // 1) Per-bot override.
        if let Some(dir) = bot
            .config
            .working_folder
            .as_deref()
            .map(str::trim)
            .filter(|d| !d.is_empty())
        {
            dirs.push(dir.to_string());
        }

        // 2) Thread-level folders (the office graph stamps these onto each
        //    node's child thread).
        if let Ok(Some(json)) = sqlx::query_scalar::<_, Option<String>>(
            "SELECT project_folders FROM threads WHERE id = ?",
        )
        .bind(thread_id.to_string())
        .fetch_optional(self.db.pool())
        .await
        .map(|row| row.flatten())
        {
            if let Ok(parsed) = serde_json::from_str::<Vec<String>>(&json) {
                dirs.extend(parsed);
            }
        }

        let chatroom_id: Option<Uuid> = sqlx::query_scalar::<_, String>(
            "SELECT chatroom_id FROM chatroom_threads WHERE thread_id = ?",
        )
        .bind(thread_id.to_string())
        .fetch_optional(self.db.pool())
        .await
        .ok()
        .flatten()
        .and_then(|s| Uuid::parse_str(&s).ok());

        // 3) Office project folders (the thread's chatroom).
        if let Some(cid) = chatroom_id {
            if let Ok(Some(room)) = ravenbot_db::queries::ChatRoomQueries::get(self.db.pool(), cid).await {
                dirs.extend(room.project_folders);
            }
        }

        // 4) Channel working folder.
        if dirs.is_empty() {
            let channel_id: Option<Uuid> =
                sqlx::query_scalar::<_, Option<String>>("SELECT channel_id FROM threads WHERE id = ?")
                    .bind(thread_id.to_string())
                    .fetch_optional(self.db.pool())
                    .await
                    .ok()
                    .flatten()
                    .flatten()
                    .and_then(|s| Uuid::parse_str(&s).ok());
            if let Some(cid) = channel_id {
                if let Ok(Some(channel)) =
                    ravenbot_db::queries::ChannelQueries::get(self.db.pool(), cid).await
                {
                    if let Some(folder) = channel
                        .working_folder
                        .as_deref()
                        .map(str::trim)
                        .filter(|f| !f.is_empty())
                    {
                        dirs.push(folder.to_string());
                    }
                }
            }
        }

        // Normalize: expand `~`, drop blanks, dedupe, and keep a folder whose
        // existence we can confirm ahead of the others. An empty or stale
        // folder still wins over falling through to a default, because
        // silently re-homing a run into a different directory is worse than a
        // clear "no such directory" from the file tool.
        let mut out: Vec<std::path::PathBuf> = Vec::new();
        for raw in dirs {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }
            let path = expand_home(trimmed);
            if !out.contains(&path) {
                out.push(path);
            }
        }
        if !out.is_empty() {
            return out;
        }

        // 5) Default folder, created on demand, named after the office/bot.
        let office_name: Option<String> = match chatroom_id {
            Some(cid) => sqlx::query_scalar::<_, String>("SELECT name FROM chatrooms WHERE id = ?")
                .bind(cid.to_string())
                .fetch_optional(self.db.pool())
                .await
                .ok()
                .flatten(),
            None => None,
        };
        let label = office_name.as_deref().unwrap_or(bot.name.as_str());
        vec![default_project_dir(label)]
    }

    /// Record what an agent is doing in the database.
    ///
    /// The UI derives live state from the event stream, which is right while a
    /// window is open. This is what makes it true the rest of the time: after a
    /// restart, in a second window, or for a user reading the database. It is
    /// fire-and-forget because a status hint failing is never worth failing a
    /// run over.
    fn mark_bot_status(&self, bot_id: Uuid, status: ravenbot_core::BotStatus) {
        let pool = self.db.pool().clone();
        tokio::spawn(async move {
            if let Err(e) = ravenbot_db::queries::BotQueries::mark_active(&pool, bot_id, status).await
            {
                tracing::warn!(%bot_id, error = %e, "Could not record bot status");
            }
        });
    }

    /// Start or resume a run
    pub async fn execute_run(&self, run: &mut Run) -> Result<(), RuntimeError> {
        // Check kill switch first
        if self.kill_switch.is_triggered().await {
            let reason = self.kill_switch.reason().await.unwrap_or_else(|| "Unknown".to_string());
            return Err(RuntimeError::KillSwitchActive(reason));
        }

        // Load thread to get the ephemeral flag (temporary chats skip memory)
        let thread_row: Option<(bool,)> = sqlx::query_as(
            "SELECT ephemeral FROM threads WHERE id = ?"
        )
        .bind(run.thread_id.to_string())
        .fetch_optional(self.db.pool())
        .await
        .unwrap_or(None);
        let thread_ephemeral = thread_row.map(|(e,)| e).unwrap_or(false);

        // Get the bot for this run
        let bot = ravenbot_db::queries::BotQueries::get(self.db.pool(), run.bot_id)
            .await?
            .ok_or_else(|| RuntimeError::TaskFailed("Bot not found".to_string()))?;

        // Enforce the bot's budget BEFORE spending (safety-critical)
        let budget_check = self.budget_manager.check_budget(bot.id).await
            .map_err(|e| RuntimeError::Model(e.to_string()))?;
        if !budget_check.allowed {
            return Err(RuntimeError::BudgetExceeded(format!(
                "Bot '{}' has exhausted its budget ({}% used). Raise the limit in Settings → Budgets to continue.",
                bot.name, budget_check.percentage_used.round()
            )));
        }

        // Resolve the project folders this run works in (all file/shell work
        // is confined to them; a default folder is auto-created when unset).
        let working_dirs = self.resolve_working_dirs(&bot, run.thread_id).await;

        // Live status: thinking
        self.emit(StreamEvent::Status {
            bot_id: bot.id,
            thread_id: run.thread_id,
            state: "thinking".to_string(),
        });

        // Get the thread for context
        let messages = ravenbot_db::queries::MessageQueries::list_by_thread(self.db.pool(), run.thread_id).await?;

        // Get relevant memories for context
        let last_user_message = messages.iter()
            .rev()
            .find(|m| matches!(m.role, ravenbot_core::MessageRole::User))
            .and_then(|m| match &m.content {
                ravenbot_core::MessageContent::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .unwrap_or("");

        // Small-talk responses should not pay for tool assembly, memory lookup,
        // plugin/MCP warming, or agent orchestration before the first token.
        if ravenbot_engines::is_native(&bot.config.engine)
            && is_simple_conversational_turn(&bot.name, last_user_message, &messages)
            && self
                .execute_simple_conversational_turn(run, &bot, &messages, last_user_message)
                .await?
        {
            return Ok(());
        }

        let memory_context = if thread_ephemeral {
            String::new()
        } else {
            self.memory_retriever.get_context(
                bot.id,
                last_user_message,
                5,
            ).await.unwrap_or_default()
        };

        // External agent engine (Claude Code / Codex / ACP): the CLI owns the
        // tool loop, so bypass the native model↔tool loop entirely and just
        // stream + persist its turn.
        if !ravenbot_engines::is_native(&bot.config.engine) {
            let is_think = last_user_message.contains("[Think]");
            let engine_cwd = working_dirs.first().cloned();
            return self
                .execute_engine_run(
                    run,
                    &bot,
                    &bot.config.engine,
                    &messages,
                    &memory_context,
                    is_think,
                    engine_cwd,
                )
                .await;
        }

        // Build the provider chain (honoring the bot's configured model id and
        // optional fallback provider), or the injected override (tests/dev).
        let providers: Vec<Arc<dyn ModelProviderTrait>> = self.provider_chain(&bot).await?;
        let mut active_provider: usize = 0;

        // Assemble tools with strict priority:
        // 1. User explicitly enabled skills for this bot (bot.skills)
        // 2. MCP servers assigned to this bot (mcp_registry.skills_for_bot)
        // 3. Plugins enabled for this bot (plugin_registry.skills_for_bot)
        // 4. Intent-aware & DeepSearch/Think auto-inclusions
        // 5. Core baseline skills
        // 6. Plugin discovery meta-skills
        let mut assembled_skills: Vec<Arc<dyn ravenbot_skills::Skill>> = Vec::new();
        let mut seen_ids = HashSet::new();

        // Helper closure to push if not seen
        let mut push_skill = |skill: Arc<dyn ravenbot_skills::Skill>| {
            if seen_ids.insert(skill.id().to_string()) {
                assembled_skills.push(skill);
            }
        };

        // Priority 1: User explicitly enabled skills on this bot (bot.skills)
        if !bot.skills.is_empty() {
            for skill_id in &bot.skills {
                if let Some(s) = self.skill_registry.get(skill_id) {
                    push_skill(s);
                }
            }
        }

        // Priority 2: MCP Tools explicitly enabled for this bot (or globally enabled)
        if let Ok(mcp_skills) = self.mcp_registry.skills_for_bot(bot.id).await {
            for s in mcp_skills {
                push_skill(s);
            }
        }

        // Priority 3: Plugins enabled for this bot
        if let Ok(plugin_skills) = self.plugin_registry.skills_for_bot(bot.id).await {
            for s in plugin_skills {
                push_skill(s);
            }
        }

        // Priority 4: DeepSearch and Intent-Driven dynamic inclusions
        let is_deep_search = last_user_message.contains("[DeepSearch]") || 
                             last_user_message.to_lowercase().contains("search the web") ||
                             last_user_message.to_lowercase().contains("google") ||
                             last_user_message.to_lowercase().contains("latest news");
        
        let is_think = last_user_message.contains("[Think]");

        if is_deep_search {
            if let Some(s) = self.skill_registry.get("web_search") { push_skill(s); }
            if let Some(s) = self.skill_registry.get("tavily_search") { push_skill(s); }
            if let Some(s) = self.skill_registry.get("browser_navigate") { push_skill(s); }
        }

        let user_lower = last_user_message.to_lowercase();
        if user_lower.contains("code") || user_lower.contains("git") || user_lower.contains("repo") || user_lower.contains("file") {
            for id in &["file_read", "file_write", "file_tree", "code_search", "code_edit", "git"] {
                if let Some(s) = self.skill_registry.get(id) { push_skill(s); }
            }
        }
        if user_lower.contains("bash") || user_lower.contains("terminal") || user_lower.contains("exec") || user_lower.contains("command") {
            if let Some(s) = self.skill_registry.get("shell_exec") { push_skill(s); }
        }
        if user_lower.contains("docker") || user_lower.contains("container") {
            if let Some(s) = self.skill_registry.get("docker") { push_skill(s); }
        }
        if user_lower.contains("database") || user_lower.contains("sql") || user_lower.contains("sqlite") {
            if let Some(s) = self.skill_registry.get("db_query") { push_skill(s); }
        }
        if user_lower.contains("http") || user_lower.contains("api") || user_lower.contains("curl") {
            if let Some(s) = self.skill_registry.get("http_request") { push_skill(s); }
        }
        if user_lower.contains("image") || user_lower.contains("draw") || user_lower.contains("picture") || user_lower.contains("photo") {
            if let Some(s) = self.skill_registry.get("image_gen") { push_skill(s); }
        }
        if user_lower.contains("test") || user_lower.contains("tdd") || user_lower.contains("red green") {
            if let Some(s) = self.skill_registry.get("tdd") { push_skill(s); }
        }
        if user_lower.contains("review") || user_lower.contains("code review") || user_lower.contains("pr") {
            if let Some(s) = self.skill_registry.get("code_review") { push_skill(s); }
        }
        if user_lower.contains("debug") || user_lower.contains("bug") || user_lower.contains("fix") || user_lower.contains("broken") {
            if let Some(s) = self.skill_registry.get("diagnosing_bugs") { push_skill(s); }
        }
        if user_lower.contains("research") || user_lower.contains("investigate") || user_lower.contains("find out") {
            if let Some(s) = self.skill_registry.get("research") { push_skill(s); }
        }
        if user_lower.contains("design") || user_lower.contains("architecture") || user_lower.contains("refactor") {
            if let Some(s) = self.skill_registry.get("codebase_design") { push_skill(s); }
            if let Some(s) = self.skill_registry.get("improve_architecture") { push_skill(s); }
        }
        if user_lower.contains("domain") || user_lower.contains("glossary") || user_lower.contains("terminology") {
            if let Some(s) = self.skill_registry.get("domain_modeling") { push_skill(s); }
        }
        if user_lower.contains("prototype") || user_lower.contains("experiment") || user_lower.contains("spike") {
            if let Some(s) = self.skill_registry.get("prototype") { push_skill(s); }
        }
        if user_lower.contains("merge conflict") || user_lower.contains("conflict") || user_lower.contains("rebase") {
            if let Some(s) = self.skill_registry.get("resolving_merge_conflicts") { push_skill(s); }
        }
        if user_lower.contains("grill") || user_lower.contains("interview me") || user_lower.contains("plan") {
            if let Some(s) = self.skill_registry.get("grilling") { push_skill(s); }
            if let Some(s) = self.skill_registry.get("grill_me") { push_skill(s); }
        }
        if user_lower.contains("handoff") || user_lower.contains("summary") || user_lower.contains("continue later") {
            if let Some(s) = self.skill_registry.get("handoff") { push_skill(s); }
        }
        if user_lower.contains("teach") || user_lower.contains("learn") || user_lower.contains("explain") {
            if let Some(s) = self.skill_registry.get("teach") { push_skill(s); }
        }
        if user_lower.contains("monitor") || user_lower.contains("system") || user_lower.contains("cpu") || user_lower.contains("memory") {
            if let Some(s) = self.skill_registry.get("system_monitor") { push_skill(s); }
        }
        if user_lower.contains("install") || user_lower.contains("package") || user_lower.contains("dependency") || user_lower.contains("npm") || user_lower.contains("pip") {
            if let Some(s) = self.skill_registry.get("package_manager") { push_skill(s); }
        }
        // Real desktop control (gated by the approval broker).
        if user_lower.contains("desktop")
            || user_lower.contains("click")
            || user_lower.contains("mouse")
            || user_lower.contains("keyboard")
            || user_lower.contains("gui")
            || user_lower.contains("on my screen")
            || user_lower.contains("computer")
        {
            if let Some(s) = self.skill_registry.get("computer_control") { push_skill(s); }
        }
        if user_lower.contains("ssh") || user_lower.contains("remote") || user_lower.contains("server") {
            if let Some(s) = self.skill_registry.get("ssh_remote") { push_skill(s); }
        }
        if user_lower.contains("api test") || user_lower.contains("endpoint") || user_lower.contains("rest") || user_lower.contains("graphql") {
            if let Some(s) = self.skill_registry.get("api_tester") { push_skill(s); }
        }
        if user_lower.contains("env") || user_lower.contains("environment") || user_lower.contains(".env") || user_lower.contains("secret") {
            if let Some(s) = self.skill_registry.get("env_manager") { push_skill(s); }
        }
        if user_lower.contains("note") || user_lower.contains("bookmark") || user_lower.contains("save this") {
            if let Some(s) = self.skill_registry.get("note_manager") { push_skill(s); }
        }
        if user_lower.contains("run") || user_lower.contains("build") || user_lower.contains("task") || user_lower.contains("make") {
            if let Some(s) = self.skill_registry.get("task_runner") { push_skill(s); }
        }

        // Vision: user attached an image — equip analysis so the model can inspect it
        let has_image_attachment = messages
            .iter()
            .rev()
            .find(|m| matches!(m.role, ravenbot_core::MessageRole::User))
            .map(|m| {
                m.attachments
                    .iter()
                    .any(|a| a.is_image && a.data.is_some())
            })
            .unwrap_or(false);
        if has_image_attachment {
            if let Some(s) = self.skill_registry.get("analyze_image") {
                push_skill(s);
            }
        }

        // Auto-equip skills matching the agent's specialty or role (especially in offices)
        let specialty_lower = bot.specialty.as_deref().unwrap_or("").to_lowercase();
        if specialty_lower.contains("dev") || specialty_lower.contains("code") || specialty_lower.contains("software") || specialty_lower.contains("backend") || specialty_lower.contains("frontend") {
            for id in &["code_search", "code_edit", "git", "file_read", "file_write", "file_tree", "shell_exec"] {
                if let Some(s) = self.skill_registry.get(id) { push_skill(s); }
            }
        }
        if specialty_lower.contains("infra") || specialty_lower.contains("devops") || specialty_lower.contains("sysadmin") {
            for id in &["shell_exec", "docker", "git", "http_request", "file_read", "file_write"] {
                if let Some(s) = self.skill_registry.get(id) { push_skill(s); }
            }
        }
        if specialty_lower.contains("qa") || specialty_lower.contains("test") {
            for id in &["browser_navigate", "shell_exec", "http_request", "code_search"] {
                if let Some(s) = self.skill_registry.get(id) { push_skill(s); }
            }
        }
        if specialty_lower.contains("research") || specialty_lower.contains("lead") || specialty_lower.contains("architect") {
            for id in &["web_search", "tavily_search", "arxiv_search", "code_search", "memory_recall"] {
                if let Some(s) = self.skill_registry.get(id) { push_skill(s); }
            }
        }
        if specialty_lower.contains("design") || specialty_lower.contains("ui") || specialty_lower.contains("ux") {
            for id in &["browser_navigate", "screenshot", "analyze_image"] {
                if let Some(s) = self.skill_registry.get(id) { push_skill(s); }
            }
        }

        // Priority 5: Foundational baseline skills
        let default_core = [
            "web_search", "file_read", "file_write", "file_tree", 
            "shell_exec", "code_search", "code_edit", "git", 
            "http_request", "memory_save", "memory_recall"
        ];
        for id in &default_core {
            if let Some(s) = self.skill_registry.get(id) {
                push_skill(s);
            }
        }

        // Priority 6: Plugin meta tools for runtime app discovery
        for s in self.plugin_registry.meta_skills() {
            push_skill(s);
        }

        // Built-ins win over MCP tools with the same name: MCP *synthesized*
        // tool lists can shadow real built-ins (e.g. `browserbase` synthesizes
        // `browser_navigate`, which would replace the real built-in skill with
        // a fabricated fallback when the MCP server can't spawn).
        let shadowed_ids: Vec<String> = assembled_skills
            .iter()
            .map(|s| s.id().to_string())
            .filter(|id| self.skill_registry.get(id).is_some())
            .collect();
        if !shadowed_ids.is_empty() {
            assembled_skills.retain(|s| !shadowed_ids.contains(&s.id().to_string()));
            for id in shadowed_ids {
                if let Some(builtin) = self.skill_registry.get(&id) {
                    assembled_skills.push(builtin);
                }
            }
        }

        // Separate tool skills from workflow skills.
        // Workflow skills (prompt-based) are injected into the system prompt
        // and do NOT count toward the tool cap.
        let (tool_skills, workflow_skills): (Vec<_>, Vec<_>) = assembled_skills
            .into_iter()
            .partition(|s| s.kind() == SkillKind::Tool);

        // Cap the tool list to keep the context bounded — but NEVER drop a
        // skill the user explicitly enabled or a foundational core skill.
        // Previously the list was truncated purely in assembly order, and
        // because MCP/plugin tools are assembled *before* the core set, a bot
        // with a few connectors assigned silently lost file_read/file_write/
        // shell_exec/git access. Protected tools are always kept.
        const PROTECTED_NATIVE_TOOLS: &[&str] = &[
            "memory_save", "memory_recall", "todo", "ask_user", "delegate", "analyze_image",
        ];
        const TOOL_CAP: usize = 40;
        let protected: HashSet<String> = bot
            .skills
            .iter()
            .cloned()
            .chain(default_core.iter().map(|s| s.to_string()))
            .chain(PROTECTED_NATIVE_TOOLS.iter().map(|s| s.to_string()))
            .collect();
        let tool_skills = cap_preserving_protected(tool_skills, &protected, TOOL_CAP, |s| {
            s.id().to_string()
        });

        let tool_definitions: Vec<ToolDefinition> = tool_skills.iter().map(|skill| {
            ToolDefinition {
                name: skill.id().to_string(),
                description: skill.description().to_string(),
                parameters: skill.input_schema(),
            }
        }).collect();

        // Tool-less models (Local llama.cpp builds, custom providers that
        // declare no tool calling) cannot receive a native tools array. When
        // tools exist anyway, degrade to a text protocol instead of silently
        // dropping every connector.
        let emulate_tools = !tool_definitions.is_empty()
            && !self
                .model_supports_tools(&bot, &providers[active_provider].provider_type())
                .await;
        if emulate_tools {
            tracing::info!(
                bot = %bot.name,
                model = %bot.config.model_id,
                "Model lacks native tool calling; emulating tools over a text protocol"
            );
            self.emit(StreamEvent::Status {
                bot_id: bot.id,
                thread_id: run.thread_id,
                state: "emulating_tools".to_string(),
            });
        }

        // Check if this thread belongs to a chatroom/office for shared team intelligence
        let chatroom_row: Option<(String,)> = sqlx::query_as(
            "SELECT chatroom_id FROM chatroom_threads WHERE thread_id = ?"
        )
        .bind(run.thread_id.to_string())
        .fetch_optional(self.db.pool())
        .await
        .unwrap_or(None);

        let mut office_context = String::new();
        if let Some((cid_str,)) = chatroom_row {
            if let Ok(cid) = uuid::Uuid::parse_str(&cid_str) {
                if let Ok(Some(room)) = ravenbot_db::queries::ChatRoomQueries::get(self.db.pool(), cid).await {
                    let mut parts = vec![format!("Office: {} ({})", room.name, room.office_template)];
                    if let Some(goal) = &room.goal {
                        parts.push(format!("Quarterly Objective: {}", goal));
                    }
                    if let Some(policy) = &room.policy {
                        parts.push(format!("Office Standards & Policy: {}", policy));
                    }
                    if let Ok(memories) = self.office_memory.retrieve(cid, last_user_message, 5, 0.3).await {
                        if !memories.is_empty() {
                            let mem_lines: Vec<String> = memories.iter().map(|(m, _)| format!("• [{}] {}", m.category, m.content)).collect();
                            parts.push(format!("Shared Team Knowledge:\n{}", mem_lines.join("\n")));
                        }
                    }
                    office_context = parts.join("\n\n");
                }
            }
        }

        // Channel context: a thread filed under a channel inherits its shared
        // instructions and working folder.
        let mut channel_context = String::new();
        let channel_row: Option<(Option<String>,)> = sqlx::query_as(
            "SELECT channel_id FROM threads WHERE id = ?"
        )
        .bind(run.thread_id.to_string())
        .fetch_optional(self.db.pool())
        .await
        .unwrap_or(None);
        if let Some((Some(cid_str),)) = channel_row {
            if let Ok(cid) = uuid::Uuid::parse_str(&cid_str) {
                if let Ok(Some(channel)) =
                    ravenbot_db::queries::ChannelQueries::get(self.db.pool(), cid).await
                {
                    let mut parts = vec![format!("Channel: {}", channel.name)];
                    if !channel.instructions.trim().is_empty() {
                        parts.push(format!("Shared Instructions:\n{}", channel.instructions));
                    }
                    if let Some(folder) = &channel.working_folder {
                        parts.push(format!("Default Working Folder: {}", folder));
                    }
                    channel_context = parts.join("\n\n");
                }
            }
        }

        // Build conversation messages
        let mut model_messages = Vec::new();

        // Add system prompt with skill and memory information
        let system_prompt = bot.config.custom_prompt.as_deref()
            .unwrap_or("You are a helpful AI assistant. Complete tasks as requested. You have access to tools that can help you accomplish tasks.");
        
        let mut context_parts = Vec::new();

        if !channel_context.is_empty() {
            context_parts.push(format!("🗂️ Channel Context:\n{}", channel_context));
        }

        if is_deep_search {
            context_parts.push("⚡ [DeepSearch Active]: The user explicitly requested DeepSearch. You MUST use your search tools (web_search, tavily_search, or browser_navigate) to look up fresh, accurate information from the web before generating your final answer.".to_string());
        }
        if is_think {
            context_parts.push("🧠 [Think Mode Active]: The user explicitly requested Deep Reasoning. Thoroughly analyze the question, inspect constraints, trace edge cases, and reason step-by-step before delivering the optimal solution.".to_string());
        }
        if !office_context.is_empty() {
            context_parts.push(format!("🏢 Team Office Context:\n{}", office_context));
        }

        if !working_dirs.is_empty() {
            let dirs_txt = working_dirs
                .iter()
                .map(|d| d.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            context_parts.push(format!(
                "📁 Project folders (your working directory — keep ALL file and shell work inside these paths): {}",
                dirs_txt
            ));
        }

        if !tool_definitions.is_empty() {
            if emulate_tools {
                context_parts.push(tool_emulation_prompt(&tool_definitions));
            } else {
                let skill_names: Vec<&str> = tool_definitions.iter().map(|d| d.name.as_str()).collect();
                context_parts.push(format!("Tools available: {}", skill_names.join(", ")));
            }
        }

        // Inject workflow skill instructions into the system prompt
        let workflow_prompts: Vec<String> = workflow_skills.iter()
            .filter_map(|s| s.prompt().map(|p| (s, p)))
            .map(|(s, p)| format!("### Workflow: {}\n{}", s.name(), p))
            .collect();
        if !workflow_prompts.is_empty() {
            context_parts.push(format!(
                "## Active Workflow Guides\n\nThe following workflow guides are available. When the user's request matches a workflow, follow its instructions as a multi-turn process:\n\n{}",
                workflow_prompts.join("\n\n---\n\n")
            ));
        }
        
        if !memory_context.is_empty() {
            context_parts.push(memory_context);
        }
        
        let full_system = if context_parts.is_empty() {
            system_prompt.to_string()
        } else {
            format!("{}\n\nContext:\n{}", system_prompt, context_parts.join("\n\n"))
        };
        
        model_messages.push(Message::text("system", full_system));

        // Add conversation history
        for msg in &messages {
            let role = match msg.role {
                ravenbot_core::MessageRole::User => "user",
                ravenbot_core::MessageRole::Assistant => "assistant",
                ravenbot_core::MessageRole::System => "system",
                ravenbot_core::MessageRole::Tool => "user",
            };

            let mut content = match &msg.content {
                ravenbot_core::MessageContent::Text { text, .. } => text.clone(),
                ravenbot_core::MessageContent::Checklist { text, items } => {
                    let checklist_text: Vec<String> = items.iter().map(|item| {
                        let status = match item.status {
                            ravenbot_core::ChecklistStatus::Completed => "✓",
                            ravenbot_core::ChecklistStatus::Failed => "✗",
                            ravenbot_core::ChecklistStatus::InProgress => "○",
                            _ => "○",
                        };
                        format!("{} {}", status, item.label)
                    }).collect();
                    text.clone().map_or_else(|| checklist_text.join("\n"), |t| {
                        format!("{}\n{}", t, checklist_text.join("\n"))
                    })
                },
                _ => continue,
            };

            // Non-image attachments (PDFs, docs, archives, …) can't be sent
            // inline, but the model must still know they were shared instead of
            // silently losing them. Text-like files are inlined by the UI.
            let non_image: Vec<&ravenbot_core::Attachment> =
                msg.attachments.iter().filter(|a| !a.is_image).collect();
            if !non_image.is_empty() {
                let mut note = String::from("\n\n[Files the user attached:]");
                for a in &non_image {
                    let path = if a.path.trim().is_empty() {
                        String::new()
                    } else {
                        format!(", path: {}", a.path)
                    };
                    note.push_str(&format!(
                        "\n- {} ({}, {} bytes{})",
                        a.name, a.mime_type, a.size, path
                    ));
                }
                note.push_str(
                    "\nYou can open a file from disk with the file_read tool when a path is given; \
                     otherwise ask the user for the path if you need its contents.",
                );
                content.push_str(&note);
            }

            // Vision: inline image attachments ride with the message
            let images: Vec<ravenbot_models::MessageImage> = msg
                .attachments
                .iter()
                .filter(|a| a.is_image)
                .filter_map(|a| {
                    a.data.as_ref().map(|d| ravenbot_models::MessageImage {
                        data: d.clone(),
                        mime: a.mime_type.clone(),
                    })
                })
                .collect();

            model_messages.push(Message::text(role, content).with_images(images));
        }

        // Resume: if this run was paused mid-loop, restore the exact in-flight
        // message state from its checkpoint (tool results live only there, not
        // in the transcript) and continue from where it stopped.
        let mut resumed_rounds = 0u32;
        if run.state == ravenbot_core::RunState::Paused {
            if let Some(restored) = run
                .checkpoint
                .as_ref()
                .and_then(|cp| cp.state_data.get("model_messages"))
                .and_then(|v| serde_json::from_value::<Vec<Message>>(v.clone()).ok())
                .filter(|m| !m.is_empty())
            {
                resumed_rounds = run
                    .checkpoint
                    .as_ref()
                    .and_then(|cp| cp.state_data.get("rounds"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0)
                    as u32;
                tracing::info!(run = %run.id, rounds = resumed_rounds, "Resuming paused run from checkpoint");
                model_messages = restored;
            }
            run.state = ravenbot_core::RunState::Planning;
        }

        // Call the model with tools, streaming tokens live to the UI.
        // Transient provider failures (network blips, 5xx, rate limits) are
        // retried once automatically before surfacing to the user.
        let on_delta = self.delta_emitter(bot.id, run.thread_id);

        let temperature = bot.config.temperature.unwrap_or(0.7);
        let max_tokens = bot.config.max_tokens.unwrap_or(4096);
        let context_window = context_window_for(&bot.config.model_id);
        let context_reserve = max_tokens as u64 + 2000;

        // Compact before the first call so an over-long thread can't overflow.
        compact_messages(&mut model_messages, context_window, context_reserve);

        // Emulation passes the model an EMPTY native tool list — the protocol
        // lives in the system prompt and comes back as fenced JSON.
        let model_tools: &[ToolDefinition] = if emulate_tools { &[] } else { &tool_definitions };
        let mut last_raw_content = String::new();
        let mut last_call_sig: Option<String> = None;
        let mut repeat_count = 0u32;
        let mut repeat_nudged = false;

        let (mut response, idx) = self
            .call_model(
                &providers,
                active_provider,
                &model_messages,
                model_tools,
                temperature,
                max_tokens,
                on_delta.clone(),
                is_think,
            )
            .await?;
        active_provider = idx;
        if emulate_tools {
            last_raw_content = apply_emulated_response(&mut response);
        }

        // Record this round's usage against the bot's budget (every call counts)
        let _ = self.budget_manager.record_usage(
            bot.id,
            response.usage.input_tokens + response.usage.output_tokens,
            response.usage.cost(0.003, 0.015),
        ).await;

        // Handle tool calls
        let mut run_sources: Vec<ravenbot_core::Source> = Vec::new();
        // Images produced by tools (screenshots) are folded into the final
        // assistant message so they persist in the transcript.
        let mut run_images: Vec<ravenbot_core::Attachment> = Vec::new();
        let mut seen_source_urls: HashSet<String> = HashSet::new();
        let total_tool_rounds = self.max_tool_rounds(&bot);
        let mut max_tool_rounds = total_tool_rounds.saturating_sub(resumed_rounds);
        while !response.tool_calls.is_empty() && max_tool_rounds > 0 {
            if self.kill_switch.is_triggered().await {
                return Err(RuntimeError::KillSwitchActive("Kill switch triggered during execution".to_string()));
            }
            if self.is_cancelled(run.id) {
                run.complete(ravenbot_core::RunOutcome::Cancelled {
                    reason: Some("User cancelled".to_string()),
                });
                ravenbot_db::queries::RunQueries::update(self.db.pool(), run).await?;
                self.clear_cancel(run.id);
                return Ok(());
            }
            // Resumable pause: park here with the checkpoint written at the end
            // of the previous round; `resume_run` re-enters from that state.
            if self.is_pause_requested(run.id) {
                run.state = ravenbot_core::RunState::Paused;
                if run.checkpoint.is_none() {
                    run.checkpoint(serde_json::json!({
                        "model_messages": &model_messages,
                        "rounds": resumed_rounds,
                    }));
                }
                ravenbot_db::queries::RunQueries::update(self.db.pool(), run).await?;
                self.clear_cancel(run.id);
                self.emit(StreamEvent::Status {
                    bot_id: bot.id,
                    thread_id: run.thread_id,
                    state: "paused".to_string(),
                });
                return Ok(());
            }

            max_tool_rounds -= 1;

            let calls: Vec<ravenbot_models::ToolCall> = response.tool_calls.clone();
            let assistant_content = response.content.clone().unwrap_or_default();

            if emulate_tools {
                // Repeat guard: the same call three rounds running → nudge
                // once; still stuck → abandon the protocol, keep the text.
                let sig = emulated_call_signature(&calls);
                if Some(&sig) == last_call_sig.as_ref() {
                    repeat_count += 1;
                } else {
                    repeat_count = 1;
                    last_call_sig = Some(sig);
                }
                if repeat_count >= 3 {
                    if repeat_nudged {
                        response.tool_calls.clear();
                        break;
                    }
                    repeat_nudged = true;
                    model_messages.push(Message::text("assistant", last_raw_content.clone()));
                    model_messages.push(Message::text(
                        "user",
                        "You have requested the same tool call three times. It will not run again. Either choose a different action or write your final answer now from the results you already have.",
                    ));
                    compact_messages(&mut model_messages, context_window, context_reserve);
                    self.emit(StreamEvent::Clear {
                        bot_id: bot.id,
                        thread_id: run.thread_id,
                    });
                    let (next_response, idx) = self
                        .call_model(
                            &providers,
                            active_provider,
                            &model_messages,
                            model_tools,
                            bot.config.temperature.unwrap_or(0.7),
                            bot.config.max_tokens.unwrap_or(4096),
                            on_delta.clone(),
                            is_think,
                        )
                        .await?;
                    active_provider = idx;
                    response = next_response;
                    last_raw_content = apply_emulated_response(&mut response);
                    continue;
                }
                // The model must see its own raw protocol text, not the
                // stripped answer that the user gets.
                model_messages.push(Message::text("assistant", last_raw_content.clone()));
            } else {
                // Native assistant turn carrying the tool-call ids, so models
                // continue the tool loop correctly (previously tool results were
                // flattened into fake user text and ids were lost).
                model_messages.push(Message::assistant_tool_calls(assistant_content, calls.clone()));
            }

            // The run is confined to its workspace, always.
            //
            // `resolve_working_dirs` never returns an empty list, so this
            // always names at least one root. It is built the explicit way
            // rather than through `with_working_dirs` so that the intent is
            // visible: if a future change ever produced no roots, this denies
            // every path instead of quietly becoming unrestricted.
            let skill_context = SkillContext::new(
                bot.id,
                run.id,
                run.thread_id,
                bot.config.sandbox_tier.clone(),
            )
            .with_working_dirs(working_dirs.clone())
            .confined();

            // ── Approval pass (sequential, in order: cards appear in order) ──
            let mut decisions: Vec<bool> = Vec::with_capacity(calls.len());
            for tool_call in &calls {
                self.emit(StreamEvent::ToolStarted {
                    thread_id: run.thread_id,
                    bot_id: bot.id,
                    name: tool_call.name.clone(),
                });
                self.emit(StreamEvent::Status {
                    bot_id: bot.id,
                    thread_id: run.thread_id,
                    state: "running_tool".to_string(),
                });

                let _ = self.audit_logger.log_tool_call(
                    bot.id,
                    Some(run.id),
                    Some(run.thread_id),
                    &tool_call.name,
                    tool_call.arguments.clone(),
                ).await;

                // ask_user never gates: the card *is* the interaction.
                if tool_call.name == "ask_user" {
                    decisions.push(true);
                    continue;
                }

                let tool_risk = self.resolve_tool_risk(&tool_call.name, &tool_skills).await;
                let mut needs_approval = match bot.approval_mode {
                    ravenbot_core::ApprovalMode::Full => false,
                    ravenbot_core::ApprovalMode::Auto => {
                        matches!(tool_risk, ravenbot_skills::SkillRisk::High)
                    }
                    ravenbot_core::ApprovalMode::Ask => {
                        !matches!(tool_risk, ravenbot_skills::SkillRisk::ReadOnly)
                    }
                };
                if tool_call.name == "delegate"
                    && !matches!(bot.approval_mode, ravenbot_core::ApprovalMode::Full)
                {
                    needs_approval = true;
                }

                if needs_approval {
                    match self
                        .request_approval(run, &bot, &tool_call.name, &tool_call.arguments, tool_risk)
                        .await
                    {
                        Ok(allowed) => decisions.push(allowed),
                        Err(e) => return Err(e),
                    }
                } else {
                    decisions.push(true);
                }
            }

            // ── Execution pass (allowed calls run concurrently) ──
            let run_ref: &Run = &*run;
            let exec_futures = calls.iter().zip(decisions.iter()).map(|(tool_call, allowed)| {
                let self_ref = self;
                let bot_ref = &bot;
                let skills_ref = &tool_skills;
                let ctx_ref = &skill_context;
                let allowed = *allowed;
                // `run_ref: &Run` is Copy, so `async move` captures it safely.
                async move {
                    if !allowed {
                        return (
                            tool_call.id.clone(),
                            tool_call.name.clone(),
                            serde_json::json!({
                                "denied": true,
                                "tool": tool_call.name,
                                "note": "The user denied this action. Do NOT retry it - explain briefly and continue with something else.",
                            }),
                        );
                    }
                    tracing::info!(
                        skill = %tool_call.name,
                        arguments = %tool_call.arguments,
                        "Executing tool"
                    );
                    let json = self_ref
                        .execute_tool_call(
                            bot_ref,
                            run_ref,
                            skills_ref,
                            ctx_ref,
                            &tool_call.name,
                            &tool_call.arguments,
                        )
                        .await;
                    (tool_call.id.clone(), tool_call.name.clone(), json)
                }
            });

            let results = futures::future::join_all(exec_futures).await;

            // ── Feed native tool results back in call order + harvest sources ──
            for (id, name, result_json) in results {
                let mut extracted_sources = Vec::new();
                extract_sources(&result_json, &mut extracted_sources);
                for source in extracted_sources {
                    if run_sources.len() >= 10 {
                        break;
                    }
                    if seen_source_urls.insert(source.url.clone()) {
                        self.emit(StreamEvent::Sources {
                            thread_id: run.thread_id,
                            sources: vec![source.clone()],
                        });
                        run_sources.push(source);
                    }
                }

                if let Some((image_name, data_url)) = extract_image(&result_json) {
                    if run_images.len() < 10 {
                        self.emit(StreamEvent::Image {
                            thread_id: run.thread_id,
                            name: image_name.clone(),
                            data_url: data_url.clone(),
                        });
                        if let Some(att) = image_attachment_from_data_url(&image_name, &data_url) {
                            run_images.push(att);
                        }
                    }
                }

                let result_text = result_json.to_string();
                if emulate_tools {
                    // Plain-text transcript for models without tool support:
                    // results ride back in as bracketed user turns.
                    model_messages.push(Message::text("user", format!("[tool_result:{}]\n{}", name, result_text)));
                } else {
                    model_messages.push(Message::tool_result(id, name.clone(), result_text));
                }
                run.add_usage(0, 0.001);
                self.emit(StreamEvent::ToolFinished {
                    thread_id: run.thread_id,
                    bot_id: bot.id,
                    name,
                });
            }
            self.emit(StreamEvent::Status {
                bot_id: bot.id,
                thread_id: run.thread_id,
                state: "thinking".to_string(),
            });

            // Checkpoint the in-flight loop state so a pause/crash can resume
            // it (the transcript does not contain native tool results).
            let rounds_used = total_tool_rounds.saturating_sub(max_tool_rounds);
            run.checkpoint(serde_json::json!({
                "model_messages": &model_messages,
                "rounds": resumed_rounds + rounds_used,
            }));
            let _ = ravenbot_db::queries::RunQueries::update(self.db.pool(), run).await;

            // A new model round begins: clear the streamed text so tool-round
            // fragments don't mix with the final streamed response.
            self.emit(StreamEvent::Clear {
                bot_id: bot.id,
                thread_id: run.thread_id,
            });

            compact_messages(&mut model_messages, context_window, context_reserve);
            let (next_response, idx) = self
                .call_model(
                    &providers,
                    active_provider,
                    &model_messages,
                    model_tools,
                    bot.config.temperature.unwrap_or(0.7),
                    bot.config.max_tokens.unwrap_or(4096),
                    on_delta.clone(),
                    is_think,
                )
                .await?;
            active_provider = idx;
            response = next_response;
            if emulate_tools {
                last_raw_content = apply_emulated_response(&mut response);
            }

            // Record each tool-round's usage as well
            let _ = self.budget_manager.record_usage(
                bot.id,
                response.usage.input_tokens + response.usage.output_tokens,
                response.usage.cost(0.003, 0.015),
            ).await;
        }

        // Update usage
        let total_tokens = run.tokens_consumed;
        let total_cost = run.cost_estimate;

        // Surface real usage to the UI telemetry (cumulative for the whole run)
        self.emit(StreamEvent::Usage {
            thread_id: run.thread_id,
            tokens: total_tokens,
            cost: total_cost,
        });

        // Create assistant message (with any harvested web sources and tool
        // images such as screenshots). Extended-thinking reasoning is persisted
        // as a <think> block so the UI's Reasoning panel survives reload.
        let has_images = !run_images.is_empty();
        if response.content.is_some() || has_images {
            let content = response.content.unwrap_or_default();
            let final_content = match response.reasoning.filter(|r| !r.trim().is_empty()) {
                Some(reasoning) if !content.contains("<think>") => {
                    format!("<think>\n{}\n</think>\n\n{}", reasoning.trim(), content)
                }
                _ => content,
            };
            let mut assistant_msg = ravenbot_core::Message::assistant_with_sources(
                run.thread_id,
                final_content,
                run_sources,
            );
            if has_images {
                assistant_msg.attachments = run_images;
            }
            ravenbot_db::queries::MessageQueries::insert(self.db.pool(), &assistant_msg).await?;
        }

        // Live status: done
        self.emit(StreamEvent::Status {
            bot_id: bot.id,
            thread_id: run.thread_id,
            state: "done".to_string(),
        });

        // Complete the run
        run.complete(ravenbot_core::RunOutcome::Success {
            result: "Response generated".to_string(),
        });

        // Save to database
        ravenbot_db::queries::RunQueries::update(self.db.pool(), run).await?;

        // Self-review and memory update (skipped for ephemeral threads)
        if !thread_ephemeral {
            if let Ok(review) = self.self_reviewer.review_run(run).await {
                if !review.memory_updates.is_empty() {
                    let _ = self.self_reviewer.apply_updates(bot.id, &review.memory_updates).await;
                }
                tracing::info!(
                    bot_id = %bot.id,
                    quality = review.quality_score,
                    "Run reviewed"
                );
            }
        }

        Ok(())
    }

    /// Build the engine prompt from the thread transcript plus the current turn.
    fn build_engine_prompt(
        messages: &[ravenbot_core::Message],
        bot_name: &str,
        last_user_message: &str,
    ) -> String {
        let mut prior: Vec<String> = Vec::new();
        // All but the final user turn (which becomes the explicit request).
        let mut seen_last = false;
        for msg in messages.iter().rev() {
            if !seen_last
                && matches!(msg.role, ravenbot_core::MessageRole::User)
                && matches!(&msg.content, ravenbot_core::MessageContent::Text { text, .. } if text == last_user_message)
            {
                seen_last = true;
                continue;
            }
            let text = match &msg.content {
                ravenbot_core::MessageContent::Text { text, .. } => text.clone(),
                ravenbot_core::MessageContent::Checklist { text, items } => {
                    let list: Vec<String> = items.iter().map(|i| format!("- {}", i.label)).collect();
                    text.clone().map_or_else(|| list.join("\n"), |t| format!("{}\n{}", t, list.join("\n")))
                }
                _ => continue,
            };
            if text.trim().is_empty() {
                continue;
            }
            let speaker = match msg.role {
                ravenbot_core::MessageRole::User => "User".to_string(),
                ravenbot_core::MessageRole::Assistant => bot_name.to_string(),
                ravenbot_core::MessageRole::System => "System".to_string(),
                ravenbot_core::MessageRole::Tool => "Tool".to_string(),
            };
            prior.push(format!("{}: {}", speaker, text));
        }
        prior.reverse();
        // Keep the replay bounded so long threads don't blow the prompt.
        if prior.len() > 24 {
            prior = prior.split_off(prior.len() - 24);
        }

        if prior.is_empty() {
            last_user_message.to_string()
        } else {
            format!(
                "Conversation so far:\n{}\n\nCurrent request:\n{}",
                prior.join("\n"),
                last_user_message
            )
        }
    }

    /// Execute one turn on an external agent engine.
    /// The bot's effective MCP servers in engine-neutral form, capped at 16
    /// so CLI startup stays cheap. Never fatal: an engine run proceeds without
    /// connectors if resolution fails.
    async fn engine_mcp_servers(&self, bot_id: uuid::Uuid) -> Vec<ravenbot_engines::EngineMcpServer> {
        let ids = match self.mcp_registry.enabled_server_ids(bot_id).await {
            Ok(ids) => ids,
            Err(e) => {
                tracing::warn!(%e, "engine run: MCP server listing failed; continuing without servers");
                return Vec::new();
            }
        };
        let mut out = Vec::new();
        for id in ids.into_iter().take(16) {
            let Ok(Some((cfg, env))) = self.mcp_registry.server_config_with_env(&id).await else {
                continue;
            };
            let is_http = matches!(cfg.transport, ravenbot_mcp::servers::McpTransport::Http)
                || cfg.url.as_deref().map(|u| !u.trim().is_empty()).unwrap_or(false);
            // Resolve `${VAR}` header references the same way McpClient does
            // before handing configs to an external process.
            let headers = cfg
                .headers
                .iter()
                .map(|(k, v)| {
                    let mut value = v.clone();
                    for (ek, ev) in &env {
                        value = value.replace(&format!("${{{}}}", ek), ev);
                    }
                    (k.clone(), value)
                })
                .collect();
            out.push(ravenbot_engines::EngineMcpServer {
                name: cfg.id.clone(),
                transport: if is_http { "http".to_string() } else { "stdio".to_string() },
                command: cfg.command.clone(),
                args: cfg.args.clone(),
                env,
                url: cfg.url.clone().filter(|u| !u.trim().is_empty()),
                headers,
            });
        }
        out
    }

    async fn execute_engine_run(
        &self,
        run: &mut Run,
        bot: &ravenbot_core::Bot,
        engine_id: &str,
        messages: &[ravenbot_core::Message],
        memory_context: &str,
        is_think: bool,
        working_dir: Option<std::path::PathBuf>,
    ) -> Result<(), RuntimeError> {
        let engine = ravenbot_engines::engine_by_id(engine_id).ok_or_else(|| {
            RuntimeError::Model(format!("Unknown engine '{}'", engine_id))
        })?;

        // System prompt: persona + memory + mode notes.
        let base_prompt = bot.config.custom_prompt.clone().unwrap_or_else(|| {
            format!(
                "You are {}, an autonomous agent running on this machine. Use your tools to complete the user's request end to end.",
                bot.name
            )
        });
        let mut system = base_prompt;
        if is_think {
            system.push_str("\n\n[Think Mode]: reason carefully, inspect constraints, and trace edge cases before answering.");
        }
        if !memory_context.is_empty() {
            system.push_str("\n\nRelevant memory:\n");
            system.push_str(memory_context);
        }

        let last_user_message = messages
            .iter()
            .rev()
            .find(|m| matches!(m.role, ravenbot_core::MessageRole::User))
            .and_then(|m| match &m.content {
                ravenbot_core::MessageContent::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .unwrap_or_default();
        let prompt = Self::build_engine_prompt(messages, &bot.name, &last_user_message);

        // Model selection for external engines: an explicit `engine_model`
        // always wins; otherwise use the bot's model id when the engine accepts
        // it (provider-qualified ids only for engines that support them).
        let model = {
            let explicit = bot
                .config
                .engine_model
                .clone()
                .filter(|m| !m.trim().is_empty());
            match explicit {
                Some(m) => Some(m),
                None => {
                    let m = bot.config.model_id.trim();
                    let accepts_full = engine.capabilities().accepts_full_model_id;
                    if m.is_empty() || (m.contains('/') && !accepts_full) {
                        None
                    } else {
                        Some(m.to_string())
                    }
                }
            }
        };

        let approval = match bot.approval_mode {
            ravenbot_core::ApprovalMode::Full => ravenbot_engines::EngineApproval::Full,
            ravenbot_core::ApprovalMode::Auto => ravenbot_engines::EngineApproval::Auto,
            ravenbot_core::ApprovalMode::Ask => ravenbot_engines::EngineApproval::Ask,
        };

        // External engines lose every connector unless the bot's effective
        // MCP servers travel with the request.
        let mcp_servers = self.engine_mcp_servers(bot.id).await;

        let request = ravenbot_engines::EngineRequest {
            prompt,
            system: Some(system),
            model,
            approval,
            mcp_servers,
            cwd: working_dir
                .clone()
                .or_else(|| std::env::current_dir().ok())
                .map(|p| p.to_string_lossy().to_string()),
            timeout_secs: 900,
            ..Default::default()
        };

        // Map engine events onto the same StreamEvent channel the UI consumes.
        let bot_id = bot.id;
        let thread_id = run.thread_id;
        let emitter = self.emitter_for(thread_id);
        // Tool ids → names, so ToolFinished can carry the model-facing name.
        let tool_names: Arc<std::sync::Mutex<HashMap<String, String>>> =
            Arc::new(std::sync::Mutex::new(HashMap::new()));
        let engine_label = engine_id.to_string();
        let on_event: ravenbot_engines::EngineCallback = Arc::new(move |event| {
            let Some(emit) = emitter.as_ref() else { return };
            match event {
                ravenbot_engines::EngineEvent::TextDelta(text) => emit(StreamEvent::Delta {
                    bot_id,
                    thread_id,
                    content: text,
                }),
                // Reasoning is persisted as a reasoning block; not streamed
                // into the answer bubble.
                ravenbot_engines::EngineEvent::ReasoningDelta(_) => {}
                ravenbot_engines::EngineEvent::SessionStarted { .. } => {}
                ravenbot_engines::EngineEvent::AssistantText(_) => {}
                ravenbot_engines::EngineEvent::ToolStarted { id, name, .. } => {
                    if let Ok(mut map) = tool_names.lock() {
                        map.insert(id, name.clone());
                    }
                    emit(StreamEvent::ToolStarted { thread_id, bot_id, name });
                }
                ravenbot_engines::EngineEvent::ToolFinished { id, .. } => {
                    let name = tool_names
                        .lock()
                        .ok()
                        .and_then(|mut map| map.remove(&id))
                        .unwrap_or_else(|| "tool".to_string());
                    emit(StreamEvent::ToolFinished { thread_id, bot_id, name });
                }
                ravenbot_engines::EngineEvent::Status(state) => {
                    let state = if state == "done" { "thinking".to_string() } else { state };
                    emit(StreamEvent::Status { bot_id, thread_id, state });
                }
                ravenbot_engines::EngineEvent::Usage { input, output, cost } => {
                    emit(StreamEvent::Usage {
                        thread_id,
                        tokens: input + output,
                        cost: cost.unwrap_or(0.0),
                    });
                }
                ravenbot_engines::EngineEvent::Warning(message) => {
                    tracing::warn!(engine = %engine_label, %message, "engine warning");
                }
            }
        });

        let cancel = ravenbot_engines::CancelToken::new();
        if let Ok(mut tokens) = self.engine_cancels.lock() {
            tokens.insert(run.id, cancel.clone());
        }

        let timeout_secs = request.timeout_secs;
        let started = std::time::Instant::now();
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            engine.run(request, on_event, cancel.clone()),
        )
        .await;

        if let Ok(mut tokens) = self.engine_cancels.lock() {
            tokens.remove(&run.id);
        }

        let outcome = match result {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(e)) if e.code == ravenbot_engines::EngineErrorCode::Cancelled => {
                run.complete(ravenbot_core::RunOutcome::Cancelled {
                    reason: Some("User cancelled".to_string()),
                });
                ravenbot_db::queries::RunQueries::update(self.db.pool(), run).await?;
                self.emit(StreamEvent::Status { bot_id, thread_id, state: "done".to_string() });
                return Ok(());
            }
            Ok(Err(e)) => {
                self.emit(StreamEvent::Status { bot_id, thread_id, state: "done".to_string() });
                return Err(RuntimeError::Model(e.message));
            }
            Err(_) => {
                // Timed out: cancel to kill the child, then report.
                cancel.cancel();
                self.emit(StreamEvent::Status { bot_id, thread_id, state: "done".to_string() });
                return Err(RuntimeError::Model(format!(
                    "Engine '{}' timed out after {}s",
                    engine_id, timeout_secs
                )));
            }
        };

        // Assemble the persisted message: `<think>` reasoning block + answer.
        let final_text = outcome.final_text.trim().to_string();
        let reasoning = outcome.reasoning.trim().to_string();
        let content = if !reasoning.is_empty() {
            format!("<think>{}</think>\n\n{}", reasoning, final_text)
        } else {
            final_text.clone()
        };

        if !content.trim().is_empty() {
            let assistant_msg = ravenbot_core::Message::assistant(run.thread_id, content);
            ravenbot_db::queries::MessageQueries::insert(self.db.pool(), &assistant_msg).await?;
        }

        // Record real usage/cost against the budget.
        let tokens = outcome.usage.map(|u| u.input + u.output).unwrap_or(0);
        run.add_usage(tokens, outcome.cost.unwrap_or(0.0));
        let _ = self
            .budget_manager
            .record_usage(bot.id, tokens, outcome.cost.unwrap_or(0.0))
            .await;

        self.emit(StreamEvent::Usage {
            thread_id,
            tokens: run.tokens_consumed,
            cost: run.cost_estimate,
        });
        self.emit(StreamEvent::Status { bot_id, thread_id, state: "done".to_string() });

        let elapsed_ms = started.elapsed().as_millis() as u64;
        run.complete(ravenbot_core::RunOutcome::Success {
            result: format!("Engine '{}' completed in {}ms", engine_id, elapsed_ms),
        });
        ravenbot_db::queries::RunQueries::update(self.db.pool(), run).await?;
        Ok(())
    }

    /// Pause a running run
    pub async fn pause_run(&self, run: &mut Run) -> Result<(), RuntimeError> {
        run.state = RunState::Paused;
        // Preserve the loop's resume checkpoint (written every tool round); only
        // stamp a minimal one if the run had not reached a checkpoint yet.
        if run.checkpoint.is_none() {
            run.checkpoint(serde_json::json!({}));
        } else {
            run.updated_at = chrono::Utc::now();
        }
        ravenbot_db::queries::RunQueries::update(self.db.pool(), run).await?;
        Ok(())
    }

    /// Cancel a running run
    pub async fn cancel_run(&self, run: &mut Run) -> Result<(), RuntimeError> {
        run.complete(ravenbot_core::RunOutcome::Cancelled {
            reason: Some("User cancelled".to_string()),
        });
        ravenbot_db::queries::RunQueries::update(self.db.pool(), run).await?;
        Ok(())
    }
}

/// Harvest web sources from a tool result JSON.
/// Recognizes both `results: [{url, title, snippet}]` arrays (search skills)
/// and direct `{url, title}` objects (browser_navigate, youtube, http tools).
fn extract_sources(value: &serde_json::Value, out: &mut Vec<ravenbot_core::Source>) {
    let harvest = |obj: &serde_json::Value, out: &mut Vec<ravenbot_core::Source>| {
        let url = obj.get("url").and_then(|v| v.as_str());
        let url = match url {
            Some(u) if u.starts_with("http") => u,
            _ => return,
        };
        let title = obj
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or(url);
        let snippet = obj
            .get("snippet")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        out.push(ravenbot_core::Source {
            url: url.to_string(),
            title: title.to_string(),
            snippet,
        });
    };

    if let Some(results) = value.get("results").and_then(|v| v.as_array()) {
        for result in results {
            harvest(result, out);
        }
    } else {
        harvest(value, out);
    }
}

/// Harvest an image from a tool result JSON. Recognizes `data_url` (screenshot
/// skill) and `image_b64`/`image_base64` payloads, returning a persistable
/// attachment so screenshots appear in the transcript.
fn extract_image(value: &serde_json::Value) -> Option<(String, String)> {
    let name = value
        .get("name")
        .or_else(|| value.get("tool"))
        .and_then(|v| v.as_str())
        .unwrap_or("tool-image")
        .to_string();

    if let Some(data_url) = value.get("data_url").and_then(|v| v.as_str()) {
        if data_url.starts_with("data:image/") {
            return Some((name, data_url.to_string()));
        }
    }
    for key in ["image_b64", "image_base64", "audio_b64"] {
        if let Some(b64) = value.get(key).and_then(|v| v.as_str()) {
            if !b64.trim().is_empty() {
                let mime = value
                    .get("mime")
                    .or_else(|| value.get("mime_type"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("image/png");
                let mime = if key == "audio_b64" { "audio/wav" } else { mime };
                return Some((name, format!("data:{};base64,{}", mime, b64)));
            }
        }
    }
    None
}

/// Build an image Attachment from a `data:<mime>;base64,<data>` URL.
fn image_attachment_from_data_url(name: &str, data_url: &str) -> Option<ravenbot_core::Attachment> {
    let rest = data_url.strip_prefix("data:")?;
    let (mime, b64) = rest.split_once(";base64,")?;
    if !mime.starts_with("image/") || b64.is_empty() {
        return None;
    }
    Some(ravenbot_core::Attachment {
        id: Uuid::new_v4(),
        name: name.to_string(),
        mime_type: mime.to_string(),
        size: b64.len() as u64,
        path: String::new(),
        data: Some(b64.to_string()),
        is_image: true,
    })
}

/// Rough token estimate for a string (~4 chars/token). Good enough to decide
/// when to compact; never billed against the model.
fn estimate_text_tokens(text: &str) -> u64 {
    (text.chars().count() as u64).div_ceil(4)
}

/// Estimated tokens for one model message (content + native tool calls).
fn estimate_message_tokens(msg: &ravenbot_models::Message) -> u64 {
    let mut total = estimate_text_tokens(&msg.content) + 4;
    for tc in &msg.tool_calls {
        total += estimate_text_tokens(&tc.name);
        total += estimate_text_tokens(&tc.arguments.to_string());
    }
    total
}

fn estimate_messages_tokens(messages: &[ravenbot_models::Message]) -> u64 {
    messages.iter().map(estimate_message_tokens).sum()
}

/// Best-effort context window for a model id. Overridable with
/// `RAVENBOT_CONTEXT_TOKENS` (applies to every model).
/// System-prompt block for tool-less models: protocol rules plus the full
/// JSON schemas of every assembled tool.
fn tool_emulation_prompt(tools: &[ToolDefinition]) -> String {
    let mut out = String::from(
        "## Tool Use (text protocol)\n\
         This model has no native tool calling, so tools work through text. When you want to use \
         tools, reply with EXACTLY ONE fenced json block and put nothing after it:\n\n\
         ```json\n{\"tool_calls\": [{\"name\": \"<tool name>\", \"arguments\": {\"<arg>\": <value>}}]}\n```\n\n\
         The runtime runs the calls and returns each result as a `[tool_result:<name>]` message. \
         Then either emit another tool block or answer the user. Never write a tool block for a \
         tool not listed below, never invent tool results, and answer normally when you need no tool.",
    );
    out.push_str("\n\nAvailable tools:\n");
    for tool in tools {
        out.push_str(&format!(
            "- {}: {}\n  arguments: {}\n",
            tool.name, tool.description, tool.parameters
        ));
    }
    out
}

/// Extract `{"tool_calls": [...]}` fenced blocks from one model reply.
/// Returns the synthesized calls plus the text with protocol blocks removed;
/// ordinary code fences the model wrote for the user are preserved verbatim.
fn parse_emulated_tool_calls(text: &str) -> (Vec<ravenbot_models::ToolCall>, String) {
    let mut calls: Vec<ravenbot_models::ToolCall> = Vec::new();
    let mut stripped = String::new();
    let mut rest = text;
    while let Some(open) = rest.find("```") {
        stripped.push_str(&rest[..open]);
        let after = &rest[open + 3..];
        let Some(newline) = after.find('\n') else {
            // Truncated fence — keep the raw text.
            stripped.push_str(&rest[open..]);
            rest = "";
            break;
        };
        let lang = after[..newline].trim();
        let body = &after[newline + 1..];
        let Some(close) = body.find("```") else {
            stripped.push_str(&rest[open..]);
            rest = "";
            break;
        };
        let block_end = open + 3 + newline + 1 + close + 3;
        // The payload is either after a `json` fence tag, or the model put the
        // JSON right after the backticks (the "tag" line is then real payload).
        let source: Option<String> = if lang.is_empty() || lang.eq_ignore_ascii_case("json") {
            Some(body[..close].to_string())
        } else if lang.starts_with('{') || lang.starts_with('[') {
            Some(format!("{}\n{}", lang, &body[..close]))
        } else {
            None
        };
        let mut is_protocol_block = false;
        if let Some(source) = source {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&source) {
                if let Some(items) = value.get("tool_calls").and_then(|v| v.as_array()) {
                    is_protocol_block = true;
                    for item in items {
                        let Some(name) = item.get("name").and_then(|v| v.as_str()) else {
                            continue;
                        };
                        if name.trim().is_empty() {
                            continue;
                        }
                        let arguments = item
                            .get("arguments")
                            .or_else(|| item.get("args"))
                            .cloned()
                            .unwrap_or_else(|| serde_json::json!({}));
                        calls.push(ravenbot_models::ToolCall {
                            name: name.to_string(),
                            arguments,
                            id: format!("emu-{}", uuid::Uuid::new_v4()),
                        });
                    }
                }
            }
        }
        if !is_protocol_block {
            // Ordinary code the model wrote for the user — keep it verbatim.
            stripped.push_str(&rest[open..block_end]);
        }
        rest = &rest[block_end..];
    }
    stripped.push_str(rest);
    (calls, stripped.trim().to_string())
}

/// Canonical form of one emulated round's calls for repeat detection.
fn emulated_call_signature(calls: &[ravenbot_models::ToolCall]) -> String {
    calls
        .iter()
        .map(|call| format!("{}:{}", call.name, call.arguments))
        .collect::<Vec<_>>()
        .join("|")
}

/// Post-process one model response in emulation mode: synthesize tool calls
/// from fenced JSON and remove the protocol text from the user-visible
/// content. Returns the raw content so the transcript can show the model its
/// own protocol block.
fn apply_emulated_response(response: &mut ravenbot_models::ModelResponse) -> String {
    let raw = response.content.clone().unwrap_or_default();
    let (calls, stripped) = parse_emulated_tool_calls(&raw);
    response.tool_calls = calls;
    response.content = Some(stripped);
    raw
}

fn context_window_for(model: &str) -> u64 {
    if let Ok(v) = std::env::var("RAVENBOT_CONTEXT_TOKENS") {
        if let Ok(n) = v.trim().parse::<u64>() {
            if n > 0 {
                return n;
            }
        }
    }
    let m = model.to_lowercase();
    if m.contains("gemini") {
        1_000_000
    } else if m.contains("claude") || m.contains("anthropic") {
        200_000
    } else if m.contains("gpt-4o")
        || m.contains("gpt-4.1")
        || m.contains("gpt-5")
        || m.contains("o1")
        || m.contains("o3")
        || m.contains("o4")
    {
        128_000
    } else if m.contains("llama")
        || m.contains("qwen")
        || m.contains("mistral")
        || m.contains("phi")
        || m.contains("gemma")
    {
        32_000
    } else {
        128_000
    }
}

/// Number of trailing messages always kept verbatim during compaction.
const COMPACT_KEEP_TAIL: usize = 12;

/// Deterministic sliding-window compaction. Keeps the system message and the
/// most recent messages that fit, replacing the dropped middle with a single
/// marker so the model knows history was elided. The kept tail never starts on
/// a `tool` result (which would orphan it from its assistant tool call).
fn compact_messages(
    messages: &mut Vec<ravenbot_models::Message>,
    window: u64,
    reserve: u64,
) {
    let available = window.saturating_sub(reserve).max(1024);
    if estimate_messages_tokens(messages) <= available {
        return;
    }

    let system_len = usize::from(
        messages
            .first()
            .map(|m| m.role == "system")
            .unwrap_or(false),
    );
    if messages.len() <= system_len + 1 {
        return;
    }

    // Greedily keep the longest suffix that fits alongside the system message,
    // but never fewer than COMPACT_KEEP_TAIL or two messages.
    let mut used = estimate_messages_tokens(&messages[..system_len]);
    let mut cut = messages.len();
    while cut > system_len + 1 {
        let next = &messages[cut - 1];
        let cost = estimate_message_tokens(next);
        let remaining = messages.len() - (cut - 1);
        if used + cost > available && remaining >= COMPACT_KEEP_TAIL {
            break;
        }
        used += cost;
        cut -= 1;
    }
    // Never start the kept tail on an orphaned tool result.
    while cut > system_len && cut < messages.len() && messages[cut].role == "tool" {
        cut -= 1;
    }
    if cut <= system_len {
        return;
    }

    let dropped = messages.len() - cut;
    let marker = ravenbot_models::Message::text(
        "user",
        format!(
            "[Earlier conversation compacted: {} message(s) omitted to fit the context window. Continue from the most recent messages.]",
            dropped
        ),
    );
    let mut compacted = Vec::with_capacity(system_len + 1 + (messages.len() - cut));
    compacted.extend_from_slice(&messages[..system_len]);
    compacted.push(marker);
    compacted.extend_from_slice(&messages[cut..]);

    tracing::info!(
        dropped,
        before = estimate_messages_tokens(messages),
        after = estimate_messages_tokens(&compacted),
        "Compacted conversation context"
    );
    *messages = compacted;
}

/// Short standalone social turns that never need tools, memory lookup, skills,
/// reasoning traces, or agent orchestration: greetings, farewells, and thanks.
const SIMPLE_CONVERSATIONAL_TURNS: &[&str] = &[
    "hi",
    "hello",
    "hey",
    "hey there",
    "hi there",
    "hello there",
    "yo",
    "sup",
    "greetings",
    "good morning",
    "good afternoon",
    "good evening",
    "good day",
    "good night",
    "goodnight",
    "how are you",
    "how are u",
    "how r you",
    "how r u",
    "how is it going",
    "hows it going",
    "how are you doing",
    "how are you today",
    "how have you been",
    "how is your day",
    "how is everything",
    "whats up",
    "what is up",
    "whats new",
    "what is new",
    "thanks",
    "thank you",
    "thankyou",
    "thanks a lot",
    "thank you very much",
    "thanks so much",
    "many thanks",
    "bye",
    "goodbye",
    "good bye",
    "bye bye",
    "see you",
    "see ya",
    "see you later",
    "take care",
    "have a good day",
    "have a nice day",
    "have a great day",
];

/// Keep recent lightweight context for a short social reply. Tool/system
/// messages and image attachments are excluded because they can be large and
/// are unnecessary for a greeting.
const SIMPLE_HISTORY_CHAR_LIMIT: usize = 2_000;

/// A conversational turn is "simple" only when it is standalone social text.
/// This deliberately rejects acknowledgments such as "ok" or "yes", which may
/// answer a pending question or approval and therefore need full context.
fn is_simple_conversational_turn(
    bot_name: &str,
    last_user_message: &str,
    messages: &[ravenbot_core::Message],
) -> bool {
    if last_user_message.contains("[DeepSearch]") || last_user_message.contains("[Think]") {
        return false;
    }
    if last_user_message.trim().is_empty() {
        return false;
    }

    let Some(latest) = messages.last() else {
        return false;
    };
    let ravenbot_core::MessageContent::Text { text, .. } = &latest.content else {
        return false;
    };
    if !matches!(latest.role, ravenbot_core::MessageRole::User) || text != last_user_message {
        return false;
    }
    if !latest.attachments.is_empty() {
        return false;
    }

    let normalized = normalize_social_text(last_user_message);
    if normalized.is_empty()
        || normalized.chars().count() > 64
        || normalized.split_whitespace().count() > 8
    {
        return false;
    }

    SIMPLE_CONVERSATIONAL_TURNS.contains(&strip_bot_vocative(&normalized, bot_name).as_str())
}

fn normalize_social_text(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn strip_bot_vocative(normalized: &str, bot_name: &str) -> String {
    let alias = normalize_social_text(bot_name);
    if alias.is_empty() || alias == normalized {
        return normalized.to_string();
    }
    if let Some(rest) = normalized
        .strip_prefix(&format!("{alias} "))
        .or_else(|| normalized.strip_suffix(&format!(" {alias}")))
    {
        rest.to_string()
    } else {
        normalized.to_string()
    }
}

fn direct_conversation_history(
    messages: &[ravenbot_core::Message],
    current_user_message: &str,
) -> Vec<ravenbot_models::Message> {
    let mut selected = Vec::new();
    let mut chars = 0;
    let mut skipped_current = false;

    for message in messages.iter().rev() {
        let ravenbot_core::MessageContent::Text { text, .. } = &message.content else {
            continue;
        };
        if text.trim().is_empty() || !message.attachments.is_empty() {
            continue;
        }
        let role = match message.role {
            ravenbot_core::MessageRole::User => "user",
            ravenbot_core::MessageRole::Assistant => "assistant",
            _ => continue,
        };
        if !skipped_current && role == "user" && text == current_user_message {
            skipped_current = true;
            continue;
        }
        chars += text.chars().count();
        selected.push(ravenbot_models::Message::text(role, text));
        if chars >= SIMPLE_HISTORY_CHAR_LIMIT {
            break;
        }
    }

    selected.reverse();
    selected.push(ravenbot_models::Message::text("user", current_user_message));
    selected
}

/// Heuristic: should a model-call failure be retried once automatically?
/// Covers network blips, timeouts, 5xx and rate limits; auth/config errors
/// are not retryable.
fn is_retryable_model_error(err: &str) -> bool {
    let e = err.to_lowercase();
    e.contains("error sending request")
        || e.contains("timed out")
        || e.contains("connection")
        || e.contains("rate limited")
        || e.contains("429")
        || e.contains("500")
        || e.contains("502")
        || e.contains("503")
        || e.contains("504")
        || e.contains("temporarily unavailable")
}

#[cfg(test)]
mod source_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_sources_from_search_results() {
        let value = json!({
            "query": "rust",
            "results": [
                { "title": "Rust Blog", "url": "https://blog.rust-lang.org/x", "snippet": "release notes" },
                { "title": "Crates.io", "url": "https://crates.io/y" },
                { "no_url": true }
            ]
        });
        let mut out = Vec::new();
        extract_sources(&value, &mut out);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].url, "https://blog.rust-lang.org/x");
        assert_eq!(out[0].snippet.as_deref(), Some("release notes"));
        assert_eq!(out[1].title, "Crates.io");
    }

    #[test]
    fn extracts_direct_url_object() {
        let value = json!({ "url": "https://example.com/page", "title": "Example" });
        let mut out = Vec::new();
        extract_sources(&value, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].url, "https://example.com/page");
    }

    #[test]
    fn ignores_non_http_urls() {
        let value = json!({ "url": "file:///etc/passwd", "title": "Local" });
        let mut out = Vec::new();
        extract_sources(&value, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn retryable_heuristic() {
        assert!(is_retryable_model_error("error sending request: connection reset"));
        assert!(is_retryable_model_error("API error 503 Service Unavailable"));
        assert!(is_retryable_model_error("Rate limited, retry after 3 seconds"));
        assert!(!is_retryable_model_error("OpenRouter API key not configured"));
        assert!(!is_retryable_model_error("Unknown provider: localx"));
    }

    // Path naming itself is covered in `ravenbot_core::paths`; this only
    // checks the runtime resolves through to the same helpers, so the
    // workspace a run lands in and the folder the user sees in Settings are
    // produced by one implementation.
    #[test]
    fn default_project_dir_delegates_to_the_shared_workspace_helper() {
        assert_eq!(
            ravenbot_core::slugify("Core Engineering!"),
            ravenbot_core::slugify("Core Engineering!")
        );
        assert_eq!(
            expand_home("/tmp/x"),
            ravenbot_core::expand_home("/tmp/x")
        );
        // A name that cannot be slugified still yields a usable folder name
        // rather than an empty one.
        assert_eq!(ravenbot_core::slugify("  "), "workspace");
    }

    #[test]
    fn host_control_policy_is_known() {
        let policy = host_control_policy();
        assert!(
            policy == "blocked" || policy == "opt_in_required",
            "unexpected policy: {policy}"
        );
    }

    #[test]
    fn extracts_screenshot_data_url_into_an_attachment() {
        let value = json!({
            "data_url": "data:image/png;base64,AAAA",
            "name": "screenshot"
        });
        let (name, data_url) = extract_image(&value).expect("image detected");
        assert_eq!(name, "screenshot");
        let att = image_attachment_from_data_url(&name, &data_url).expect("attachment");
        assert!(att.is_image);
        assert_eq!(att.mime_type, "image/png");
        assert_eq!(att.data.as_deref(), Some("AAAA"));
    }

    #[test]
    fn extracts_base64_image_field_with_mime() {
        let value = json!({ "image_b64": "BBBB", "mime": "image/jpeg" });
        let (_, data_url) = extract_image(&value).expect("image detected");
        assert!(data_url.starts_with("data:image/jpeg;base64,"));
    }

    #[test]
    fn ignores_non_image_tool_results() {
        assert!(extract_image(&json!({ "output": "ok" })).is_none());
        assert!(image_attachment_from_data_url("x", "data:text/plain;base64,AAAA").is_none());
    }

    #[test]
    fn tool_cap_never_drops_protected_tools() {
        // Protected tools sit at the END of assembly order (core skills are
        // added after MCP/plugin tools), exactly the case that used to lose
        // file/shell access when the list was truncated blindly.
        let protected: HashSet<String> = ["file_read", "shell_exec"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let mut items: Vec<(String, usize)> =
            (0..50).map(|i| (format!("connector_tool_{i}"), i)).collect();
        items.push(("file_read".to_string(), 100));
        items.push(("shell_exec".to_string(), 101));

        let capped = cap_preserving_protected(items, &protected, 5, |(id, _)| id.clone());
        let ids: Vec<&str> = capped.iter().map(|(id, _)| id.as_str()).collect();

        assert_eq!(capped.len(), 5, "cap is still enforced");
        assert!(ids.contains(&"file_read"), "core file tool must survive: {ids:?}");
        assert!(ids.contains(&"shell_exec"), "core shell tool must survive: {ids:?}");
        assert_eq!(ids[0], "file_read", "protected tools come first");
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use ravenbot_core::{Bot, Thread};
    use std::path::PathBuf;

    #[tokio::test]
    async fn office_project_folders_drive_working_dirs() {
        use ravenbot_db::queries::{BotQueries, ChatRoomQueries, ThreadQueries};
        let db = temp_db().await;
        let runtime = Runtime::new(db.clone());
        let bot = Bot::new("Dev", "x");
        BotQueries::insert(db.pool(), &bot).await.unwrap();

        let mut room = ravenbot_core::ChatRoom::new("My Office", "", "custom");
        room.project_folders = vec!["/tmp/rb-proj-a".into(), "/tmp/rb-proj-b".into()];
        ChatRoomQueries::create(db.pool(), &room).await.unwrap();

        let thread = Thread::new(bot.id, "project test");
        ThreadQueries::create(db.pool(), &thread).await.unwrap();
        sqlx::query("INSERT INTO chatroom_threads (chatroom_id, thread_id) VALUES (?, ?)")
            .bind(room.id.to_string())
            .bind(thread.id.to_string())
            .execute(db.pool())
            .await
            .unwrap();

        let dirs = runtime.resolve_working_dirs(&bot, thread.id).await;
        assert_eq!(dirs.len(), 2, "both office folders should be used");
        assert_eq!(dirs[0].to_string_lossy(), "/tmp/rb-proj-a");
        assert_eq!(dirs[1].to_string_lossy(), "/tmp/rb-proj-b");
    }

    /// In-memory databases don't round-trip through our `sqlite:{path}?mode=rwc`
    /// URL builder, so integration tests use a unique temp file instead.
    async fn temp_db() -> ravenbot_db::Database {
        redirect_data_root_to_temp();
        let path = PathBuf::from(std::env::temp_dir())
            .join(format!("ravenbot-test-{}.db", Uuid::new_v4()));
        ravenbot_db::Database::new(&path)
            .await
            .expect("temp test database")
    }

    #[tokio::test]
    async fn kill_switch_blocks_run() {
        let db = temp_db().await;
        let runtime = Runtime::new(db);
        runtime.trigger_kill_switch("integration test").await;

        let mut run = ravenbot_core::Run::new(Uuid::new_v4(), Uuid::new_v4());
        let err = runtime.execute_run(&mut run).await.unwrap_err();
        assert!(matches!(err, RuntimeError::KillSwitchActive(_)));
    }

    #[tokio::test]
    async fn missing_bot_fails_cleanly() {
        let db = temp_db().await;
        let runtime = Runtime::new(db);

        let mut run = ravenbot_core::Run::new(Uuid::new_v4(), Uuid::new_v4());
        let err = runtime.execute_run(&mut run).await.unwrap_err();
        assert!(matches!(err, RuntimeError::TaskFailed(_)));
    }

    #[tokio::test]
    async fn unknown_provider_is_model_error() {
        let db = temp_db().await;
        let runtime = Runtime::new(db);

        let mut bot = Bot::new("TestBot", "integration test bot");
        bot.config.model_provider = "bogus-provider".to_string();
        bot.config.model_id = "some/model".to_string();
        ravenbot_db::queries::BotQueries::insert(runtime.db.pool(), &bot)
            .await
            .unwrap();

        let thread = Thread::new(bot.id, "test thread");
        ravenbot_db::queries::ThreadQueries::create(runtime.db.pool(), &thread)
            .await
            .unwrap();
        let msg = ravenbot_core::Message::user(thread.id, "hello");
        ravenbot_db::queries::MessageQueries::insert(runtime.db.pool(), &msg)
            .await
            .unwrap();

        let mut run = ravenbot_core::Run::new(bot.id, thread.id);
        let err = runtime.execute_run(&mut run).await.unwrap_err();
        assert!(matches!(err, RuntimeError::Model(_)));
    }

    /// An agent's stored status has to follow the run.
    ///
    /// `bots.status` and `bots.last_active_at` were only ever written by the
    /// whole-row insert and update, which nothing calls mid-run, so both stayed
    /// at their initial values for the life of every bot. The UI compensated by
    /// deriving state from the event stream, which is right while a window is
    /// open and useless after a restart.
    /// A delegated agent must stay in the office that asked for the work.
    ///
    /// `exec_delegation` created a thread with no `chatroom_threads` row, so
    /// `resolve_working_dirs` walked its whole priority chain and landed on
    /// `default_project_dir(bot.name)` — the target was moved out of the
    /// office into a folder of its own, with no goal, no policy, no roster and
    /// no office memory, and returned work done in the wrong place.
    #[tokio::test]
    async fn a_delegated_thread_is_linked_to_the_office_and_its_workspace() {
        let db = temp_db().await;
        let runtime = Runtime::new(db.clone());

        // An office with a real workspace.
        let mut room = ravenbot_core::ChatRoom::new("Delegation Test", "office", "it-office");
        room.project_folders = vec!["/tmp/rb-deleg-office".to_string()];
        ravenbot_db::queries::ChatRoomQueries::create(db.pool(), &room)
            .await
            .unwrap();

        let lead = Bot::new("Lead", "leads the office");
        ravenbot_db::queries::BotQueries::insert(db.pool(), &lead)
            .await
            .unwrap();

        let office_thread = Thread::new(lead.id, "office group");
        ravenbot_db::queries::ThreadQueries::create(db.pool(), &office_thread)
            .await
            .unwrap();
        sqlx::query("INSERT INTO chatroom_threads (chatroom_id, thread_id, created_at) VALUES (?, ?, ?)")
            .bind(room.id.to_string())
            .bind(office_thread.id.to_string())
            .bind(chrono::Utc::now().to_rfc3339())
            .execute(db.pool())
            .await
            .unwrap();

        let specialist = Bot::new("Specialist", "does the work");
        ravenbot_db::queries::BotQueries::insert(db.pool(), &specialist)
            .await
            .unwrap();
        ravenbot_db::queries::ChatRoomQueries::add_member(
            db.pool(),
            &ravenbot_core::ChatRoomMember {
                chatroom_id: room.id,
                bot_id: specialist.id,
                rank: "Developer".into(),
                specialty: "Implementation".into(),
                joined_at: chrono::Utc::now(),
            },
        )
        .await
        .unwrap();

        let delegate_thread = Thread::new(specialist.id, "will be delegated to");
        ravenbot_db::queries::ThreadQueries::create(db.pool(), &delegate_thread)
            .await
            .unwrap();
        let run = Run::new(specialist.id, delegate_thread.id);
        ravenbot_db::queries::RunQueries::insert(db.pool(), &run).await.unwrap();

        runtime
            .link_thread_to_office(&delegate_thread, &room)
            .await
            .expect("linking the delegated thread");

        // Linked to the room…
        let linked: Option<String> = sqlx::query_scalar(
            "SELECT chatroom_id FROM chatroom_threads WHERE thread_id = ?",
        )
        .bind(delegate_thread.id.to_string())
        .fetch_optional(db.pool())
        .await
        .unwrap();
        assert_eq!(linked, Some(room.id.to_string()));

        // …and carrying the office's folders, so the runtime confines it there.
        let folders: Option<String> =
            sqlx::query_scalar("SELECT project_folders FROM threads WHERE id = ?")
                .bind(delegate_thread.id.to_string())
                .fetch_optional(db.pool())
                .await
                .unwrap();
        let folders: Vec<String> = serde_json::from_str(&folders.unwrap()).unwrap();
        assert_eq!(folders, room.project_folders);
    }

    /// The capability gate refuses a tool the agent's own grant excludes.
    ///
    /// Nothing read `bot.permissions` before, so a list a user had carefully
    /// narrowed in the settings changed nothing at all.
    #[tokio::test]
    async fn a_tool_the_agents_grant_excludes_never_runs() {
        let db = temp_db().await;
        let runtime = Runtime::new(db.clone());

        // Equipped with the skill, but not allowed to use it.
        let mut bot = Bot::new("Narrowed", "cannot shell");
        bot.skills = vec!["shell_exec".to_string()];
        bot.permissions = vec![ravenbot_core::Permission::FileSystem {
            paths: vec!["/".to_string()],
        }];
        ravenbot_db::queries::BotQueries::insert(db.pool(), &bot)
            .await
            .unwrap();

        let thread = Thread::new(bot.id, "capability");
        ravenbot_db::queries::ThreadQueries::create(db.pool(), &thread)
            .await
            .unwrap();

        let skill = ravenbot_skills::SkillRegistry::new_builtin()
            .get("shell_exec")
            .expect("shell_exec is registered");
        let skill_ctx = SkillContext::new(
            bot.id,
            Uuid::new_v4(),
            thread.id,
            ravenbot_core::SandboxTier::OsLevel,
        )
        .with_working_dirs(vec![std::path::PathBuf::from("/tmp")])
        .confined();

        let result = runtime
            .execute_tool_call(
                &bot,
                &Run::new(bot.id, thread.id),
                &[skill],
                &skill_ctx,
                "shell_exec",
                &serde_json::json!({ "command": "touch /tmp/should-not-exist" }),
            )
            .await;

        let err = result["error"]
            .as_str()
            .expect("the call should be refused")
            .to_string();
        assert!(err.contains("shell"), "the refusal should name the missing capability: {err}");
        // The strongest form of the assertion: the command did not run.
        assert!(
            !std::path::Path::new("/tmp/should-not-exist").exists(),
            "a denied tool still executed"
        );
    }

    #[tokio::test]
    async fn a_run_records_the_bots_status_and_last_activity() {
        let db = temp_db().await;
        let runtime = Runtime::new(db.clone());

        let mut bot = Bot::new("Lively", "liveness test");
        // No provider, so the run fails fast — after it has already announced
        // that it was thinking, which is the transition under test.
        bot.config.model_provider = "bogus-provider".to_string();
        bot.config.model_id = "some/model".to_string();
        ravenbot_db::queries::BotQueries::insert(db.pool(), &bot)
            .await
            .unwrap();

        let thread = Thread::new(bot.id, "liveness");
        ravenbot_db::queries::ThreadQueries::create(db.pool(), &thread)
            .await
            .unwrap();
        ravenbot_db::queries::MessageQueries::insert(
            db.pool(),
            &ravenbot_core::Message::user(thread.id, "hello"),
        )
        .await
        .unwrap();

        let mut run = ravenbot_core::Run::new(bot.id, thread.id);
        let _ = runtime.execute_run(&mut run).await;

        // `mark_bot_status` is a detached task, so give the runtime a moment
        // to schedule and run it rather than asserting on a race.
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;

        let stored = ravenbot_db::queries::BotQueries::get(db.pool(), bot.id)
            .await
            .unwrap()
            .expect("bot row");

        assert!(
            stored.last_active_at.is_some(),
            "last_active_at was never written, so an agent that went quiet looks identical to one that just arrived"
        );
        // The run reached "thinking" and never finished, so the honest state is
        // busy, not idle.
        assert_ne!(
            stored.status,
            ravenbot_core::BotStatus::Idle,
            "status stayed idle through a run"
        );
    }

    #[tokio::test]
    async fn announcing_a_finished_run_returns_the_agent_to_idle() {
        let db = temp_db().await;
        let runtime = Runtime::new(db.clone());

        let bot = Bot::new("Finisher", "status round trip");
        ravenbot_db::queries::BotQueries::insert(db.pool(), &bot)
            .await
            .unwrap();

        // Drive the same path `emit` uses, which is the only place status is
        // recorded, so the test fails if that mapping ever drifts.
        runtime.emit(StreamEvent::Status {
            bot_id: bot.id,
            thread_id: Uuid::new_v4(),
            state: "thinking".to_string(),
        });
        runtime.emit(StreamEvent::Status {
            bot_id: bot.id,
            thread_id: Uuid::new_v4(),
            state: "done".to_string(),
        });
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;

        let stored = ravenbot_db::queries::BotQueries::get(db.pool(), bot.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.status, ravenbot_core::BotStatus::Idle);
        assert!(stored.last_active_at.is_some());
    }

    #[tokio::test]
    async fn startup_clears_status_left_behind_by_a_crash() {
        let db = temp_db().await;
        let mut bot = Bot::new("CrashVictim", "left mid-run");
        bot.status = ravenbot_core::BotStatus::Thinking;
        ravenbot_db::queries::BotQueries::insert(db.pool(), &bot)
            .await
            .unwrap();

        let reset = ravenbot_db::queries::BotQueries::mark_all_idle(db.pool())
            .await
            .unwrap();
        assert_eq!(reset, 1, "the busy bot was not reset");

        let stored = ravenbot_db::queries::BotQueries::get(db.pool(), bot.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.status, ravenbot_core::BotStatus::Idle);
    }

    #[tokio::test]
    async fn ephemeral_thread_skips_memory_but_runs() {
        let db = temp_db().await;
        let runtime = Runtime::new(db);

        let mut bot = Bot::new("LocalBot", "integration test bot");
        bot.config.model_provider = "local".to_string();
        ravenbot_db::queries::BotQueries::insert(runtime.db.pool(), &bot)
            .await
            .unwrap();

        let thread = Thread::new_ephemeral(bot.id, "temporary thread");
        ravenbot_db::queries::ThreadQueries::create(runtime.db.pool(), &thread)
            .await
            .unwrap();
        let msg = ravenbot_core::Message::user(thread.id, "hello");
        ravenbot_db::queries::MessageQueries::insert(runtime.db.pool(), &msg)
            .await
            .unwrap();

        let mut run = ravenbot_core::Run::new(bot.id, thread.id);
        // Local provider errors without configured weights, but the ephemeral
        // path must at least surface as a model error, not a config/panic error.
        let err = runtime.execute_run(&mut run).await.unwrap_err();
        assert!(matches!(err, RuntimeError::Model(_)));
    }
}

#[cfg(test)]
mod e2e_tests {
    use super::*;
    use ravenbot_core::{Bot, Thread};
    use sqlx::Row;
    use ravenbot_models::{
        ModelProviderTrait, ModelResponse, Message as ModelMessage, ToolCall, ToolDefinition,
        Usage,
    };
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Scripted provider: round 0 streams reasoning + calls a tool;
    /// round 1 streams the final answer. Captures what it was fed.
    pub(crate) struct MockProvider {
        calls: AtomicUsize,
        /// (role, content, native tool_calls count) per message, per call
        seen_turns: std::sync::Mutex<Vec<Vec<(String, String, usize)>>>,
        seen_enable_reasoning: std::sync::Mutex<Vec<bool>>,
    }

    impl MockProvider {
        pub(crate) fn new() -> Self {
            Self {
                calls: AtomicUsize::new(0),
                seen_turns: std::sync::Mutex::new(Vec::new()),
                seen_enable_reasoning: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait::async_trait]
    impl ModelProviderTrait for MockProvider {
        fn provider_type(&self) -> ravenbot_core::ModelProvider {
            ravenbot_core::ModelProvider::OpenRouter
        }

        fn with_model(self: Box<Self>, _model_id: String) -> Box<dyn ModelProviderTrait> {
            Box::new(*self)
        }

        async fn complete(
            &self,
            _messages: &[ModelMessage],
            _tools: &[ToolDefinition],
            _temperature: f32,
            _max_tokens: u32,
        ) -> Result<ModelResponse, ravenbot_models::ModelError> {
            unreachable!("E2E path uses complete_stream")
        }

        async fn complete_stream(
            &self,
            messages: &[ModelMessage],
            _tools: &[ToolDefinition],
            _temperature: f32,
            _max_tokens: u32,
            on_delta: DeltaCallback,
            enable_reasoning: bool,
        ) -> Result<ModelResponse, ravenbot_models::ModelError> {
            let round = self.calls.fetch_add(1, Ordering::SeqCst);
            self.seen_turns.lock().unwrap().push(
                messages
                    .iter()
                    .map(|m| (m.role.clone(), m.content.clone(), m.tool_calls.len()))
                    .collect(),
            );
            self.seen_enable_reasoning.lock().unwrap().push(enable_reasoning);

            if round == 0 {
                // Reasoning streams inside <think> (as the UI expects)
                on_delta("<think>");
                on_delta("The user wants me to remember rust facts.");
                on_delta("</think>\n\n");
                Ok(ModelResponse {
                    content: None,
                    tool_calls: vec![ToolCall {
                        name: "memory_save".to_string(),
                        arguments: serde_json::json!({
                            "content": "Rust is memory-safe",
                            "importance": 0.6
                        }),
                        id: "call-1".to_string(),
                    }],
                    usage: Usage { input_tokens: 12, output_tokens: 8 },
                    reasoning: Some("The user wants me to remember rust facts.".to_string()),
                })
            } else {
                on_delta("Here");
                on_delta(" is what I found about rust.");
                Ok(ModelResponse {
                    content: Some("Here is what I found about rust.".to_string()),
                    tool_calls: vec![],
                    usage: Usage { input_tokens: 30, output_tokens: 10 },
                    reasoning: Some("Checked the saved memory.".to_string()),
                })
            }
        }

        async fn health_check(&self) -> Result<bool, ravenbot_models::ModelError> {
            Ok(true)
        }
    }

    async fn temp_db() -> ravenbot_db::Database {
        redirect_data_root_to_temp();
        let path = PathBuf::from(std::env::temp_dir())
            .join(format!("ravenbot-e2e-{}.db", Uuid::new_v4()));
        ravenbot_db::Database::new(&path).await.expect("temp db")
    }

    #[tokio::test]
    async fn full_pipeline_streams_executes_tools_and_persists() {
        let db = temp_db().await;
        let runtime = Runtime::new(db.clone());

        let mut bot = Bot::new("E2E", "end-to-end test bot");
        bot.config.model_provider = "openrouter".to_string();
        ravenbot_db::queries::BotQueries::insert(db.pool(), &bot).await.unwrap();

        let thread = Thread::new(bot.id, "e2e thread");
        ravenbot_db::queries::ThreadQueries::create(db.pool(), &thread).await.unwrap();

        let user_msg = ravenbot_core::Message::user(thread.id, "[Think] remember: rust is memory-safe");
        ravenbot_db::queries::MessageQueries::insert(db.pool(), &user_msg).await.unwrap();

        let mock = Arc::new(MockProvider::new());
        runtime.set_provider_override(Some(mock.clone() as Arc<dyn ModelProviderTrait>)).await;
        runtime.set_auto_allow_approvals(true);

        // Collect stream events
        let events: Arc<std::sync::Mutex<Vec<String>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
        let events_cb = events.clone();
        runtime.set_stream_emitter(Some(Arc::new(move |ev: StreamEvent| {
            let label = match &ev {
                StreamEvent::Delta { content, .. } => format!("delta:{}", content),
                StreamEvent::Clear { .. } => "clear".to_string(),
                StreamEvent::ToolStarted { name, .. } => format!("tool_start:{}", name),
                StreamEvent::ToolFinished { name, .. } => format!("tool_end:{}", name),
                StreamEvent::Sources { .. } => "sources".to_string(),
                StreamEvent::Image { name, .. } => format!("image:{}", name),
                StreamEvent::Status { state, .. } => format!("status:{}", state),
                StreamEvent::Usage { tokens, .. } => format!("usage:{}", tokens),
                StreamEvent::ApprovalRequested { .. } => "approval_requested".to_string(),
                StreamEvent::ApprovalDecided { allowed, .. } => {
                    format!("approval_decided:{}", allowed)
                }
                StreamEvent::QuestionAsked { .. } => "question_asked".to_string(),
                StreamEvent::QuestionAnswered { .. } => "question_answered".to_string(),
            };
            events_cb.lock().unwrap().push(label);
        })));

        let mut run = ravenbot_core::Run::new(bot.id, thread.id);
        runtime.execute_run(&mut run).await.expect("run should succeed");

        // 1. Two model rounds happened (tool round + final)
        assert_eq!(mock.calls.load(Ordering::SeqCst), 2);

        // 2. [Think] intent reached the provider as enable_reasoning
        assert_eq!(*mock.seen_enable_reasoning.lock().unwrap(), vec![true, true]);

        // 3. Native tool round-trip: round 2 must carry the assistant turn with
        //    its tool_call ids AND a matching `tool`-role result message.
        let round2 = &mock.seen_turns.lock().unwrap()[1];
        assert!(
            round2
                .iter()
                .any(|(role, _, calls)| role == "assistant" && *calls == 1),
            "assistant turn must carry native tool_calls with ids"
        );
        assert!(
            round2
                .iter()
                .any(|(role, c, _)| role == "tool" && c.contains("Rust is memory-safe")),
            "native tool result must be fed back in a `tool` message"
        );

        // 4. Stream events: reasoning + text deltas, tool lifecycle
        {
            let ev = events.lock().unwrap();
            assert!(ev.iter().any(|e| e.contains("delta:<think>")), "reasoning deltas streamed");
            assert!(ev.iter().any(|e| e.contains("tool_start:memory_save")));
            assert!(ev.iter().any(|e| e.contains("tool_end:memory_save")));
            assert!(ev.iter().any(|e| e.contains("delta:Here")));
        }

        // 5. Final assistant message persisted: <think> reasoning block + content
        let messages = ravenbot_db::queries::MessageQueries::list_by_thread(db.pool(), thread.id)
            .await
            .unwrap();
        let last = messages.last().unwrap();
        let final_text = match &last.content {
            ravenbot_core::MessageContent::Text { text, .. } => text.clone(),
            other => panic!("unexpected content: {other:?}"),
        };
        assert!(final_text.starts_with("<think>"), "reasoning must be persisted: {final_text}");
        assert!(final_text.contains("Here is what I found about rust."));
        assert!(final_text.contains("Checked the saved memory."));

        // 6. Run completed successfully
        assert!(matches!(run.state, ravenbot_core::RunState::Completed));

        // 7. Memory tool actually executed (a fact was saved)
        let facts = sqlx::query("SELECT COUNT(*) as c FROM memory_facts WHERE content LIKE '%memory-safe%'")
            .fetch_one(db.pool())
            .await
            .unwrap();
        let count: i64 = facts.get("c");
        assert!(count >= 1, "memory_save tool must have persisted a fact");

        // 8. Audit log captured the tool call (event stores serialized
        // AuditEventType JSON containing the tool name)
        let rows = sqlx::query("SELECT COUNT(*) as c FROM audit_log WHERE event LIKE '%memory_save%'")
            .fetch_one(db.pool())
            .await
            .unwrap();
        let audit_count: i64 = rows.get("c");
        assert!(audit_count >= 1, "tool call must be audited");
    }
}

#[cfg(test)]
mod honesty_tests {
    use super::*;
    use ravenbot_core::{Bot, Budget, BudgetLimit, BudgetPeriod, Thread};
    use ravenbot_governance::BudgetManager;
    use ravenbot_models::{
        ModelProviderTrait, ModelResponse, Message as ModelMessage, ToolCall, ToolDefinition,
        Usage,
    };
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    async fn temp_db() -> ravenbot_db::Database {
        redirect_data_root_to_temp();
        let path = PathBuf::from(std::env::temp_dir())
            .join(format!("ravenbot-honesty-{}.db", Uuid::new_v4()));
        ravenbot_db::Database::new(&path).await.expect("temp db")
    }

    /// Provider that delegates on round 0 and answers on later rounds.
    struct DelegatingProvider {
        calls: AtomicUsize,
        target_name: String,
    }

    #[async_trait::async_trait]
    impl ModelProviderTrait for DelegatingProvider {
        fn provider_type(&self) -> ravenbot_core::ModelProvider {
            ravenbot_core::ModelProvider::OpenRouter
        }

        fn with_model(self: Box<Self>, _model_id: String) -> Box<dyn ModelProviderTrait> {
            Box::new(*self)
        }

        async fn complete(
            &self,
            _m: &[ModelMessage],
            _t: &[ToolDefinition],
            _temp: f32,
            _max: u32,
        ) -> Result<ModelResponse, ravenbot_models::ModelError> {
            unreachable!()
        }

        async fn complete_stream(
            &self,
            _messages: &[ModelMessage],
            _tools: &[ToolDefinition],
            _temperature: f32,
            _max_tokens: u32,
            on_delta: DeltaCallback,
            _enable_reasoning: bool,
        ) -> Result<ModelResponse, ravenbot_models::ModelError> {
            let round = self.calls.fetch_add(1, Ordering::SeqCst);
            if round == 0 {
                Ok(ModelResponse {
                    content: None,
                    tool_calls: vec![ToolCall {
                        name: "delegate".to_string(),
                        arguments: serde_json::json!({
                            "bot_id": self.target_name,
                            "instruction": "Answer: what is 2+2?"
                        }),
                        id: "call-1".to_string(),
                    }],
                    usage: Usage { input_tokens: 10, output_tokens: 5 },
                    reasoning: None,
                })
            } else {
                let text = if round == 1 {
                    "The answer from the specialist: 4."
                } else {
                    "The specialist answered: 4."
                };
                on_delta(text);
                Ok(ModelResponse {
                    content: Some(text.to_string()),
                    tool_calls: vec![],
                    usage: Usage { input_tokens: 20, output_tokens: 6 },
                    reasoning: None,
                })
            }
        }

        async fn health_check(&self) -> Result<bool, ravenbot_models::ModelError> {
            Ok(true)
        }
    }

    #[tokio::test]
    async fn budget_exhaustion_refuses_run() {
        let db = temp_db().await;
        let runtime = Runtime::new(db.clone());

        let bot = Bot::new("Budgeted", "budget test");
        ravenbot_db::queries::BotQueries::insert(db.pool(), &bot).await.unwrap();

        let budgets = BudgetManager::new(db.pool().clone());
        budgets
            .set_budget(&Budget::new(bot.id, BudgetLimit::Tokens { max: 0 }, BudgetPeriod::Total))
            .await
            .unwrap();

        let thread = Thread::new(bot.id, "budget thread");
        ravenbot_db::queries::ThreadQueries::create(db.pool(), &thread).await.unwrap();
        let msg = ravenbot_core::Message::user(thread.id, "hello");
        ravenbot_db::queries::MessageQueries::insert(db.pool(), &msg).await.unwrap();

        let mut run = ravenbot_core::Run::new(bot.id, thread.id);
        let err = runtime.execute_run(&mut run).await.unwrap_err();
        assert!(matches!(err, RuntimeError::BudgetExceeded(_)));
    }

    #[tokio::test]
    async fn delegation_runs_target_bot_for_real() {
        let db = temp_db().await;
        let runtime = Runtime::new(db.clone());

        let specialist = Bot::new("Specialist", "target bot");
        ravenbot_db::queries::BotQueries::insert(db.pool(), &specialist).await.unwrap();

        let manager_bot = Bot::new("Manager", "delegating bot");
        ravenbot_db::queries::BotQueries::insert(db.pool(), &manager_bot).await.unwrap();

        let thread = Thread::new(manager_bot.id, "delegation thread");
        ravenbot_db::queries::ThreadQueries::create(db.pool(), &thread).await.unwrap();
        let msg = ravenbot_core::Message::user(thread.id, "delegate please");
        ravenbot_db::queries::MessageQueries::insert(db.pool(), &msg).await.unwrap();

        let mock = Arc::new(DelegatingProvider { calls: AtomicUsize::new(0), target_name: "Specialist".to_string() });
        runtime.set_provider_override(Some(mock.clone() as Arc<dyn ModelProviderTrait>)).await;
        runtime.set_auto_allow_approvals(true);

        // Delegation is now scoped: the target has to be on the caller's list.
        // This test predates the check, so it grants it explicitly rather than
        // relying on the old "any bot may reach any bot" behaviour.
        let mut manager = manager_bot.clone();
        manager.delegate_to = vec![specialist.id];
        ravenbot_db::queries::BotQueries::update(db.pool(), &manager)
            .await
            .unwrap();

        let mut run = ravenbot_core::Run::new(manager_bot.id, thread.id);
        runtime.execute_run(&mut run).await.expect("delegating run should succeed");

        // 3 model rounds: parent round 0 (delegate tool), child run, parent final
        assert_eq!(mock.calls.load(Ordering::SeqCst), 3);

        // The specialist actually answered in its own thread
        let specialist_threads = ravenbot_db::queries::ThreadQueries::list_by_bot(db.pool(), specialist.id)
            .await
            .unwrap();
        assert_eq!(specialist_threads.len(), 1, "delegation must create a thread for the target bot");
        let specialist_msgs = ravenbot_db::queries::MessageQueries::list_by_thread(db.pool(), specialist_threads[0].id)
            .await
            .unwrap();
        let specialist_answer = specialist_msgs
            .iter()
            .rev()
            .find(|m| matches!(m.role, ravenbot_core::MessageRole::Assistant))
            .and_then(|m| match &m.content {
                ravenbot_core::MessageContent::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .unwrap_or_default();
        assert!(specialist_answer.contains("4"), "specialist must actually answer: {specialist_answer}");

        // Manager's persisted final message exists (the specialist's answer is
        // fed back in-memory to the parent run — proven by the 3 model rounds
        // and the specialist's own thread above)
    }
}

#[cfg(test)]
mod budget_tracking_tests {
    use super::*;
    use crate::e2e_tests::MockProvider;
    use ravenbot_core::{Bot, Budget, BudgetLimit, BudgetPeriod, Thread};
    use ravenbot_governance::BudgetManager;
    use ravenbot_models::ModelProviderTrait;
    use std::path::PathBuf;

    async fn temp_db() -> ravenbot_db::Database {
        redirect_data_root_to_temp();
        let path = PathBuf::from(std::env::temp_dir())
            .join(format!("ravenbot-budget-{}.db", Uuid::new_v4()));
        ravenbot_db::Database::new(&path).await.expect("temp db")
    }

    #[tokio::test]
    async fn usage_is_actually_tracked_and_trip_budgets() {
        let db = temp_db().await;
        let runtime = Runtime::new(db.clone());

        let bot = Bot::new("Tracker", "budget tracking test");
        ravenbot_db::queries::BotQueries::insert(db.pool(), &bot).await.unwrap();

        let budgets = BudgetManager::new(db.pool().clone());
        // The scripted mock spends exactly 60 tokens (20 + 40)
        budgets
            .set_budget(&Budget::new(bot.id, BudgetLimit::Tokens { max: 59 }, BudgetPeriod::Total))
            .await
            .unwrap();

        let thread = Thread::new(bot.id, "budget tracking thread");
        ravenbot_db::queries::ThreadQueries::create(db.pool(), &thread).await.unwrap();
        // Use task-like text so this exercises the full tool loop rather than
        // the standalone-greeting fast path.
        let msg = ravenbot_core::Message::user(thread.id, "remember this");
        ravenbot_db::queries::MessageQueries::insert(db.pool(), &msg).await.unwrap();

        runtime.set_provider_override(Some(Arc::new(MockProvider::new()) as Arc<dyn ModelProviderTrait>)).await;
        runtime.set_auto_allow_approvals(true);

        let mut run = ravenbot_core::Run::new(bot.id, thread.id);
        runtime.execute_run(&mut run).await.expect("first run under budget");

        // Usage MUST be recorded (was a no-op stub before)
        let (tokens_used, _) = budgets.get_usage(bot.id).await.unwrap();
        assert_eq!(tokens_used, 60, "record_usage must accumulate real tokens");

        // The budget is now exhausted: next run must be refused
        let check = budgets.check_budget(bot.id).await.unwrap();
        assert!(!check.allowed, "59-token budget must be exhausted after 60 tokens");
        assert!(check.percentage_used >= 100.0);

        // Reset works
        budgets.reset_usage(bot.id).await.unwrap();
        let check_after = budgets.check_budget(bot.id).await.unwrap();
        assert!(check_after.allowed);
    }
}

/// Point the data root at a temporary directory for the whole test binary.
///
/// Without this, every test that runs an agent falls through
/// `resolve_working_dirs` to `default_project_dir(bot.name)`, which creates
/// `~/RAVENBOT/projects/<bot>/` — the *live* data directory. Running the suite
/// once left 273 directories there, and because the repository is checked out
/// at exactly that path, they also showed up as untracked files in the working
/// tree.
///
/// Each crate's tests are a separate OS process, so setting the variable here
/// cannot affect `ravenbot-core`'s own `paths` tests, which save and restore it
/// around their own assertions. Within this binary the value is the same for
/// every call, so the write is idempotent and the mutex only exists to keep two
/// threads from writing the environment at the same moment. Every test that can
/// create a directory does so via a run, and every test that runs calls
/// [`temp_db`] first, so the variable is always in place before anything reads
/// it.
fn redirect_data_root_to_temp() {
    use std::sync::OnceLock;
    static INIT: OnceLock<()> = OnceLock::new();
    INIT.get_or_init(|| {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("ravenbot-test-root-{}", uuid::Uuid::new_v4()));
        std::env::set_var("RAVENBOT_HOME", &dir);
    });
}

#[cfg(test)]
mod parity_tests {
    use super::*;
    use ravenbot_core::{Bot, Thread};
    use ravenbot_models::{
        ModelProviderTrait, ModelResponse, Message as ModelMessage, ToolCall, ToolDefinition, Usage,
    };
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    async fn temp_db() -> ravenbot_db::Database {
        redirect_data_root_to_temp();
        let path = PathBuf::from(std::env::temp_dir())
            .join(format!("ravenbot-parity-{}.db", Uuid::new_v4()));
        ravenbot_db::Database::new(&path).await.expect("temp db")
    }

    /// Returns scripted rounds in order and captures what it was fed.
    struct ScriptProvider {
        calls: AtomicUsize,
        script: Vec<ModelResponse>,
        seen: std::sync::Mutex<Vec<Vec<(String, String)>>>,
        tool_counts: std::sync::Mutex<Vec<usize>>,
    }

    impl ScriptProvider {
        fn new(script: Vec<ModelResponse>) -> Self {
            Self {
                calls: AtomicUsize::new(0),
                script,
                seen: std::sync::Mutex::new(Vec::new()),
                tool_counts: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait::async_trait]
    impl ModelProviderTrait for ScriptProvider {
        fn provider_type(&self) -> ravenbot_core::ModelProvider {
            ravenbot_core::ModelProvider::OpenRouter
        }
        fn with_model(self: Box<Self>, _m: String) -> Box<dyn ModelProviderTrait> {
            Box::new(*self)
        }
        async fn complete(
            &self,
            _m: &[ModelMessage],
            _t: &[ToolDefinition],
            _temp: f32,
            _max: u32,
        ) -> Result<ModelResponse, ravenbot_models::ModelError> {
            unreachable!("uses complete_stream")
        }
        async fn complete_stream(
            &self,
            messages: &[ModelMessage],
            tools: &[ToolDefinition],
            _temperature: f32,
            _max_tokens: u32,
            on_delta: DeltaCallback,
            _enable_reasoning: bool,
        ) -> Result<ModelResponse, ravenbot_models::ModelError> {
            let round = self.calls.fetch_add(1, Ordering::SeqCst);
            self.tool_counts.lock().unwrap().push(tools.len());
            self.seen.lock().unwrap().push(
                messages
                    .iter()
                    .map(|m| (m.role.clone(), m.content.clone()))
                    .collect(),
            );
            let resp = self
                .script
                .get(round)
                .cloned()
                .unwrap_or_else(|| ModelResponse {
                    content: Some("done".to_string()),
                    tool_calls: vec![],
                    usage: Usage { input_tokens: 1, output_tokens: 1 },
                    reasoning: None,
                });
            if let Some(text) = &resp.content {
                if !text.is_empty() {
                    on_delta(text);
                }
            }
            Ok(resp)
        }
        async fn health_check(&self) -> Result<bool, ravenbot_models::ModelError> {
            Ok(true)
        }
    }

    fn tool_call(name: &str, args: serde_json::Value, id: &str) -> ToolCall {
        ToolCall { name: name.to_string(), arguments: args, id: id.to_string() }
    }

    async fn seed_run(
        db: &ravenbot_db::Database,
        bot: &Bot,
    ) -> (Thread, ravenbot_core::Run) {
        let thread = Thread::new(bot.id, "parity thread");
        ravenbot_db::queries::ThreadQueries::create(db.pool(), &thread)
            .await
            .unwrap();
        let msg = ravenbot_core::Message::user(thread.id, "go");
        ravenbot_db::queries::MessageQueries::insert(db.pool(), &msg)
            .await
            .unwrap();
        let run = ravenbot_core::Run::new(bot.id, thread.id);
        (thread, run)
    }

    #[tokio::test]
    async fn ask_user_parks_and_resumes_with_the_answer() {
        let db = temp_db().await;
        let runtime = Arc::new(Runtime::new(db.clone()));

        let bot = Bot::new("Asker", "asks questions");
        ravenbot_db::queries::BotQueries::insert(db.pool(), &bot).await.unwrap();
        let (thread, _) = seed_run(&db, &bot).await;

        let script = vec![
            ModelResponse {
                content: None,
                tool_calls: vec![tool_call(
                    "ask_user",
                    serde_json::json!({
                        "question": "Which database should I use?",
                        "header": "Database",
                        "options": ["Postgres", "SQLite"]
                    }),
                    "call-ask",
                )],
                usage: Usage { input_tokens: 5, output_tokens: 2 },
                reasoning: None,
            },
            ModelResponse {
                content: Some("Understood.".to_string()),
                tool_calls: vec![],
                usage: Usage { input_tokens: 6, output_tokens: 2 },
                reasoning: None,
            },
        ];
        let provider = Arc::new(ScriptProvider::new(script));
        runtime
            .set_provider_override(Some(provider.clone() as Arc<dyn ModelProviderTrait>))
            .await;
        // Interactive: do NOT auto-allow, so the question actually parks.

        let mut run = ravenbot_core::Run::new(bot.id, thread.id);
        let run_id = run.id;
        let rt = runtime.clone();
        let handle = tokio::spawn(async move { rt.execute_run(&mut run).await });

        // Wait for the question to park, then answer it.
        let mut answered = false;
        for _ in 0..100 {
            let pending =
                ravenbot_db::queries::QuestionQueries::list_pending_for_thread(db.pool(), thread.id)
                    .await
                    .unwrap();
            if let Some(q) = pending.first() {
                assert!(q.question.contains("Which database"));
                assert_eq!(q.options, vec!["Postgres", "SQLite"]);
                ravenbot_db::queries::QuestionQueries::answer(db.pool(), q.id, "Postgres")
                    .await
                    .unwrap();
                answered = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        assert!(answered, "ask_user must park a pending question");
        assert_ne!(run_id, Uuid::nil());

        handle.await.unwrap().expect("run resumes after the answer");

        // The answer is fed back to the model as the native tool result.
        let seen = provider.seen.lock().unwrap();
        let round2 = &seen[1];
        assert!(
            round2
                .iter()
                .any(|(role, c)| role == "tool" && c.contains("Postgres")),
            "answer must be fed back as a tool result: {round2:?}"
        );
    }

    #[tokio::test]
    async fn parallel_tool_calls_all_execute_and_round_trip() {
        let db = temp_db().await;
        let runtime = Runtime::new(db.clone());

        let bot = Bot::new("Parallel", "multi tool");
        ravenbot_db::queries::BotQueries::insert(db.pool(), &bot).await.unwrap();
        let (thread, _) = seed_run(&db, &bot).await;

        let script = vec![
            ModelResponse {
                content: None,
                tool_calls: vec![
                    tool_call(
                        "memory_save",
                        serde_json::json!({ "content": "fact one" }),
                        "call-1",
                    ),
                    tool_call(
                        "memory_save",
                        serde_json::json!({ "content": "fact two" }),
                        "call-2",
                    ),
                ],
                usage: Usage { input_tokens: 5, output_tokens: 3 },
                reasoning: None,
            },
            ModelResponse {
                content: Some("Saved both.".to_string()),
                tool_calls: vec![],
                usage: Usage { input_tokens: 7, output_tokens: 2 },
                reasoning: None,
            },
        ];
        let provider = Arc::new(ScriptProvider::new(script));
        runtime
            .set_provider_override(Some(provider.clone() as Arc<dyn ModelProviderTrait>))
            .await;
        runtime.set_auto_allow_approvals(true);

        let mut run = ravenbot_core::Run::new(bot.id, thread.id);
        runtime.execute_run(&mut run).await.expect("multi-tool run");

        let seen = provider.seen.lock().unwrap();
        let round2 = &seen[1];
        let tool_msgs: Vec<&(String, String)> = round2.iter().filter(|(r, _)| r == "tool").collect();
        assert_eq!(tool_msgs.len(), 2, "both tool results must be fed back");
        assert!(tool_msgs.iter().any(|(_, c)| c.contains("fact one")));
        assert!(tool_msgs.iter().any(|(_, c)| c.contains("fact two")));
    }

    /// Full runtime path on an external engine: a fake `claude` CLI emits
    /// stream-json, and the runtime must stream + persist the turn.
    #[cfg(unix)]
    #[tokio::test]
    async fn engine_run_streams_and_persists_assistant_message() {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;

        let script_path = std::env::temp_dir().join(format!(
            "ravenbot-fake-claude-rt-{}.sh",
            Uuid::new_v4()
        ));
        let frames = [
            r#"{"type":"system","subtype":"init","session_id":"s1","model":"m"}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"engine says hi"}}}"#,
            r#"{"type":"result","is_error":false,"total_cost_usd":0.0,"usage":{"input_tokens":3,"output_tokens":2}}"#,
        ]
        .join("\n");
        let mut file = std::fs::File::create(&script_path).unwrap();
        writeln!(file, "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{}'", frames.replace('\'', "'\\''")).unwrap();
        drop(file);
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("RAVENBOT_CLAUDE_CMD", script_path.to_string_lossy().to_string());

        let db = temp_db().await;
        let runtime = Runtime::new(db.clone());

        let mut bot = Bot::new("EngineBot", "runs on claude cli");
        bot.config.engine = "claude".to_string();
        bot.config.model_id = String::new();
        ravenbot_db::queries::BotQueries::insert(db.pool(), &bot).await.unwrap();
        let (thread, mut run) = seed_run(&db, &bot).await;

        let stream_events: Arc<std::sync::Mutex<Vec<String>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
        let ev = stream_events.clone();
        runtime.set_stream_emitter(Some(Arc::new(move |e: StreamEvent| {
            if let StreamEvent::Delta { content, .. } = e {
                ev.lock().unwrap().push(content);
            }
        })));

        runtime.execute_run(&mut run).await.expect("engine run succeeds");
        std::env::remove_var("RAVENBOT_CLAUDE_CMD");
        let _ = std::fs::remove_file(&script_path);

        // The answer streamed…
        assert!(
            stream_events.lock().unwrap().join("").contains("engine says hi"),
            "engine text must stream to the UI"
        );

        // …and was persisted as the assistant message.
        let messages = ravenbot_db::queries::MessageQueries::list_by_thread(db.pool(), thread.id)
            .await
            .unwrap();
        let last = messages.last().unwrap();
        let text = match &last.content {
            ravenbot_core::MessageContent::Text { text, .. } => text.clone(),
            other => panic!("unexpected content: {other:?}"),
        };
        assert!(text.contains("engine says hi"), "persisted: {text}");
    }

    /// Provider fallback: when the primary errors, `call_model` moves to the
    /// secondary and reports which index answered.
    #[tokio::test]
    async fn call_model_falls_back_to_secondary_provider() {
        struct FailProvider;
        #[async_trait::async_trait]
        impl ModelProviderTrait for FailProvider {
            fn provider_type(&self) -> ravenbot_core::ModelProvider {
                ravenbot_core::ModelProvider::OpenAI
            }
            fn with_model(self: Box<Self>, _m: String) -> Box<dyn ModelProviderTrait> {
                self
            }
            async fn complete(
                &self,
                _m: &[ModelMessage],
                _t: &[ToolDefinition],
                _temp: f32,
                _max: u32,
            ) -> Result<ModelResponse, ravenbot_models::ModelError> {
                Err(ravenbot_models::ModelError::Auth("primary down".into()))
            }
            async fn complete_stream(
                &self,
                _m: &[ModelMessage],
                _t: &[ToolDefinition],
                _temp: f32,
                _max: u32,
                _cb: DeltaCallback,
                _r: bool,
            ) -> Result<ModelResponse, ravenbot_models::ModelError> {
                Err(ravenbot_models::ModelError::Auth("primary down".into()))
            }
            async fn health_check(&self) -> Result<bool, ravenbot_models::ModelError> {
                Ok(false)
            }
        }

        struct OkProvider;
        #[async_trait::async_trait]
        impl ModelProviderTrait for OkProvider {
            fn provider_type(&self) -> ravenbot_core::ModelProvider {
                ravenbot_core::ModelProvider::Anthropic
            }
            fn with_model(self: Box<Self>, _m: String) -> Box<dyn ModelProviderTrait> {
                self
            }
            async fn complete(
                &self,
                _m: &[ModelMessage],
                _t: &[ToolDefinition],
                _temp: f32,
                _max: u32,
            ) -> Result<ModelResponse, ravenbot_models::ModelError> {
                Ok(ModelResponse {
                    content: Some("fallback answer".into()),
                    tool_calls: vec![],
                    usage: Usage { input_tokens: 1, output_tokens: 1 },
                    reasoning: None,
                })
            }
            async fn complete_stream(
                &self,
                _m: &[ModelMessage],
                _t: &[ToolDefinition],
                _temp: f32,
                _max: u32,
                _cb: DeltaCallback,
                _r: bool,
            ) -> Result<ModelResponse, ravenbot_models::ModelError> {
                Ok(ModelResponse {
                    content: Some("fallback answer".into()),
                    tool_calls: vec![],
                    usage: Usage { input_tokens: 1, output_tokens: 1 },
                    reasoning: None,
                })
            }
            async fn health_check(&self) -> Result<bool, ravenbot_models::ModelError> {
                Ok(true)
            }
        }

        let db = temp_db().await;
        let runtime = Runtime::new(db.clone());
        let chain: Vec<Arc<dyn ModelProviderTrait>> =
            vec![Arc::new(FailProvider), Arc::new(OkProvider)];
        let cb: DeltaCallback = Arc::new(|_| {});
        let (resp, idx) = runtime
            .call_model(&chain, 0, &[], &[], 0.0, 100, cb, false)
            .await
            .expect("fallback should answer");
        assert_eq!(idx, 1, "must report the fallback provider index");
        assert_eq!(resp.content.as_deref(), Some("fallback answer"));
    }

    /// Context compaction keeps the system prompt + newest messages, drops the
    /// middle, and never orphans a tool result.
    #[test]
    fn compaction_drops_middle_and_keeps_recent() {
        fn msg(role: &str, text: &str) -> ravenbot_models::Message {
            ravenbot_models::Message::text(role, text)
        }
        let mut messages = vec![msg("system", "SYSTEM")];
        for i in 0..40 {
            messages.push(msg("user", &format!("filler {} {}", i, "x".repeat(2000))));
            messages.push(msg("assistant", &format!("reply {} {}", i, "y".repeat(2000))));
        }
        messages.push(msg("user", "THE MOST RECENT QUESTION"));
        let before = messages.len();

        // Tiny window forces aggressive compaction down to the keep-tail floor.
        compact_messages(&mut messages, 700, 200);

        assert!(messages.len() < before, "should have dropped the middle");
        assert!(messages.len() <= COMPACT_KEEP_TAIL + 2, "kept tail floor: {}", messages.len());
        assert_eq!(messages[0].role, "system");
        assert!(messages.iter().any(|m| m.role == "user" && m.content.contains("compacted")));
        assert_eq!(messages.last().unwrap().content, "THE MOST RECENT QUESTION");

        // The kept tail must not begin on an orphaned tool result.
        assert_ne!(messages[1].role, "tool");
    }

    #[test]
    fn recognizes_standalone_social_turns() {
        fn history(texts: &[&str]) -> Vec<ravenbot_core::Message> {
            texts
                .iter()
                .map(|text| ravenbot_core::Message::user(Uuid::new_v4(), *text))
                .collect()
        }

        assert!(is_simple_conversational_turn(
            "RANO",
            "how are you ?",
            &history(&["how are you ?"])
        ));
        assert!(is_simple_conversational_turn(
            "Night Agent",
            "Hello Night Agent!",
            &history(&["Hello Night Agent!"])
        ));
        assert!(is_simple_conversational_turn(
            "RANO",
            "thanks",
            &history(&["Previous task", "thanks"])
        ));

        assert!(!is_simple_conversational_turn(
            "RANO",
            "hi, summarize this repository",
            &history(&["hi, summarize this repository"])
        ));
        assert!(!is_simple_conversational_turn(
            "RANO",
            "yes",
            &history(&["yes"])
        ));
        assert!(!is_simple_conversational_turn(
            "RANO",
            "[Think] hi",
            &history(&["[Think] hi"])
        ));
    }

    #[test]
    fn attachments_and_stale_history_need_the_full_agent_loop() {
        let mut attached =
            ravenbot_core::Message::user(Uuid::new_v4(), "how are you ?");
        attached.attachments.push(ravenbot_core::Attachment {
            id: Uuid::new_v4(),
            name: "screenshot.png".to_string(),
            mime_type: "image/png".to_string(),
            size: 4,
            path: String::new(),
            data: Some("AAAA".to_string()),
            is_image: true,
        });
        assert!(!is_simple_conversational_turn("RANO", "how are you ?", &[attached]));

        let stale = vec![
            ravenbot_core::Message::user(Uuid::new_v4(), "how are you ?"),
            ravenbot_core::Message::assistant(Uuid::new_v4(), "Fine."),
        ];
        assert!(!is_simple_conversational_turn("RANO", "how are you ?", &stale));
    }

    /// Pause parks the run at a tool-round boundary with a checkpoint; resume
    /// re-enters from that checkpoint and finishes.
    #[tokio::test]
    async fn pause_then_resume_continues_from_checkpoint() {
        struct PauseProvider {
            runtime: Arc<Runtime>,
            run_id: Uuid,
            calls: AtomicUsize,
        }
        #[async_trait::async_trait]
        impl ModelProviderTrait for PauseProvider {
            fn provider_type(&self) -> ravenbot_core::ModelProvider {
                ravenbot_core::ModelProvider::OpenRouter
            }
            fn with_model(self: Box<Self>, _m: String) -> Box<dyn ModelProviderTrait> {
                Box::new(*self)
            }
            async fn complete(
                &self,
                _m: &[ModelMessage],
                _t: &[ToolDefinition],
                _temp: f32,
                _max: u32,
            ) -> Result<ModelResponse, ravenbot_models::ModelError> {
                unreachable!()
            }
            async fn complete_stream(
                &self,
                _m: &[ModelMessage],
                _t: &[ToolDefinition],
                _temp: f32,
                _max: u32,
                _cb: DeltaCallback,
                _r: bool,
            ) -> Result<ModelResponse, ravenbot_models::ModelError> {
                let round = self.calls.fetch_add(1, Ordering::SeqCst);
                if round == 0 {
                    // Ask for a pause, then request a tool so the checkpoint is
                    // meaningful on the next boundary.
                    self.runtime.request_pause(self.run_id);
                    Ok(ModelResponse {
                        content: None,
                        tool_calls: vec![tool_call(
                            "memory_save",
                            serde_json::json!({ "content": "paused fact" }),
                            "pause-1",
                        )],
                        usage: Usage { input_tokens: 2, output_tokens: 1 },
                        reasoning: None,
                    })
                } else {
                    Ok(ModelResponse {
                        content: Some("resumed and finished".to_string()),
                        tool_calls: vec![],
                        usage: Usage { input_tokens: 3, output_tokens: 2 },
                        reasoning: None,
                    })
                }
            }
            async fn health_check(&self) -> Result<bool, ravenbot_models::ModelError> {
                Ok(true)
            }
        }

        let db = temp_db().await;
        let runtime = Arc::new(Runtime::new(db.clone()));
        let bot = Bot::new("Resumable", "pause/resume test");
        ravenbot_db::queries::BotQueries::insert(db.pool(), &bot).await.unwrap();
        let (thread, mut run) = seed_run(&db, &bot).await;
        let run_id = run.id;

        let provider = Arc::new(PauseProvider {
            runtime: runtime.clone(),
            run_id,
            calls: AtomicUsize::new(0),
        });
        runtime
            .set_provider_override(Some(provider.clone() as Arc<dyn ModelProviderTrait>))
            .await;
        runtime.set_auto_allow_approvals(true);

        // First execution pauses at the loop boundary.
        runtime.execute_run(&mut run).await.expect("first run parks");
        assert_eq!(run.state, ravenbot_core::RunState::Paused, "run must be Paused");
        assert!(run.checkpoint.is_some(), "a resume checkpoint must be written");

        // Resume: restores the checkpoint and completes.
        runtime.execute_run(&mut run).await.expect("resume completes");
        assert_eq!(run.state, ravenbot_core::RunState::Completed);
        assert_eq!(provider.calls.load(Ordering::SeqCst), 2, "one call per phase");

        let messages = ravenbot_db::queries::MessageQueries::list_by_thread(db.pool(), thread.id)
            .await
            .unwrap();
        let last = messages.last().unwrap();
        let text = match &last.content {
            ravenbot_core::MessageContent::Text { text, .. } => text.clone(),
            other => panic!("unexpected content: {other:?}"),
        };
        assert!(text.contains("resumed and finished"), "persisted: {text}");
    }

    #[tokio::test]
    async fn simple_greeting_bypasses_tool_discovery() {
        let db = temp_db().await;
        let runtime = Runtime::new(db.clone());

        let bot = Bot::new("RANO", "casual replies");
        ravenbot_db::queries::BotQueries::insert(db.pool(), &bot).await.unwrap();
        let thread = Thread::new(bot.id, "greeting thread");
        ravenbot_db::queries::ThreadQueries::create(db.pool(), &thread)
            .await
            .unwrap();
        let user_message = ravenbot_core::Message::user(thread.id, "how are you ?");
        ravenbot_db::queries::MessageQueries::insert(db.pool(), &user_message)
            .await
            .unwrap();

        let provider = Arc::new(ScriptProvider::new(vec![ModelResponse {
            content: Some("Doing well, thanks!".to_string()),
            tool_calls: vec![],
            usage: Usage { input_tokens: 8, output_tokens: 4 },
            reasoning: None,
        }]));
        runtime
            .set_provider_override(Some(provider.clone() as Arc<dyn ModelProviderTrait>))
            .await;

        let mut run = ravenbot_core::Run::new(bot.id, thread.id);
        runtime.execute_run(&mut run).await.expect("fast greeting run");

        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        assert_eq!(*provider.tool_counts.lock().unwrap(), vec![0]);

        let messages = ravenbot_db::queries::MessageQueries::list_by_thread(db.pool(), thread.id)
            .await
            .unwrap();
        let last = messages.last().expect("assistant message");
        let assistant_text = match &last.content {
            ravenbot_core::MessageContent::Text { text, .. } => text.clone(),
            other => panic!("unexpected assistant content: {other:?}"),
        };
        assert!(assistant_text.contains("Doing well"));
        assert!(matches!(
            run.outcome,
            Some(ravenbot_core::RunOutcome::Success { .. })
        ));
    }

    #[tokio::test]
    async fn empty_lightweight_reply_falls_back_to_full_loop() {
        let db = temp_db().await;
        let runtime = Runtime::new(db.clone());

        let bot = Bot::new("RANO", "casual replies");
        ravenbot_db::queries::BotQueries::insert(db.pool(), &bot).await.unwrap();
        let thread = Thread::new(bot.id, "empty greeting thread");
        ravenbot_db::queries::ThreadQueries::create(db.pool(), &thread)
            .await
            .unwrap();
        let user_message = ravenbot_core::Message::user(thread.id, "hi");
        ravenbot_db::queries::MessageQueries::insert(db.pool(), &user_message)
            .await
            .unwrap();

        let provider = Arc::new(ScriptProvider::new(vec![
            ModelResponse {
                content: None,
                tool_calls: vec![],
                usage: Usage { input_tokens: 2, output_tokens: 0 },
                reasoning: None,
            },
            ModelResponse {
                content: Some("Recovered.".to_string()),
                tool_calls: vec![],
                usage: Usage { input_tokens: 3, output_tokens: 1 },
                reasoning: None,
            },
        ]));
        runtime
            .set_provider_override(Some(provider.clone() as Arc<dyn ModelProviderTrait>))
            .await;

        let mut run = ravenbot_core::Run::new(bot.id, thread.id);
        runtime.execute_run(&mut run).await.expect("fallback run");
        assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
        assert_eq!(provider.tool_counts.lock().unwrap().len(), 2);
        assert!(provider.tool_counts.lock().unwrap()[1] > 0);

        let messages = ravenbot_db::queries::MessageQueries::list_by_thread(db.pool(), thread.id)
            .await
            .unwrap();
        let assistant_text = match &messages.last().expect("assistant message").content {
            ravenbot_core::MessageContent::Text { text, .. } => text.clone(),
            other => panic!("unexpected assistant content: {other:?}"),
        };
        assert!(assistant_text.contains("Recovered"));
    }

    #[tokio::test]
    async fn cancellation_stops_the_run_cleanly() {
        let db = temp_db().await;
        let runtime = Runtime::new(db.clone());

        let bot = Bot::new("Cancellable", "cancel test");
        ravenbot_db::queries::BotQueries::insert(db.pool(), &bot).await.unwrap();
        let (_thread, mut run) = seed_run(&db, &bot).await;

        let script = vec![ModelResponse {
            content: None,
            tool_calls: vec![tool_call(
                "memory_save",
                serde_json::json!({ "content": "should not run" }),
                "call-x",
            )],
            usage: Usage { input_tokens: 1, output_tokens: 1 },
            reasoning: None,
        }];
        runtime
            .set_provider_override(Some(Arc::new(ScriptProvider::new(script)) as Arc<dyn ModelProviderTrait>))
            .await;
        runtime.set_auto_allow_approvals(true);

        // Cancel before execution: the loop must stop at its first boundary.
        runtime.request_cancel(run.id);
        runtime.execute_run(&mut run).await.expect("cancel returns Ok");

        assert!(
            matches!(run.outcome, Some(ravenbot_core::RunOutcome::Cancelled { .. })),
            "run must be marked Cancelled, got {:?}",
            run.outcome
        );
    }
}

#[cfg(test)]
mod office_tests {
    use super::*;
    use ravenbot_core::{Bot, ChatRoom, ChatRoomMember};
    use ravenbot_models::{
        ModelProviderTrait, ModelResponse, Message as ModelMessage, ToolDefinition, Usage,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    async fn temp_db() -> ravenbot_db::Database {
        redirect_data_root_to_temp();
        let path = std::path::PathBuf::from(std::env::temp_dir())
            .join(format!("ravenbot-office-{}.db", Uuid::new_v4()));
        ravenbot_db::Database::new(&path).await.expect("temp db")
    }

    /// Scripts planning JSON on the first call, a synthesized answer afterwards.
    struct OfficeProvider {
        calls: AtomicUsize,
        plan_json: String,
    }

    #[async_trait::async_trait]
    impl ModelProviderTrait for OfficeProvider {
        fn provider_type(&self) -> ravenbot_core::ModelProvider {
            ravenbot_core::ModelProvider::OpenRouter
        }
        fn with_model(self: Box<Self>, _m: String) -> Box<dyn ModelProviderTrait> {
            Box::new(*self)
        }
        async fn complete(
            &self,
            _m: &[ModelMessage],
            _t: &[ToolDefinition],
            _temp: f32,
            _max: u32,
        ) -> Result<ModelResponse, ravenbot_models::ModelError> {
            unreachable!()
        }
        async fn complete_stream(
            &self,
            _m: &[ModelMessage],
            _t: &[ToolDefinition],
            _temp: f32,
            _max: u32,
            on_delta: DeltaCallback,
            _r: bool,
        ) -> Result<ModelResponse, ravenbot_models::ModelError> {
            let n = self.calls.fetch_add(1, Ordering::SeqCst);
            // First call is the planner; every later call is a node run or the
            // synthesizer — both return text.
            let text = if n == 0 {
                self.plan_json.clone()
            } else if n == 1 {
                "drafted the campaign".to_string()
            } else {
                "Here is the integrated final answer.".to_string()
            };
            on_delta(&text);
            Ok(ModelResponse {
                content: Some(text),
                tool_calls: vec![],
                usage: Usage { input_tokens: 1, output_tokens: 1 },
                reasoning: None,
            })
        }
        async fn health_check(&self) -> Result<bool, ravenbot_models::ModelError> {
            Ok(true)
        }
    }

    async fn seed_office(db: &ravenbot_db::Database) -> (Uuid, uuid::Uuid, uuid::Uuid) {
        let mut lead = Bot::new("Chief of Staff", "lead");
        lead.is_orchestrator = true;
        ravenbot_db::queries::BotQueries::insert(db.pool(), &lead).await.unwrap();
        let worker = Bot::new("Growth Marketer", "worker");
        ravenbot_db::queries::BotQueries::insert(db.pool(), &worker).await.unwrap();

        let room = ChatRoom::new("Growth Office", "test", "marketing");
        ravenbot_db::queries::ChatRoomQueries::create(db.pool(), &room).await.unwrap();
        for (bot, rank) in [(&lead, "lead"), (&worker, "specialist")] {
            let m = ChatRoomMember {
                chatroom_id: room.id,
                bot_id: bot.id,
                rank: rank.to_string(),
                specialty: "growth".to_string(),
                joined_at: chrono::Utc::now(),
            };
            ravenbot_db::queries::ChatRoomQueries::add_member(db.pool(), &m).await.unwrap();
        }
        (room.id, lead.id, worker.id)
    }

    #[tokio::test]
    async fn plan_office_parses_model_plan() {
        let db = temp_db().await;
        let runtime = Runtime::new(db.clone());
        let (_, lead_id, _) = seed_office(&db).await;
        let plan_json = r#"{"tasks":[{"bot":"Growth Marketer","instruction":"Draft the campaign","depends_on":[]},{"bot":"Chief of Staff","instruction":"Review it","depends_on":[0]}],"final_summary_from":"Chief of Staff"}"#;
        runtime
            .set_provider_override(Some(Arc::new(OfficeProvider {
                calls: AtomicUsize::new(0),
                plan_json: plan_json.to_string(),
            }) as Arc<dyn ModelProviderTrait>))
            .await;

        let members = vec![
            crate::orchestrator::OfficeMember {
                bot_id: Uuid::new_v4(),
                name: "Chief of Staff".into(),
                rank: "lead".into(),
                specialty: "growth".into(),
            },
            crate::orchestrator::OfficeMember {
                bot_id: Uuid::new_v4(),
                name: "Growth Marketer".into(),
                rank: "specialist".into(),
                specialty: "growth".into(),
            },
        ];
        let plan = runtime
            .plan_office(
                lead_id,
                Some("Ship Q3"),
                None,
                &members,
                "Launch a blog",
            )
            .await
            .expect("plan");
        assert_eq!(plan.tasks.len(), 2);
        assert_eq!(plan.tasks[1].depends_on, vec![0]);
    }

    #[tokio::test]
    async fn synthesize_office_uses_the_lead_provider() {
        let db = temp_db().await;
        let runtime = Runtime::new(db.clone());
        let (_, lead_id, _) = seed_office(&db).await;
        runtime
            .set_provider_override(Some(Arc::new(OfficeProvider {
                calls: AtomicUsize::new(99), // not the planner
                plan_json: String::new(),
            }) as Arc<dyn ModelProviderTrait>))
            .await;

        let results = vec![("Growth Marketer".to_string(), "drafted".to_string())];
        let out = runtime.synthesize_office(lead_id, "Launch a blog", &results).await;
        assert!(out.contains("integrated final answer"), "got: {out}");
    }

    #[tokio::test]
    async fn plan_office_can_return_a_clarifying_question() {
        let db = temp_db().await;
        let runtime = Runtime::new(db.clone());
        let (_, lead_id, _) = seed_office(&db).await;
        runtime
            .set_provider_override(Some(Arc::new(OfficeProvider {
                calls: AtomicUsize::new(0),
                plan_json: r#"{"question":"Which audience should the launch target?"}"#.to_string(),
            }) as Arc<dyn ModelProviderTrait>))
            .await;

        let members = vec![crate::orchestrator::OfficeMember {
            bot_id: Uuid::new_v4(),
            name: "Chief of Staff".into(),
            rank: "lead".into(),
            specialty: "growth".into(),
        }];
        let plan = runtime
            .plan_office(lead_id, None, None, &members, "Launch a blog")
            .await
            .expect("clarification plan");
        assert!(plan.is_clarification());
        assert!(plan.tasks.is_empty());
        assert_eq!(
            plan.question.as_deref(),
            Some("Which audience should the launch target?")
        );
    }

    #[tokio::test]
    async fn plan_office_falls_back_to_none_on_garbage() {
        let db = temp_db().await;
        let runtime = Runtime::new(db.clone());
        let (_, lead_id, _) = seed_office(&db).await;
        runtime
            .set_provider_override(Some(Arc::new(OfficeProvider {
                calls: AtomicUsize::new(0),
                plan_json: "I cannot plan that.".to_string(),
            }) as Arc<dyn ModelProviderTrait>))
            .await;

        let members = vec![crate::orchestrator::OfficeMember {
            bot_id: Uuid::new_v4(),
            name: "Chief of Staff".into(),
            rank: "lead".into(),
            specialty: "growth".into(),
        }];
        assert!(runtime.plan_office(lead_id, None, None, &members, "hi").await.is_none());
    }
}

#[cfg(test)]
mod emulation_tests {
    use super::*;

    fn tool(name: &str) -> ToolDefinition {
        ToolDefinition {
            name: name.to_string(),
            description: format!("{name} does things"),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {"query": {"type": "string"}},
            }),
        }
    }

    #[test]
    fn protocol_block_is_parsed_and_stripped() {
        let text = "Let me search.\n```json\n{\"tool_calls\": [{\"name\": \"web_search\", \"arguments\": {\"query\": \"omarchy\"}}]}\n```";
        let (calls, stripped) = parse_emulated_tool_calls(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "web_search");
        assert_eq!(calls[0].arguments["query"], "omarchy");
        assert!(calls[0].id.starts_with("emu-"));
        assert_eq!(stripped, "Let me search.");
    }

    #[test]
    fn ordinary_code_fences_survive_verbatim() {
        let text = "Here is code:\n```rust\nfn main() {}\n```\nDone.";
        let (calls, stripped) = parse_emulated_tool_calls(text);
        assert!(calls.is_empty());
        assert_eq!(stripped, text);
    }

    #[test]
    fn json_fence_without_tool_calls_is_not_a_protocol_block() {
        let text = "```json\n{\"answer\": 42}\n```";
        let (calls, stripped) = parse_emulated_tool_calls(text);
        assert!(calls.is_empty());
        assert_eq!(stripped, text);
    }

    #[test]
    fn malformed_and_unclosed_blocks_are_preserved() {
        let (calls, stripped) = parse_emulated_tool_calls("```json\n{broken\n```");
        assert!(calls.is_empty());
        assert_eq!(stripped, "```json\n{broken\n```");
        let (calls, stripped) = parse_emulated_tool_calls("prefix ```json\n{\"tool_calls\": []");
        assert!(calls.is_empty());
        assert!(stripped.starts_with("prefix ```json"));
    }

    #[test]
    fn multiple_protocol_blocks_and_args_alias() {
        let text = "```{\"tool_calls\":[{\"name\":\"a\",\"args\":{\"x\":1}}]}\n```\nmid\n```json\n{\"tool_calls\":[{\"name\":\"b\"}]}\n```";
        let (calls, stripped) = parse_emulated_tool_calls(text);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].arguments["x"], 1);
        assert_eq!(calls[1].arguments, serde_json::json!({}));
        assert_eq!(stripped, "mid");
    }

    #[test]
    fn call_signature_detects_repeats() {
        let mk = || vec![ravenbot_models::ToolCall {
            name: "web_search".into(),
            arguments: serde_json::json!({"query": "x"}),
            id: "ignored".into(),
        }];
        assert_eq!(emulated_call_signature(&mk()), emulated_call_signature(&mk()));
        let other = vec![ravenbot_models::ToolCall {
            name: "web_search".into(),
            arguments: serde_json::json!({"query": "y"}),
            id: "ignored".into(),
        }];
        assert_ne!(emulated_call_signature(&mk()), emulated_call_signature(&other));
    }

    #[test]
    fn emulation_prompt_lists_schemas_and_protocol() {
        let prompt = tool_emulation_prompt(&[tool("web_search"), tool("file_read")]);
        assert!(prompt.contains("## Tool Use (text protocol)"));
        assert!(prompt.contains("- web_search: web_search does things"));
        assert!(prompt.contains("\"query\""));
        assert!(prompt.contains("\"tool_calls\""));
    }
}
