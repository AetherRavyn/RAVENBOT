//! Codex engine.
//!
//! Drives OpenAI's `codex` CLI. Codex's JSON output shapes have moved across
//! releases, so the parser is deliberately tolerant: it understands the
//! `item.*` / `turn.completed` families it emits today and falls back to
//! treating unknown JSON as text rather than failing the turn.

use async_trait::async_trait;
use tokio::io::AsyncWriteExt;

use crate::process::{self, spawn_with_stdin, stream_lines};
use crate::{
    AgentEngine, CancelToken, EngineApproval, EngineCallback, EngineCapabilities, EngineError,
    EngineEvent, EngineOutcome, EngineRequest, EngineUsage,
};

pub struct CodexEngine {
    /// Binary to run. Defaults to `codex`; override with `RAVENBOT_CODEX_CMD`.
    command: String,
}

impl CodexEngine {
    pub fn new() -> Self {
        Self::with_command(
            std::env::var("RAVENBOT_CODEX_CMD")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "codex".to_string()),
        )
    }

    /// Explicit binary path (wrappers, versioned builds, tests).
    pub fn with_command(command: impl Into<String>) -> Self {
        Self { command: command.into() }
    }

    fn build_args(&self, req: &EngineRequest) -> Vec<String> {
        let mut args: Vec<String> = vec!["exec".into(), "--json".into()];
        // Non-interactive runs should not fail on a missing repo.
        args.push("--skip-git-repo-check".into());
        match req.approval {
            EngineApproval::Full => {
                args.push("--dangerously-bypass-approvals-and-sandbox".into());
            }
            EngineApproval::Auto => {
                args.push("--full-auto".into());
            }
            EngineApproval::Ask => {
                args.push("--sandbox".into());
                args.push("read-only".into());
                args.push("--ask-for-approval".into());
                args.push("never".into());
            }
        }
        if let Some(model) = &req.model {
            if !model.trim().is_empty() {
                args.push("--model".into());
                args.push(model.clone());
            }
        }
        if let Some(cwd) = &req.cwd {
            if !cwd.trim().is_empty() {
                args.push("--cd".into());
                args.push(cwd.clone());
            }
        }
        for server in &req.mcp_servers {
            args.extend(mcp_overrides(server));
        }
        args.extend(req.extra_args.iter().cloned());
        args
    }

    fn build_prompt(req: &EngineRequest) -> String {
        let mut prompt = req.prompt.clone();
        if !req.images.is_empty() {
            prompt.push_str("\n\nAttached images:\n");
            for img in &req.images {
                prompt.push_str(&format!("- {}\n", img.path));
            }
        }
        prompt
    }
}

impl Default for CodexEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AgentEngine for CodexEngine {
    fn id(&self) -> &str {
        "codex"
    }
    fn display_name(&self) -> &str {
        "Codex"
    }
    fn command(&self) -> &str {
        &self.command
    }
    fn install_hint(&self) -> &str {
        "Install Codex: npm install -g @openai/codex  (then run `codex login`)"
    }
    fn sign_in_hint(&self) -> &str {
        "Run `codex login` in a terminal."
    }
    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            resume: false,
            images: false,
            effort: false,
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

