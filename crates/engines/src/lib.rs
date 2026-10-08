//! RAVENBOT agent engines.
//!
//! A parallel to `ravenbot-models`: instead of calling a model API directly,
//! an **engine** drives a locally-installed agent CLI (Claude Code, Codex, any
//! ACP-speaking agent) and normalizes its native protocol into one event
//! stream. This is what lets a RAVENBOT bot run on the same full agent
//! (persistent session, subagents, native tools, compaction) the user already
//! has installed — the OpenMausBot/Grok-Bot shape.
//!
//! The runtime decides per bot between the native tool loop and an engine.

pub mod acp;
pub mod claude;
pub mod cli;
pub mod codex;
pub mod process;

pub use acp::AcpEngine;
pub use claude::ClaudeEngine;
pub use codex::CodexEngine;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Notify;

/// How the engine should treat actions that need consent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineApproval {
    /// Fail-closed: anything that would prompt is denied non-interactively.
    Ask,
    /// Auto-approve edits; still deny dangerous prompts.
    Auto,
    /// Bypass all permission checks (explicit opt-in).
    Full,
}

impl EngineApproval {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "auto" => EngineApproval::Auto,
            "full" | "bypass" | "bypasspermissions" => EngineApproval::Full,
            _ => EngineApproval::Ask,
        }
    }
}

/// An attached image (already on disk). Engines that support native image
/// input read these paths; others receive them as prompt references.
#[derive(Debug, Clone)]
pub struct EngineImage {
    pub path: String,
    pub mime: String,
}

/// One MCP server RAVENBOT wants an external engine's agent to have. The
/// runtime resolves the bot's effective servers once and hands the same list
/// to every driver, which serializes it into its native dialect (ACP
/// `session/new`, Claude Code `--mcp-config`, Codex `-c` overrides).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EngineMcpServer {
    pub name: String,
    /// "stdio" or "http".
    pub transport: String,
    pub command: String,
    pub args: Vec<String>,
    /// Resolved env values for stdio servers (secrets already materialized —
    /// never echo these).
    pub env: HashMap<String, String>,
    pub url: Option<String>,
    /// Headers for http servers with `${VAR}` references already resolved.
    pub headers: HashMap<String, String>,
}

/// One engine invocation (a "turn").
#[derive(Debug, Clone)]
pub struct EngineRequest {
    pub prompt: String,
    pub system: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub cwd: Option<String>,
    /// Provider-native session id to continue (drivers that support resume).
    pub resume: Option<String>,
    pub approval: EngineApproval,
    pub images: Vec<EngineImage>,
    /// Extra environment variables layered over the inherited environment.
    pub env: HashMap<String, String>,
    /// Extra CLI arguments (advanced / forward-compat).
    pub extra_args: Vec<String>,
    /// Hard wall-clock budget for the turn.
    pub timeout_secs: u64,
    /// MCP servers to forward to the engine's agent (empty = none).
    pub mcp_servers: Vec<EngineMcpServer>,
}

impl Default for EngineRequest {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            system: None,
            model: None,
            effort: None,
            cwd: None,
            resume: None,
            approval: EngineApproval::Ask,
            images: Vec::new(),
            env: HashMap::new(),
            extra_args: Vec::new(),
            timeout_secs: 600,
            mcp_servers: Vec::new(),
        }
    }
}

/// Normalized events emitted while an engine runs.
#[derive(Debug, Clone)]
pub enum EngineEvent {
    SessionStarted { session_id: Option<String>, model: Option<String> },
    TextDelta(String),
    ReasoningDelta(String),
    ToolStarted { id: String, name: String, summary: Option<String> },
    ToolFinished { id: String, ok: bool },
    /// A whole assistant text block (used when the CLI did not stream it).
    AssistantText(String),
    Usage { input: u64, output: u64, cost: Option<f64> },
    /// A status change the UI can mirror (thinking / running_tool).
    Status(String),
    /// Non-fatal warning the user should see.
    Warning(String),
}

