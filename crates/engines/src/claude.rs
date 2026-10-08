//! Claude Code engine.
//!
//! Drives the locally-installed `claude` CLI in non-interactive print mode,
//! streaming its native `stream-json` protocol and normalizing it into
//! `EngineEvent`s. The session id is captured from the `init` frame and can be
//! passed back as `--resume` to continue the same conversation across turns.
//!
//! Protocol reference (observed on Claude Code 2.1.x):
//!   {"type":"system","subtype":"init","session_id":…,"model":…}
//!   {"type":"stream_event","event":{"type":"content_block_delta","delta":{…
//!        {"type":"text_delta","text":…} | {"type":"thinking_delta","thinking":…}}}}
//!   {"type":"assistant","message":{"content":[…],"usage":{…}}}
//!   {"type":"user","message":{"content":[{"type":"tool_result",…}]}}
//!   {"type":"result","is_error":…,"total_cost_usd":…,"usage":{…},"result":…}


use async_trait::async_trait;
use tokio::io::AsyncWriteExt;

use crate::process::{self, spawn_with_stdin, stream_lines};
use crate::{
    AgentEngine, CancelToken, EngineApproval, EngineCallback, EngineCapabilities, EngineError,
    EngineEvent, EngineOutcome, EngineRequest, EngineUsage,
};

pub struct ClaudeEngine {
    /// Binary to run. Defaults to `claude`; override with `RAVENBOT_CLAUDE_CMD`
    /// to point at a versioned build or wrapper.
    command: String,
}

impl ClaudeEngine {
    pub fn new() -> Self {
        Self::with_command(
            std::env::var("RAVENBOT_CLAUDE_CMD")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "claude".to_string()),
        )
    }

    /// Explicit binary path (wrappers, versioned builds, tests).
    pub fn with_command(command: impl Into<String>) -> Self {
        Self { command: command.into() }
    }

    fn permission_mode(approval: EngineApproval) -> &'static str {
        match approval {
            // Fail-closed: never prompt, deny anything that would need consent.
            EngineApproval::Ask => "dontAsk",
            EngineApproval::Auto => "acceptEdits",
            EngineApproval::Full => "bypassPermissions",
        }
    }

    fn build_args(&self, req: &EngineRequest, session_id: &str) -> Vec<String> {
        let mut args: Vec<String> = vec![
            "-p".into(),
            "--output-format".into(),
            "stream-json".into(),
            "--verbose".into(),
            "--include-partial-messages".into(),
            "--permission-mode".into(),
            Self::permission_mode(req.approval).into(),
        ];
        // Non-Full runs must never hang waiting for a terminal prompt.
        if req.approval != EngineApproval::Full {
            args.push("--permission-prompts".into());
            args.push("none".into());
        }
        if let Some(model) = &req.model {
            if !model.trim().is_empty() {
                args.push("--model".into());
                args.push(model.clone());
            }
        }
        if let Some(effort) = &req.effort {
            if !effort.trim().is_empty() {
                args.push("--effort".into());
                args.push(effort.clone());
            }
        }
        if let Some(system) = &req.system {
            if !system.trim().is_empty() {
                args.push("--append-system-prompt".into());
                args.push(system.clone());
            }
        }
        match &req.resume {
            Some(id) if !id.trim().is_empty() => {
                args.push("--resume".into());
                args.push(id.clone());
            }
            _ => {
                args.push("--session-id".into());
                args.push(session_id.to_string());
            }
        }
        args.extend(req.extra_args.iter().cloned());
        args
    }

    fn build_prompt(req: &EngineRequest) -> String {
        let mut prompt = req.prompt.clone();
        if !req.images.is_empty() {
            prompt.push_str("\n\nAttached images (read them with your file tools):\n");
            for img in &req.images {
                prompt.push_str(&format!("- {} ({})\n", img.path, img.mime));
            }
        }
        prompt
    }
}