        let args = self.build_args(&req);
        let prompt = Self::build_prompt(&req);
        let mut child = spawn_with_stdin(self.command(), &args, req.cwd.as_deref(), &req.env)?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(prompt.as_bytes())
                .await
                .map_err(|e| EngineError::spawn(format!("failed to write prompt to codex: {}", e)))?;
            let _ = stdin.flush().await;
            drop(stdin);
        }

        let mut outcome = EngineOutcome::default();
        let mut final_text = String::new();
        let mut reasoning = String::new();
        let mut tool_index = 0usize;
        let mut auth_failed = false;

        let result = stream_lines(
            child,
            |line| {
                let trimmed = line.trim();
                let Ok(frame) = serde_json::from_str::<serde_json::Value>(trimmed) else {
                    // Some builds print human text; surface it rather than drop it.
                    if !trimmed.is_empty() {
                        final_text.push_str(trimmed);
                        final_text.push('\n');
                        on_event(EngineEvent::TextDelta(format!("{}\n", trimmed)));
                    }
                    return;
                };
                handle_frame(
                    &frame,
                    &mut outcome,
                    &mut final_text,
                    &mut reasoning,
                    &mut tool_index,
                    &mut auth_failed,
                    &on_event,
                );
            },
            &cancel,
        )
        .await;

        if auth_failed {
            return Err(EngineError::auth(
                "Codex is not signed in. Run `codex login` in a terminal.",
            ));
        }

        match result {
            Ok(()) => {
                outcome.final_text = final_text;
                outcome.reasoning = reasoning;
                outcome.ok = true;
                Ok(outcome)
            }
            Err(e) => {
                if !final_text.trim().is_empty() {
                    outcome.final_text = final_text;
                    outcome.reasoning = reasoning;
                    outcome.ok = true;
                    Ok(outcome)
                } else {
                    Err(e)
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_frame(
    frame: &serde_json::Value,
    outcome: &mut EngineOutcome,
    final_text: &mut String,
    reasoning: &mut String,
    tool_index: &mut usize,
    auth_failed: &mut bool,
    on_event: &EngineCallback,
) {
    let frame_type = frame.get("type").and_then(|v| v.as_str()).unwrap_or("");

    if let Some(msg) = frame.get("message").and_then(|v| v.as_str()) {
        if is_auth_error(msg) {
            *auth_failed = true;
            on_event(EngineEvent::Warning(msg.to_string()));
        }
    }

    match frame_type {
        // Current Codex shape: {"type":"item.started"|"item.completed","item":{…}}
        "item.started" | "item.completed" | "item.updated" => {
            let item = frame.get("item").cloned().unwrap_or_default();
            let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let id = item
                .get("id")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| {
                    *tool_index += 1;
                    format!("item-{}", tool_index)
                });
            match item_type {
                "agent_message" | "assistant_message" => {
                    if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                        if frame_type == "item.completed" {
                            final_text.push_str(text);
                            on_event(EngineEvent::AssistantText(text.to_string()));
                        } else {
                            on_event(EngineEvent::TextDelta(text.to_string()));
                        }
                    }
                }
                "reasoning" | "reasoning_summary" => {
                    if let Some(text) = item
                        .get("text")
                        .or_else(|| item.get("summary"))
                        .and_then(|v| v.as_str())
                    {
                        reasoning.push_str(text);
                        on_event(EngineEvent::ReasoningDelta(text.to_string()));
                    }
                }
                "command_execution" | "command" | "file_change" | "mcp_tool_call"
                | "web_search" => {
                    let name = item
                        .get("command")
                        .and_then(|v| v.as_str())
                        .or_else(|| item.get("type").and_then(|v| v.as_str()))
                        .unwrap_or("tool")
                        .to_string();
                    if frame_type == "item.started" {
                        on_event(EngineEvent::Status("running_tool".to_string()));
                        on_event(EngineEvent::ToolStarted {
                            id: id.clone(),
                            name: truncate(&name, 80),
                            summary: Some(truncate(&name, 200)),
                        });
                    } else if frame_type == "item.completed" {
                        let status = item.get("status").and_then(|v| v.as_str()).unwrap_or("completed");
                        let ok = status != "failed" && status != "declined";
                        on_event(EngineEvent::Status("thinking".to_string()));
                        on_event(EngineEvent::ToolFinished { id, ok });
                    }
                }
                _ => {}
            }
        }
        "turn.completed" | "turn.failed" => {
            let ok = frame_type == "turn.completed";
            outcome.ok = ok;
            outcome.stop_reason = frame.get("stop_reason").and_then(|v| v.as_str()).map(str::to_string);
            if let Some(usage) = frame.get("usage") {
                let input = usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                let output = usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                outcome.usage = Some(EngineUsage { input, output, cached_input: 0 });
                on_event(EngineEvent::Usage { input, output, cost: None });
            }
            if !ok {
                if let Some(err) = frame.get("error").and_then(|v| v.as_str()) {
                    if is_auth_error(err) {
                        *auth_failed = true;
                    }
                    on_event(EngineEvent::Warning(err.to_string()));
                }
            }
        }
        _ => {
            if let Some(text) = frame.get("text").and_then(|v| v.as_str()) {
                final_text.push_str(text);
                on_event(EngineEvent::TextDelta(text.to_string()));
            }
        }
    }
}

/// Expose one MCP server through Codex's `-c mcp_servers.<name>.*` config
/// overrides. Values are emitted as JSON literals, which the TOML config
/// parser also accepts (basic strings, arrays, inline tables).
pub(crate) fn mcp_overrides(server: &crate::EngineMcpServer) -> Vec<String> {
    let key = if !server.name.is_empty()
        && server
            .name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        server.name.clone()
    } else {
        format!("\"{}\"", server.name.replace('"', "'"))
    };
    let mut out = Vec::new();
    if server.transport == "http" {
        let Some(url) = server.url.as_deref().filter(|u| !u.trim().is_empty()) else {
            return out;
        };
        out.push("-c".into());
        out.push(format!("mcp_servers.{key}.url={}", json_value(&url)));
        if !server.headers.is_empty() {
            out.push("-c".into());
            out.push(format!(
                "mcp_servers.{key}.http_headers={}",
                json_value(&server.headers)
            ));
        }
    } else {
        if server.command.trim().is_empty() {
            return out;
        }
        out.push("-c".into());
        out.push(format!("mcp_servers.{key}.command={}", json_value(&server.command)));
        if !server.args.is_empty() {
            out.push("-c".into());
            out.push(format!("mcp_servers.{key}.args={}", json_value(&server.args)));
        }
        if !server.env.is_empty() {
            out.push("-c".into());
            out.push(format!("mcp_servers.{key}.env={}", json_value(&server.env)));
        }
    }
    out
}

