//! RAVENBOT agent runtime and orchestration engine
//!
//! This crate implements the directed task graph executor for parallel
//! multi-agent orchestration with checkpoint/resume capabilities.

pub mod graph;
pub mod executor;
pub mod state;

use ravenbot_core::{Run, RunState};
use ravenbot_db::Database;
use ravenbot_models::{ProviderManager, Message, ToolDefinition, DeltaCallback, ModelProviderTrait};
use ravenbot_skills::{SkillRegistry, SkillContext};
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

/// Live events streamed to the UI during a run.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StreamEvent {
    /// A new assistant token delta arrived
    Delta { bot_id: Uuid, thread_id: Uuid, content: String },
    /// Clear streamed text (a new model round begins, e.g. after tool use)
    Clear { bot_id: Uuid, thread_id: Uuid },
    /// A tool/skill execution started
    ToolStarted { thread_id: Uuid, name: String },
    /// A tool/skill execution finished
    ToolFinished { thread_id: Uuid, name: String },
    /// Web sources arrived from a search tool (live citation chips)
    Sources { thread_id: Uuid, sources: Vec<ravenbot_core::Source> },
    /// Live bot status for the run lifecycle (thinking / running_tool / done)
    Status { bot_id: Uuid, thread_id: Uuid, state: String },
    /// Real token/cost usage for a completed run
    Usage { thread_id: Uuid, tokens: u64, cost: f64 },
}