/// What became of a turn.
#[derive(Debug, Clone, Default)]
pub struct EngineOutcome {
    pub session_id: Option<String>,
    pub ok: bool,
    pub stop_reason: Option<String>,
    pub cost: Option<f64>,
    pub usage: Option<EngineUsage>,
    /// Final assistant text assembled by the driver.
    pub final_text: String,
    /// Final reasoning text, if any.
    pub reasoning: String,
    /// Human-readable list of actions the engine denied (fail-closed mode).
    pub denials: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct EngineUsage {
    pub input: u64,
    pub output: u64,
    pub cached_input: u64,
}

/// Errors from an engine. `setup` marks failures the user fixes by installing
/// or authenticating rather than retrying.
#[derive(Debug, Clone, thiserror::Error)]
#[error("{message}")]
pub struct EngineError {
    pub code: EngineErrorCode,
    pub message: String,
    pub setup: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineErrorCode {
    MissingCli,
    Auth,
    Timeout,
    Cancelled,
    Protocol,
    Upstream,
    Spawn,
}

impl EngineError {
    pub fn missing_cli(cmd: &str, hint: &str) -> Self {
        Self {
            code: EngineErrorCode::MissingCli,
            message: format!("`{}` is not installed or not on PATH. {}", cmd, hint),
            setup: true,
        }
    }
    pub fn auth(msg: impl Into<String>) -> Self {
        Self { code: EngineErrorCode::Auth, message: msg.into(), setup: true }
    }
    pub fn protocol(msg: impl Into<String>) -> Self {
        Self { code: EngineErrorCode::Protocol, message: msg.into(), setup: false }
    }
    pub fn upstream(msg: impl Into<String>) -> Self {
        Self { code: EngineErrorCode::Upstream, message: msg.into(), setup: false }
    }
    pub fn spawn(msg: impl Into<String>) -> Self {
        Self { code: EngineErrorCode::Spawn, message: msg.into(), setup: true }
    }
    pub fn timeout(secs: u64) -> Self {
        Self {
            code: EngineErrorCode::Timeout,
            message: format!("engine turn timed out after {}s", secs),
            setup: false,
        }
    }
    pub fn cancelled() -> Self {
        Self { code: EngineErrorCode::Cancelled, message: "engine turn cancelled".to_string(), setup: false }
    }
}

/// Callback invoked synchronously for each engine event. Must be cheap.
pub type EngineCallback = Arc<dyn Fn(EngineEvent) + Send + Sync>;

/// Cooperative cancellation handle for a running engine.
#[derive(Clone, Default)]
pub struct CancelToken {
    flag: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl CancelToken {
    pub fn new() -> Self {
        Self {
            flag: Arc::new(AtomicBool::new(false)),
            notify: Arc::new(Notify::new()),
        }
    }
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
    /// Resolves as soon as cancellation is requested.
    pub async fn cancelled(&self) {
        if self.is_cancelled() {
            return;
        }
        self.notify.notified().await;
    }
}

/// Static description of an engine for pickers and settings.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EngineInfo {
    pub id: String,
    pub display_name: String,
    pub command: String,
    pub available: bool,
    pub version: Option<String>,
    pub install_hint: Option<String>,
    pub sign_in_hint: Option<String>,
    /// Model ids the engine reports/accepts (empty = engine default).
    pub models: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct EngineCapabilities {
    /// Keeps a live session across turns (resume by session id).
    pub resume: bool,
    /// Accepts images as native input (vs prompt path references).
    pub images: bool,
    /// Supports an effort/reasoning level knob.
    pub effort: bool,
    /// The CLI can be interrupted mid-turn.
    pub interrupt: bool,
    /// The CLI accepts provider-qualified model ids (`provider/model`).
    pub accepts_full_model_id: bool,
}

#[async_trait::async_trait]
pub trait AgentEngine: Send + Sync {
    fn id(&self) -> &str;
    fn display_name(&self) -> &str;
    /// Binary name looked up on PATH.
    fn command(&self) -> &str;
    fn install_hint(&self) -> &str;
    fn sign_in_hint(&self) -> &str;
    fn capabilities(&self) -> EngineCapabilities;