fn json_value<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max).collect::<String>() + "…"
}

fn is_auth_error(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("not logged in")
        || lower.contains("login")
        || lower.contains("unauthorized")
        || lower.contains("invalid api key")
        || lower.contains("401")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EngineApproval, EngineRequest};

    #[test]
    fn args_cover_approval_modes() {
        let engine = CodexEngine::new();
        let full = engine.build_args(&EngineRequest {
            approval: EngineApproval::Full,
            ..Default::default()
        });
        assert!(full.join(" ").contains("--dangerously-bypass-approvals-and-sandbox"));
        let auto = engine.build_args(&EngineRequest {
            approval: EngineApproval::Auto,
            ..Default::default()
        });
        assert!(auto.join(" ").contains("--full-auto"));
        let ask = engine.build_args(&EngineRequest {
            approval: EngineApproval::Ask,
            ..Default::default()
        });
        assert!(ask.join(" ").contains("--sandbox read-only"));
    }

    #[test]
    fn mcp_overrides_stdio_and_http() {
        let stdio = crate::EngineMcpServer {
            name: "git".into(),
            transport: "stdio".into(),
            command: "uvx".into(),
            args: vec!["mcp-server-git".into()],
            env: std::collections::HashMap::from([("GIT_REPO".to_string(), "/tmp".to_string())]),
            ..Default::default()
        };
        let args = mcp_overrides(&stdio);
        let joined = args.join(" ");
        assert!(joined.contains("-c mcp_servers.git.command=\"uvx\""), "{joined}");
        assert!(joined.contains("mcp_servers.git.args=[\"mcp-server-git\"]"), "{joined}");
        assert!(joined.contains("mcp_servers.git.env={\"GIT_REPO\":\"/tmp\"}"), "{joined}");

        let http = crate::EngineMcpServer {
            name: "remote".into(),
            transport: "http".into(),
            url: Some("https://mcp.example.com/api".into()),
            headers: std::collections::HashMap::from([(
                "Authorization".to_string(),
                "Bearer tok".to_string(),
            )]),
            ..Default::default()
        };
        let joined = mcp_overrides(&http).join(" ");
        assert!(joined.contains("mcp_servers.remote.url=\"https://mcp.example.com/api\""), "{joined}");
        assert!(joined.contains("mcp_servers.remote.http_headers={\"Authorization\":\"Bearer tok\"}"), "{joined}");
    }

    #[test]
    fn mcp_overrides_skip_broken_and_quote_odd_names() {
        let no_command = crate::EngineMcpServer {
            name: "empty".into(),
            transport: "stdio".into(),
            ..Default::default()
        };
        assert!(mcp_overrides(&no_command).is_empty());
        let no_url = crate::EngineMcpServer {
            name: "bad".into(),
            transport: "http".into(),
            ..Default::default()
        };
        assert!(mcp_overrides(&no_url).is_empty());
        let odd = crate::EngineMcpServer {
            name: "my server!".into(),
            transport: "stdio".into(),
            command: "run".into(),
            ..Default::default()
        };
        let joined = mcp_overrides(&odd).join(" ");
        assert!(joined.contains("mcp_servers.\"my server!\".command=\"run\""), "{joined}");
    }

    #[test]
    fn build_args_forwards_mcp_servers() {
        let engine = CodexEngine::new();
        let req = EngineRequest {
            mcp_servers: vec![crate::EngineMcpServer {
                name: "git".into(),
                transport: "stdio".into(),
                command: "uvx".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let joined = engine.build_args(&req).join(" ");
        assert!(joined.contains("-c mcp_servers.git.command=\"uvx\""), "{joined}");
    }

    #[test]
    fn parses_agent_message_items() {
        let mut outcome = EngineOutcome::default();
        let mut final_text = String::new();
        let mut reasoning = String::new();
        let mut tool_index = 0usize;
        let mut auth_failed = false;
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let ev = events.clone();
        let cb: EngineCallback = std::sync::Arc::new(move |e| ev.lock().unwrap().push(e));

        let frame = serde_json::json!({
            "type": "item.completed",
            "item": { "type": "agent_message", "id": "m1", "text": "hello" }
        });
        handle_frame(&frame, &mut outcome, &mut final_text, &mut reasoning, &mut tool_index, &mut auth_failed, &cb);
        assert_eq!(final_text, "hello");
        assert!(events.lock().unwrap().iter().any(|e| matches!(e, EngineEvent::AssistantText(t) if t == "hello")));
    }
}