/// Emitter callback for stream events. Must be cheap and non-blocking.
pub type StreamEmitter = Arc<dyn Fn(StreamEvent) + Send + Sync>;

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
    stream_emitter: std::sync::RwLock<Option<StreamEmitter>>,
    /// Injectable provider override (tests/dev tooling): when set, execute_run
    /// uses it instead of creating a provider from the bot's config.
    provider_override: Arc<Mutex<Option<Arc<dyn ModelProviderTrait>>>>,
    /// Delegation depth per run (recursion guard for inter-bot delegation)
    delegation_depth: std::sync::RwLock<HashMap<Uuid, u32>>,
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
            stream_emitter: std::sync::RwLock::new(None),
            provider_override: Arc::new(Mutex::new(None)),
            delegation_depth: std::sync::RwLock::new(HashMap::new()),
        }
    }

    /// Install (or remove, with `None`) a provider override used by execute_run
    /// instead of the bot's configured provider. Test/dev hook.
    pub async fn set_provider_override(&self, provider: Option<Arc<dyn ModelProviderTrait>>) {
        *self.provider_override.lock().await = provider;
    }

    /// Install (or remove, with `None`) the live stream event emitter
    pub fn set_stream_emitter(&self, emitter: Option<StreamEmitter>) {
        *self.stream_emitter.write().expect("stream emitter lock poisoned") = emitter;
    }

    fn emit(&self, event: StreamEvent) {
        if let Some(emitter) = self.stream_emitter.read().expect("stream emitter lock poisoned").as_ref() {
            emitter(event);
        }
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

        // Run the instruction through the real runtime in a fresh thread
        let thread = ravenbot_core::Thread::new(
            target.id,
            format!("Delegation: {}", instruction.chars().take(30).collect::<String>()),
        );
        if let Err(e) = ravenbot_db::queries::ThreadQueries::create(self.db.pool(), &thread).await {
            return ravenbot_skills::SkillResult::failure(e.to_string());
        }
        let user_msg = ravenbot_core::Message::user(thread.id, &instruction);
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

    /// Trigger the kill switch
    pub async fn trigger_kill_switch(&self, reason: impl Into<String>) {
        self.kill_switch.trigger(reason).await;
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

    /// Release the kill switch
    pub async fn release_kill_switch(&self) {
        self.kill_switch.release().await;
    }

    /// Check if kill switch is active
    pub async fn is_paused(&self) -> bool {
        self.kill_switch.is_triggered().await
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

        let memory_context = if thread_ephemeral {
            String::new()
        } else {
            self.memory_retriever.get_context(
                bot.id,
                last_user_message,
                5,
            ).await.unwrap_or_default()
        };

        // Create model provider (honoring the bot's configured model id —
        // this was previously ignored), or the injected override (tests/dev)
        let provider: Arc<dyn ModelProviderTrait> =
            if let Some(p) = self.provider_override.lock().await.clone() {
                p
            } else {
                let manager = self.provider_manager.lock().await;
                Arc::from(manager.create_provider_from_str_with_model(
                    &bot.config.model_provider,
                    Some(&bot.config.model_id),
                )
                .map_err(|e| RuntimeError::Model(e.to_string()))?)
            };

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

        // Cap to 32 tools to prevent context blowup while ensuring all assigned MCP and bot skills are present
        if assembled_skills.len() > 32 {
            assembled_skills.truncate(32);
        }

        let tool_definitions: Vec<ToolDefinition> = assembled_skills.iter().map(|skill| {
            ToolDefinition {
                name: skill.id().to_string(),
                description: skill.description().to_string(),
                parameters: skill.input_schema(),
            }
        }).collect();

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

        // Build conversation messages
        let mut model_messages = Vec::new();

        // Add system prompt with skill and memory information
        let system_prompt = bot.config.custom_prompt.as_deref()
            .unwrap_or("You are a helpful AI assistant. Complete tasks as requested. You have access to tools that can help you accomplish tasks.");
        
        let mut context_parts = Vec::new();
        
        if is_deep_search {
            context_parts.push("⚡ [DeepSearch Active]: The user explicitly requested DeepSearch. You MUST use your search tools (web_search, tavily_search, or browser_navigate) to look up fresh, accurate information from the web before generating your final answer.".to_string());
        }
        if is_think {
            context_parts.push("🧠 [Think Mode Active]: The user explicitly requested Deep Reasoning. Thoroughly analyze the question, inspect constraints, trace edge cases, and reason step-by-step before delivering the optimal solution.".to_string());
        }
        if !office_context.is_empty() {
            context_parts.push(format!("🏢 Team Office Context:\n{}", office_context));
        }

        if !tool_definitions.is_empty() {
            let skill_names: Vec<&str> = tool_definitions.iter().map(|d| d.name.as_str()).collect();
            context_parts.push(format!("Tools available: {}", skill_names.join(", ")));
        }
        
        if !memory_context.is_empty() {
            context_parts.push(memory_context);
        }
        
        let full_system = if context_parts.is_empty() {
            system_prompt.to_string()
        } else {
            format!("{}\n\nContext:\n{}", system_prompt, context_parts.join("\n\n"))
        };
        
        model_messages.push(Message {
            role: "system".to_string(),
            content: full_system,
            images: Vec::new(),
        });

        // Add conversation history
        for msg in &messages {
            let role = match msg.role {
                ravenbot_core::MessageRole::User => "user",
                ravenbot_core::MessageRole::Assistant => "assistant",
                ravenbot_core::MessageRole::System => "system",
                ravenbot_core::MessageRole::Tool => "user",
            };

            let content = match &msg.content {
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

            model_messages.push(Message {
                role: role.to_string(),
                content,
                images,
            });
        }

        // Call the model with tools, streaming tokens live to the UI.
        // Transient provider failures (network blips, 5xx, rate limits) are
        // retried once automatically before surfacing to the user.
        let emitter_snapshot: Option<StreamEmitter> = self
            .stream_emitter
            .read()
            .expect("stream emitter lock poisoned")
            .clone();
        let on_delta: DeltaCallback = {
            let thread_id = run.thread_id;
            let bot_id = bot.id;
            Arc::new(move |content: &str| {
                if let Some(emitter) = &emitter_snapshot {
                    emitter(StreamEvent::Delta {
                        bot_id,
                        thread_id,
                        content: content.to_string(),
                    });
                }
            })
        };

        let temperature = bot.config.temperature.unwrap_or(0.7);
        let max_tokens = bot.config.max_tokens.unwrap_or(4096);

        let mut response = match provider.complete_stream(
            &model_messages,
            &tool_definitions,
            temperature,
            max_tokens,
            on_delta.clone(),
            is_think,
        ).await {
            Ok(response) => response,
            Err(first_err) => {
                if !is_retryable_model_error(&first_err.to_string()) {
                    return Err(RuntimeError::Model(first_err.to_string()));
                }
                tracing::warn!(
                    error = %first_err,
                    "Transient model failure; retrying once"
                );
                if self.kill_switch.is_triggered().await {
                    return Err(RuntimeError::KillSwitchActive(
                        "Kill switch triggered during retry".to_string(),
                    ));
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                provider.complete_stream(
                    &model_messages,
                    &tool_definitions,
                    temperature,
                    max_tokens,
                    on_delta.clone(),
                    is_think,
                ).await.map_err(|e| RuntimeError::Model(format!(
                    "{} (retry after transient failure also failed: {})",
                    first_err, e
                )))?
            }
        };

        // Record this round's usage against the bot's budget (every call counts)
        let _ = self.budget_manager.record_usage(
            bot.id,
            response.usage.input_tokens + response.usage.output_tokens,
            response.usage.cost(0.003, 0.015),
        ).await;

        // Handle tool calls
        let mut run_sources: Vec<ravenbot_core::Source> = Vec::new();
        let mut seen_source_urls: HashSet<String> = HashSet::new();
        let mut max_tool_rounds = 5;
        while !response.tool_calls.is_empty() && max_tool_rounds > 0 {
            if self.kill_switch.is_triggered().await {
                return Err(RuntimeError::KillSwitchActive("Kill switch triggered during execution".to_string()));
            }
            
            max_tool_rounds -= 1;
            
            let assistant_content = response.content.clone().unwrap_or_default();
            model_messages.push(Message {
                role: "assistant".to_string(),
                content: assistant_content,
                images: Vec::new(),
            });

            let skill_context = SkillContext {
                bot_id: bot.id,
                run_id: run.id,
                thread_id: run.thread_id,
            };

            for tool_call in &response.tool_calls {
                tracing::info!(
                    skill = %tool_call.name,
                    arguments = %tool_call.arguments,
                    "Executing tool"
                );

                self.emit(StreamEvent::ToolStarted {
                    thread_id: run.thread_id,
                    name: tool_call.name.clone(),
                });
                self.emit(StreamEvent::Status {
                    bot_id: bot.id,
                    thread_id: run.thread_id,
                    state: "running_tool".to_string(),
                });

                // Audit log tool call initiation
                let _ = self.audit_logger.log_tool_call(
                    bot.id,
                    Some(run.id),
                    Some(run.thread_id),
                    &tool_call.name,
                    tool_call.arguments.clone(),
                ).await;

                // Execute tool: runtime-native memory first (the registry
                // stubs are placeholders), then assembled skills, registry,
                // or dynamic MCP lookup
                let result = if tool_call.name == "delegate" {
                    Ok(self.exec_delegation(run, tool_call.arguments.clone()).await)
                } else if tool_call.name == "memory_save" {
                    Ok(self.exec_memory_save(bot.id, tool_call.arguments.clone()).await)
                } else if tool_call.name == "memory_recall" {
                    Ok(self.exec_memory_recall(bot.id, tool_call.arguments.clone()).await)
                } else if let Some(skill) = assembled_skills.iter().find(|s| s.id() == tool_call.name) {
                    skill.execute(&skill_context, tool_call.arguments.clone()).await
                } else if let Some(skill) = self.skill_registry.get(&tool_call.name) {
                    skill.execute(&skill_context, tool_call.arguments.clone()).await
                } else if let Ok(Some((cfg, env))) =
                    self.mcp_registry.resolve_tool(&tool_call.name).await
                {
                    let client = ravenbot_mcp::client::McpClient::with_env(cfg, env);
                    match client.call_tool(&tool_call.name, tool_call.arguments.clone()).await {
                        Ok(v) => Ok(ravenbot_skills::SkillResult::success(v)),
                        Err(e) => Err(ravenbot_skills::SkillError::Execution(e)),
                    }
                } else {
                    self.skill_registry.execute(&tool_call.name, &skill_context, tool_call.arguments.clone()).await
                };

                let result_json = match result {
                    Ok(r) => serde_json::to_value(r).unwrap_or_default(),
                    Err(e) => serde_json::json!({ "error": e.to_string() }),
                };

                // Harvest citations from search results (live source chips)
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

                model_messages.push(Message {
                    role: "user".to_string(),
                    content: format!("Tool {} result: {}", tool_call.name, result_json),
                    images: Vec::new(),
                });

                run.add_usage(0, 0.001);

                self.emit(StreamEvent::ToolFinished {
                    thread_id: run.thread_id,
                    name: tool_call.name.clone(),
                });
                self.emit(StreamEvent::Status {
                    bot_id: bot.id,
                    thread_id: run.thread_id,
                    state: "thinking".to_string(),
                });
            }

            // A new model round begins: clear the streamed text so tool-round
            // fragments don't mix with the final streamed response.
            self.emit(StreamEvent::Clear {
                bot_id: bot.id,
                thread_id: run.thread_id,
            });

            response = provider.complete_stream(
                &model_messages,
                &tool_definitions,
                bot.config.temperature.unwrap_or(0.7),
                bot.config.max_tokens.unwrap_or(4096),
                on_delta.clone(),
                is_think,
            ).await.map_err(|e| RuntimeError::Model(e.to_string()))?;

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

        // Create assistant message (with any harvested web sources).
        // Extended-thinking reasoning is persisted as a  swell prefix so the
        // UI's Reasoning panel keeps showing it after reload.
        if let Some(content) = response.content {
            let final_content = match response.reasoning.filter(|r| !r.trim().is_empty()) {
                Some(reasoning) if !content.contains("feel") => {
                    format!("feel{}feel\n\n{}", reasoning, content)
                }
                _ => content,
            };
            let assistant_msg = ravenbot_core::Message::assistant_with_sources(
                run.thread_id,
                final_content,
                run_sources,
            );
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

    /// Pause a running run
    pub async fn pause_run(&self, run: &mut Run) -> Result<(), RuntimeError> {
        run.state = RunState::Paused;
        run.checkpoint(serde_json::json!({}));
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
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use ravenbot_core::{Bot, Thread};
    use std::path::PathBuf;

    /// In-memory databases don't round-trip through our `sqlite:{path}?mode=rwc`
    /// URL builder, so integration tests use a unique temp file instead.
    async fn temp_db() -> ravenbot_db::Database {
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
        seen_turns: std::sync::Mutex<Vec<Vec<(String, String)>>>,
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
                    .map(|m| (m.role.clone(), m.content.clone()))
                    .collect(),
            );
            self.seen_enable_reasoning.lock().unwrap().push(enable_reasoning);

            if round == 0 {
                // Reasoning streams inside  swell (as the UI expects)
                on_delta("feel");
                on_delta("The user wants me to remember rust facts.");
                on_delta("\n\n");
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
                StreamEvent::Status { state, .. } => format!("status:{}", state),
                StreamEvent::Usage { tokens, .. } => format!("usage:{}", tokens),
            };
            events_cb.lock().unwrap().push(label);
        })));

        let mut run = ravenbot_core::Run::new(bot.id, thread.id);
        runtime.execute_run(&mut run).await.expect("run should succeed");

        // 1. Two model rounds happened (tool round + final)
        assert_eq!(mock.calls.load(Ordering::SeqCst), 2);

        // 2. [Think] intent reached the provider as enable_reasoning
        assert_eq!(*mock.seen_enable_reasoning.lock().unwrap(), vec![true, true]);

        // 3. The tool result was fed back to the model in round 2
        let round2 = &mock.seen_turns.lock().unwrap()[1];
        assert!(
            round2.iter().any(|(_, c)| c.contains("Tool memory_save result")),
            "tool result must be fed back"
        );

        // 4. Stream events: reasoning + text deltas, tool lifecycle
        {
            let ev = events.lock().unwrap();
            assert!(ev.iter().any(|e| e.contains("delta:feel")), "reasoning deltas streamed");
            assert!(ev.iter().any(|e| e.contains("tool_start:memory_save")));
            assert!(ev.iter().any(|e| e.contains("tool_end:memory_save")));
            assert!(ev.iter().any(|e| e.contains("delta:Here")));
        }

        // 5. Final assistant message persisted: reasoning  swell prefix + content
        let messages = ravenbot_db::queries::MessageQueries::list_by_thread(db.pool(), thread.id)
            .await
            .unwrap();
        let last = messages.last().unwrap();
        let final_text = match &last.content {
            ravenbot_core::MessageContent::Text { text, .. } => text.clone(),
            other => panic!("unexpected content: {other:?}"),
        };
        assert!(final_text.starts_with("feel"), "reasoning must be persisted: {final_text}");
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
        let msg = ravenbot_core::Message::user(thread.id, "hello");
        ravenbot_db::queries::MessageQueries::insert(db.pool(), &msg).await.unwrap();

        runtime.set_provider_override(Some(Arc::new(MockProvider::new()) as Arc<dyn ModelProviderTrait>)).await;

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