    /// Probe the machine: is the CLI present, what version.
    async fn detect(&self) -> EngineInfo {
        let available = process::find_binary(self.command()).is_some();
        let version = if available {
            process::version_of(self.command(), &["--version"]).await
        } else {
            None
        };
        EngineInfo {
            id: self.id().to_string(),
            display_name: self.display_name().to_string(),
            command: self.command().to_string(),
            available,
            version,
            install_hint: Some(self.install_hint().to_string()),
            sign_in_hint: Some(self.sign_in_hint().to_string()),
            models: Vec::new(),
        }
    }

    /// Run one turn, streaming normalized events.
    async fn run(
        &self,
        req: EngineRequest,
        on_event: EngineCallback,
        cancel: CancelToken,
    ) -> Result<EngineOutcome, EngineError>;
}

/// The full engine catalog: built-in CLI engines plus any ACP agents the user
/// configured via `RAVENBOT_ACP_ENGINES`.
pub fn all_engines() -> Vec<Box<dyn AgentEngine>> {
    let mut engines: Vec<Box<dyn AgentEngine>> = vec![
        Box::new(ClaudeEngine::new()),
        Box::new(CodexEngine::new()),
    ];
    for spec in cli::all_specs() {
        engines.push(Box::new(cli::CliEngine::new(spec)));
    }
    for engine in acp::configured_engines() {
        engines.push(Box::new(engine));
    }
    engines
}

pub fn engine_by_id(id: &str) -> Option<Box<dyn AgentEngine>> {
    all_engines().into_iter().find(|e| e.id() == id)
}

/// Detect every known engine on this machine.
pub async fn list_engines() -> Vec<EngineInfo> {
    let mut out = Vec::new();
    for engine in all_engines() {
        out.push(engine.detect().await);
    }
    out
}

/// Is `id` the native (built-in) loop rather than an external engine?
pub fn is_native(id: &str) -> bool {
    id.trim().is_empty() || id.eq_ignore_ascii_case("native")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_is_default_and_recognized() {
        assert!(is_native(""));
        assert!(is_native("native"));
        assert!(is_native("Native"));
        assert!(!is_native("claude"));
    }

    #[test]
    fn approval_parsing_is_fail_closed() {
        assert_eq!(EngineApproval::parse("full"), EngineApproval::Full);
        assert_eq!(EngineApproval::parse("auto"), EngineApproval::Auto);
        assert_eq!(EngineApproval::parse("ask"), EngineApproval::Ask);
        assert_eq!(EngineApproval::parse("nonsense"), EngineApproval::Ask);
    }

    #[tokio::test]
    async fn cancel_token_latches() {
        let token = CancelToken::new();
        assert!(!token.is_cancelled());
        token.cancel();
        assert!(token.is_cancelled());
        // cancelled() returns immediately after cancellation
        tokio::time::timeout(std::time::Duration::from_millis(50), token.cancelled())
            .await
            .expect("cancelled future resolves");
    }

    /// Manual probe of this machine's installed engines.
    /// Run with: cargo test -p ravenbot-engines detect_real_engines -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "depends on CLIs installed on the host"]
    async fn detect_real_engines() {
        for info in list_engines().await {
            println!(
                "engine {}: available={} version={:?} command={}",
                info.id, info.available, info.version, info.command
            );
        }
    }

    #[test]
    fn catalog_ids_are_stable() {
        let engines = all_engines();
        let ids: Vec<String> = engines.iter().map(|e| e.id().to_string()).collect();
        assert!(ids.contains(&"claude".to_string()));
        assert!(ids.contains(&"codex".to_string()));
        assert!(ids.contains(&"opencode".to_string()));
        assert!(engine_by_id("claude").is_some());
        assert!(engine_by_id("nope").is_none());
    }
}