impl Default for ClaudeEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AgentEngine for ClaudeEngine {
    fn id(&self) -> &str {
        "claude"
    }
    fn display_name(&self) -> &str {
        "Claude Code"
    }
    fn command(&self) -> &str {
        &self.command
    }
    fn install_hint(&self) -> &str {
        "Install Claude Code: npm install -g @anthropic-ai/claude-code"
    }
    fn sign_in_hint(&self) -> &str {
        "Run `claude` once in a terminal and complete /login."
    }
    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            resume: true,
            images: false,
            effort: true,
            interrupt: true,
            accepts_full_model_id: false,
        }
    }

    async fn run(
        &self,
        req: EngineRequest,
        on_event: EngineCallback,
        cancel: CancelToken,
    ) -> Result<EngineOutcome, EngineError> {
        if process::find_binary(self.command()).is_none() {
            return Err(EngineError::missing_cli(self.command(), self.install_hint()));
        }

        let session_id = uuid::Uuid::new_v4().to_string();
        let mut args = self.build_args(&req, &session_id);
        // Forward the bot's MCP servers through a per-run --mcp-config file;
        // dropped (file deleted) when the turn ends.
        let _mcp_config = if req.mcp_servers.is_empty() {
            None
        } else {
            match write_temp_mcp_config(&req.mcp_servers) {
                Ok(cfg) => {
                    args.push("--mcp-config".into());
                    args.push(cfg.path.to_string_lossy().to_string());
                    Some(cfg)
                }
                Err(e) => {
                    on_event(EngineEvent::Warning(format!(
                        "claude: could not write MCP config, continuing without servers: {e}"
                    )));
                    None
                }
            }
        };
        let prompt = Self::build_prompt(&req);

        let mut child = spawn_with_stdin(self.command(), &args, req.cwd.as_deref(), &req.env)?;

        // Prompt over stdin — never argv (keeps it off the process list).
        {
            let mut stdin = child
                .stdin
                .take()
                .ok_or_else(|| EngineError::spawn("claude child has no stdin"))?;
            stdin
                .write_all(prompt.as_bytes())
                .await
                .map_err(|e| EngineError::spawn(format!("failed to write prompt to claude: {}", e)))?;
            stdin
                .flush()
                .await
                .map_err(|e| EngineError::spawn(format!("failed to flush prompt: {}", e)))?;
            drop(stdin);
        }

        let mut outcome = EngineOutcome {
            session_id: Some(session_id),
            ..Default::default()
        };
        let mut saw_stream_delta = false;
        let mut final_text = String::new();
        let mut reasoning = String::new();
        let mut auth_failed = false;
        let mut denials: Vec<String> = Vec::new();

        let result = stream_lines(
            child,
            |line| {
                let Ok(frame) = serde_json::from_str::<serde_json::Value>(line) else {
                    return;
                };
                let frame_type = frame.get("type").and_then(|v| v.as_str()).unwrap_or("");
                match frame_type {
                    "system" => {
                        if frame.get("subtype").and_then(|v| v.as_str()) == Some("init") {
                            if let Some(sid) = frame.get("session_id").and_then(|v| v.as_str()) {
                                outcome.session_id = Some(sid.to_string());
                            }
                            on_event(EngineEvent::SessionStarted {
                                session_id: frame
                                    .get("session_id")
                                    .and_then(|v| v.as_str())
                                    .map(str::to_string),
                                model: frame.get("model").and_then(|v| v.as_str()).map(str::to_string),
                            });
                        }
                    }
                    "stream_event" => {
                        // Drop subagent narration (parallel Tasks interleave).
                        if frame.get("parent_tool_use_id").map(|v| !v.is_null()).unwrap_or(false) {
                            return;
                        }
                        let event = frame.get("event").cloned().unwrap_or_default();
                        if event.get("type").and_then(|v| v.as_str()) != Some("content_block_delta") {
                            return;
                        }
                        let delta = event.get("delta").cloned().unwrap_or_default();
                        match delta.get("type").and_then(|v| v.as_str()) {
                            Some("text_delta") => {
                                if let Some(text) = delta.get("text").and_then(|v| v.as_str()) {
                                    if !text.is_empty() {
                                        saw_stream_delta = true;
                                        final_text.push_str(text);
                                        on_event(EngineEvent::TextDelta(text.to_string()));
                                    }
                                }
                            }
                            Some("thinking_delta") => {
                                if let Some(think) = delta.get("thinking").and_then(|v| v.as_str()) {
                                    if !think.is_empty() {
                                        reasoning.push_str(think);
                                        on_event(EngineEvent::ReasoningDelta(think.to_string()));
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    "assistant" => {
                        let message = frame.get("message").cloned().unwrap_or_default();
                        let content = message
                            .get("content")
                            .and_then(|v| v.as_array())
                            .cloned()
                            .unwrap_or_default();

                        let whole_text: String = content
                            .iter()
                            .filter(|b| b.get("type").and_then(|v| v.as_str()) == Some("text"))
                            .filter_map(|b| b.get("text").and_then(|v| v.as_str()))
                            .collect::<Vec<_>>()
                            .join("");

                        if frame.get("is_api_error_message").and_then(|v| v.as_bool()) == Some(true)
                            || is_auth_error(&whole_text)
                        {
                            auth_failed = true;
                            on_event(EngineEvent::Warning(whole_text.clone()));
                        } else if !whole_text.trim().is_empty() {
                            // Fallback for CLIs/paths that never streamed the block.
                            if !saw_stream_delta {
                                final_text.push_str(&whole_text);
                                on_event(EngineEvent::TextDelta(whole_text.clone()));
                            }
                            saw_stream_delta = false;
                            on_event(EngineEvent::AssistantText(whole_text));
                        }

                        for block in &content {
                            if block.get("type").and_then(|v| v.as_str()) == Some("tool_use") {
                                let id = block.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                let name = block.get("name").and_then(|v| v.as_str()).unwrap_or("tool").to_string();
                                let summary = command_summary(block.get("input"));
                                on_event(EngineEvent::Status("running_tool".to_string()));
                                on_event(EngineEvent::ToolStarted { id, name, summary });
                            }
                        }

                        if let Some(usage) = message.get("usage") {
                            let input = usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0)
                                + usage.get("cache_read_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                            let output = usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                            if input > 0 || output > 0 {
                                on_event(EngineEvent::Usage { input, output, cost: None });
                            }
                        }
                    }
                    "user" => {
                        let content = frame
                            .pointer("/message/content")
                            .and_then(|v| v.as_array())
                            .cloned()
                            .unwrap_or_default();
                        for block in &content {
                            if block.get("type").and_then(|v| v.as_str()) == Some("tool_result") {
                                let id = block
                                    .get("tool_use_id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                let ok = block.get("is_error").and_then(|v| v.as_bool()) != Some(true);
                                on_event(EngineEvent::Status("thinking".to_string()));
                                on_event(EngineEvent::ToolFinished { id, ok });
                            }
                        }
                    }
                    "result" => {
                        outcome.ok = frame.get("is_error").and_then(|v| v.as_bool()) != Some(true);
                        outcome.stop_reason = frame
                            .get("stop_reason")
                            .and_then(|v| v.as_str())
                            .map(str::to_string);
                        outcome.cost = frame.get("total_cost_usd").and_then(|v| v.as_f64());
                        if let Some(text) = frame.get("result").and_then(|v| v.as_str()) {
                            if final_text.trim().is_empty() {
                                final_text.push_str(text);
                            }
                        }
                        if let Some(usage) = frame.get("usage") {
                            let input = usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0)
                                + usage.get("cache_read_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0)
                                + usage.get("cache_creation_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                            let output = usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                            let cached = usage.get("cache_read_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                            outcome.usage = Some(EngineUsage { input, output, cached_input: cached });
                            on_event(EngineEvent::Usage { input, output, cost: outcome.cost });
                        }
                        if let Some(items) = frame.get("permission_denials").and_then(|v| v.as_array()) {
                            for item in items {
                                denials.push(
                                    item.get("tool_name")
                                        .and_then(|v| v.as_str())
                                        .or_else(|| item.as_str())
                                        .unwrap_or("tool")
                                        .to_string(),
                                );
                            }
                        }
                    }
                    _ => {}
                }
            },
            &cancel,
        )
        .await;

        match result {
            Ok(()) => {
                if auth_failed {
                    return Err(EngineError::auth(
                        "Claude Code is not signed in. Run `claude` once and complete /login.",
                    ));
                }
                outcome.final_text = final_text;
                outcome.reasoning = reasoning;
                outcome.denials = denials;
                Ok(outcome)
            }
            Err(e) if e.code == crate::EngineErrorCode::Cancelled => Err(e),
            Err(e) => {
                // A process that exited non-zero but did produce a result frame
                // still counts as a completed turn.
                if !final_text.trim().is_empty() {
                    outcome.final_text = final_text;
                    outcome.reasoning = reasoning;
                    outcome.denials = denials;
                    outcome.ok = true;
                    Ok(outcome)
                } else if auth_failed {
                    Err(EngineError::auth(
                        "Claude Code is not signed in. Run `claude` once and complete /login.",
                    ))
                } else {
                    Err(e)
                }
            }
        }
    }
}

/// Claude Code `--mcp-config` payload: `{"mcpServers": {name: {...}}}` with
/// stdio entries `{type:"stdio", command, args, env}` and http entries
/// `{type:"http", url, headers}`. Broken entries are dropped, not fatal.
pub(crate) fn mcp_config_json(servers: &[crate::EngineMcpServer]) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for server in servers {
        let entry = if server.transport == "http" {
            match server.url.as_deref().filter(|u| !u.trim().is_empty()) {
                Some(url) => serde_json::json!({"type": "http", "url": url, "headers": server.headers}),
                None => continue,
            }
        } else {
            if server.command.trim().is_empty() {
                continue;
            }
            serde_json::json!({
                "type": "stdio",
                "command": server.command,
                "args": server.args,
                "env": server.env,
            })
        };
        map.insert(server.name.clone(), entry);
    }
    serde_json::json!({"mcpServers": map})
}

/// A per-run MCP config file; deleting it on drop keeps resolved headers and
/// env values off the disk past the turn.
struct TempMcpConfig {
    path: std::path::PathBuf,
}

impl Drop for TempMcpConfig {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn write_temp_mcp_config(servers: &[crate::EngineMcpServer]) -> Result<TempMcpConfig, String> {
    let value = mcp_config_json(servers);
    let path = std::env::temp_dir().join(format!(
        "ravenbot-claude-mcp-{}.json",
        uuid::Uuid::new_v4()
    ));
    let content = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    if let Ok(meta) = std::fs::metadata(&path) {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(meta.permissions().mode() & 0o600));
    }
    Ok(TempMcpConfig { path })
}

/// Best-effort one-line summary of a tool call for the activity chip.
fn command_summary(input: Option<&serde_json::Value>) -> Option<String> {
    let input = input?;
    for key in ["command", "file_path", "path", "pattern", "url", "query"] {
        if let Some(value) = input.get(key).and_then(|v| v.as_str()) {
            if !value.is_empty() {
                return Some(truncate(value, 200));
            }
        }
    }
    None
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}

fn is_auth_error(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("invalid api key")
        || lower.contains("authentication")
        || lower.contains("not logged in")
        || (lower.contains("api error") && lower.contains("login"))
        || lower.contains("please run /login")
        || lower.contains("401")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EngineApproval, EngineRequest};

    #[test]
    fn permission_modes_are_fail_closed() {
        assert_eq!(ClaudeEngine::permission_mode(EngineApproval::Ask), "dontAsk");
        assert_eq!(ClaudeEngine::permission_mode(EngineApproval::Auto), "acceptEdits");
        assert_eq!(ClaudeEngine::permission_mode(EngineApproval::Full), "bypassPermissions");
    }

    #[test]
    fn build_args_includes_expected_flags() {
        let engine = ClaudeEngine::new();
        let req = EngineRequest {
            model: Some("sonnet".into()),
            system: Some("be terse".into()),
            approval: EngineApproval::Auto,
            resume: Some("sess-123".into()),
            ..Default::default()
        };
        let args = engine.build_args(&req, "new-uuid");
        let joined = args.join(" ");
        assert!(joined.contains("--output-format stream-json"));
        assert!(joined.contains("--permission-mode acceptEdits"));
        assert!(joined.contains("--permission-prompts none"));
        assert!(joined.contains("--model sonnet"));
        assert!(joined.contains("--append-system-prompt be terse"));
        assert!(joined.contains("--resume sess-123"));
        assert!(!joined.contains("--session-id"));
    }

    #[test]
    fn build_args_uses_session_id_without_resume() {
        let engine = ClaudeEngine::new();
        let req = EngineRequest::default();
        let args = engine.build_args(&req, "abc-uuid");
        let joined = args.join(" ");
        assert!(joined.contains("--session-id abc-uuid"));
        assert!(!joined.contains("--resume"));
    }

    #[test]
    fn full_mode_does_not_force_permission_prompts_none() {
        let engine = ClaudeEngine::new();
        let req = EngineRequest { approval: EngineApproval::Full, ..Default::default() };
        let joined = engine.build_args(&req, "abc-uuid").join(" ");
        assert!(joined.contains("--permission-mode bypassPermissions"));
        assert!(!joined.contains("--permission-prompts"));
    }

    #[test]
    fn prompt_includes_image_references() {
        let req = EngineRequest {
            prompt: "look".into(),
            images: vec![crate::EngineImage { path: "/tmp/a.png".into(), mime: "image/png".into() }],
            ..Default::default()
        };
        let prompt = ClaudeEngine::build_prompt(&req);
        assert!(prompt.contains("/tmp/a.png"));
    }

    fn stdio_server() -> crate::EngineMcpServer {
        crate::EngineMcpServer {
            name: "git".into(),
            transport: "stdio".into(),
            command: "uvx".into(),
            args: vec!["mcp-server-git".into()],
            env: std::collections::HashMap::from([("GIT_REPO".to_string(), "/tmp".to_string())]),
            ..Default::default()
        }
    }

    fn http_server() -> crate::EngineMcpServer {
        crate::EngineMcpServer {
            name: "remote".into(),
            transport: "http".into(),
            url: Some("https://mcp.example.com/api".into()),
            headers: std::collections::HashMap::from([(
                "Authorization".to_string(),
                "Bearer tok".to_string(),
            )]),
            ..Default::default()
        }
    }

    #[test]
    fn mcp_config_json_covers_both_transports() {
        let cfg = mcp_config_json(&[stdio_server(), http_server()]);
        assert_eq!(
            cfg["mcpServers"]["git"],
            serde_json::json!({"type": "stdio", "command": "uvx", "args": ["mcp-server-git"], "env": {"GIT_REPO": "/tmp"}})
        );
        assert_eq!(
            cfg["mcpServers"]["remote"],
            serde_json::json!({"type": "http", "url": "https://mcp.example.com/api", "headers": {"Authorization": "Bearer tok"}})
        );
        assert!(cfg["mcpServers"].get("bogus").is_none());
    }

    #[test]
    fn mcp_config_json_drops_broken_entries() {
        let no_url = crate::EngineMcpServer { name: "bad".into(), transport: "http".into(), ..Default::default() };
        let no_command = crate::EngineMcpServer { name: "empty".into(), transport: "stdio".into(), ..Default::default() };
        let cfg = mcp_config_json(&[no_url, no_command]);
        assert_eq!(cfg["mcpServers"].as_object().unwrap().len(), 0);
    }

    #[test]
    fn temp_mcp_config_is_written_and_cleaned_up() {
        let path = {
            let cfg = write_temp_mcp_config(&[stdio_server()]).expect("write temp config");
            assert!(cfg.path.exists());
            cfg.path.clone()
        };
        assert!(!path.exists(), "temp MCP config must be deleted on drop");
    }

    /// End-to-end driver test against a fake CLI that emits the real
    /// stream-json protocol — no network, no auth, fully deterministic.
    #[cfg(unix)]
    #[tokio::test]
    async fn parses_fake_stream_json_into_events_and_outcome() {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;

        let script_path = std::env::temp_dir().join(format!(
            "ravenbot-fake-claude-{}.sh",
            uuid::Uuid::new_v4()
        ));
        let frames = [
            r#"{"type":"system","subtype":"init","session_id":"sess-abc","model":"claude-x"}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Hello "}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"thinking_delta","thinking":"let me think"}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"world"}}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Hello world"}],"usage":{"input_tokens":5,"output_tokens":2}}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","is_error":false}]}}"#,
            r#"{"type":"result","is_error":false,"total_cost_usd":0.001,"usage":{"input_tokens":5,"output_tokens":2}}"#,
        ]
        .join("\n");
        let mut file = std::fs::File::create(&script_path).unwrap();
        writeln!(file, "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{}'", frames.replace('\'', "'\\''")).unwrap();
        drop(file);
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();

        // Use an explicit command rather than the process env so parallel
        // tests cannot race on RAVENBOT_CLAUDE_CMD.
        let engine = ClaudeEngine::with_command(script_path.to_string_lossy().to_string());
        assert_eq!(engine.command(), script_path.to_string_lossy());

        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let ev = events.clone();
        let cb: crate::EngineCallback = std::sync::Arc::new(move |e| ev.lock().unwrap().push(e));

        let req = EngineRequest { prompt: "hi".into(), ..Default::default() };
        let outcome = engine
            .run(req, cb, crate::CancelToken::new())
            .await
            .expect("fake claude run succeeds");

        let _ = std::fs::remove_file(&script_path);

        assert_eq!(outcome.session_id.as_deref(), Some("sess-abc"));
        assert!(outcome.ok);
        assert_eq!(outcome.final_text, "Hello world");
        assert_eq!(outcome.reasoning, "let me think");
        assert_eq!(outcome.cost, Some(0.001));
        assert_eq!(outcome.usage.unwrap().input, 5);

        let seen = events.lock().unwrap();
        assert!(seen.iter().any(|e| matches!(e, crate::EngineEvent::TextDelta(t) if t == "Hello ")));
        assert!(seen.iter().any(|e| matches!(e, crate::EngineEvent::ReasoningDelta(t) if t == "let me think")));
    }
}
