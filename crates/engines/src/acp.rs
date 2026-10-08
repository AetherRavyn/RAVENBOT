//! Generic Agent Client Protocol (ACP) engine.
//!
//! ACP is the JSON-RPC-over-stdio standard that agent CLIs (Gemini CLI, Zed
//! agents, and others) speak. One driver therefore covers any ACP agent:
//! install the CLI, point RAVENBOT at it, and it becomes a bot engine.
//!
//! Configure instances with `RAVENBOT_ACP_ENGINES`, a JSON array:
//! ```json
//! [{"id":"gemini","display_name":"Gemini CLI","command":"gemini","args":["--acp"]}]
//! ```
//!
//! Protocol methods used: `initialize`, `session/new`, `session/prompt`;
//! notifications `session/update`; server request `session/request_permission`.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{ChildStdin, ChildStdout};
use tokio::sync::Mutex;

use crate::process::{self, spawn_with_stdin};
use crate::{
    AgentEngine, CancelToken, EngineApproval, EngineCallback, EngineCapabilities, EngineError,
    EngineEvent, EngineMcpServer, EngineOutcome, EngineRequest,
};

/// Serialize engine-neutral MCP servers into the ACP `session/new` form:
/// stdio → `{name, command, args, env[]}`, http → `{type:"http", name, url,
/// headers[]}` (env/header lists are `{name, value}` pairs, sorted for stable
/// payloads). Returns the payload plus the names of http servers that had to
/// be dropped because the agent lacks HTTP MCP capability.
pub(crate) fn mcp_servers_json(
    servers: &[EngineMcpServer],
    supports_http: bool,
) -> (Vec<Value>, Vec<String>) {
    fn pairs(map: &std::collections::HashMap<String, String>) -> Vec<Value> {
        let mut keys: Vec<&String> = map.keys().collect();
        keys.sort();
        keys.into_iter()
            .map(|k| json!({"name": k, "value": map[k]}))
            .collect()
    }
    let mut out = Vec::new();
    let mut skipped = Vec::new();
    for server in servers {
        if server.transport == "http" {
            let usable = supports_http
                && server
                    .url
                    .as_deref()
                    .map(|u| !u.trim().is_empty())
                    .unwrap_or(false);
            if !usable {
                skipped.push(server.name.clone());
                continue;
            }
            out.push(json!({
                "type": "http",
                "name": server.name,
                "url": server.url,
                "headers": pairs(&server.headers),
            }));
        } else {
            if server.command.trim().is_empty() {
                skipped.push(server.name.clone());
                continue;
            }
            out.push(json!({
                "name": server.name,
                "command": server.command,
                "args": server.args,
                "env": pairs(&server.env),
            }));
        }
    }
    (out, skipped)
}

/// Bundled stdio handles for an ACP session.
struct AcpIo {
    stdin: Arc<Mutex<ChildStdin>>,
    reader: Lines<BufReader<ChildStdout>>,
}

impl AcpIo {
    async fn write(&self, frame: &Value) -> Result<(), EngineError> {
        let mut guard = self.stdin.lock().await;
        guard
            .write_all(format!("{}\n", frame).as_bytes())
            .await
            .map_err(|e| EngineError::spawn(format!("ACP write failed: {}", e)))?;
        guard
            .flush()
            .await
            .map_err(|e| EngineError::spawn(format!("ACP flush failed: {}", e)))
    }

    /// Send a request and return its id.
    async fn send(&self, id: u64, method: &str, params: Value) -> Result<(), EngineError> {
        self.write(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
            .await
    }

    /// Read until the response with `target_id`, dispatching notifications and
    /// answering server→client requests along the way.
    async fn await_id(
        &mut self,
        target_id: u64,
        on_event: &EngineCallback,
        approval: EngineApproval,
        cancel: &CancelToken,
    ) -> Result<Value, EngineError> {
        loop {
            let line = tokio::select! {
                _ = cancel.cancelled() => return Err(EngineError::cancelled()),
                line = self.reader.next_line() => line
                    .map_err(|e| EngineError::protocol(format!("ACP read error: {}", e)))?
                    .ok_or_else(|| EngineError::protocol("ACP process closed before responding"))?,
            };
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let Ok(frame) = serde_json::from_str::<Value>(line) else {
                continue;
            };

            let has_method = frame.get("method").is_some();
            let id = frame.get("id");

            if !has_method {
                if id.and_then(|v| v.as_u64()) == Some(target_id) {
                    return Ok(frame);
                }
                continue;
            }

            if id.is_some() {
                // Server→client request (permission).
                handle_server_request(self, &frame, approval, on_event).await;
                continue;
            }

            // Notification.
            if frame.get("method").and_then(|v| v.as_str()) == Some("session/update") {
                let mut text = String::new();
                let mut reasoning = String::new();
                if let Some(update) = frame.pointer("/params/update") {
                    handle_update(update, &mut text, &mut reasoning, on_event);
                }
            }
        }
    }
}

pub struct AcpEngine {
    id: String,
    display_name: String,
    command: String,
    args: Vec<String>,
}

impl AcpEngine {
    pub fn new(
        id: impl Into<String>,
        display_name: impl Into<String>,
        command: impl Into<String>,
        args: Vec<String>,
    ) -> Self {
        Self { id: id.into(), display_name: display_name.into(), command: command.into(), args }
    }
}

#[async_trait]
impl AgentEngine for AcpEngine {
    fn id(&self) -> &str {
        &self.id
    }
    fn display_name(&self) -> &str {
        &self.display_name
    }
    fn command(&self) -> &str {
        &self.command
    }
    fn install_hint(&self) -> &str {
        "Install the ACP-speaking agent CLI and ensure it is on PATH."
    }
    fn sign_in_hint(&self) -> &str {
        "Sign in with the agent's own CLI before using it here."
    }
    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities { resume: true, images: false, effort: false, interrupt: true, accepts_full_model_id: false }
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

        let mut child = spawn_with_stdin(self.command(), &self.args, req.cwd.as_deref(), &req.env)?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| EngineError::spawn("acp child has no stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| EngineError::spawn("acp child has no stdout"))?;
        let mut io = AcpIo { stdin: Arc::new(Mutex::new(stdin)), reader: BufReader::new(stdout).lines() };

        // 1. initialize
        io.send(1, "initialize", json!({
            "protocolVersion": 1,
            "clientCapabilities": { "fs": { "readTextFile": true, "writeTextFile": true } }
        }))
        .await?;
        let init_resp = io.await_id(1, &on_event, req.approval, &cancel).await?;
        if let Some(err) = init_resp.get("error") {
            return Err(EngineError::protocol(format!("ACP initialize failed: {}", err)));
        }

        // 2. session/new — forward RAVENBOT's MCP servers. Agents that did not
        // declare HTTP MCP support in `initialize` only receive stdio servers;
        // skipped remote servers surface as a warning, never a failed turn.
        let supports_http = init_resp
            .pointer("/result/agentCapabilities/mcpCapabilities")
            .and_then(|caps| caps.get("http"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let (mcp_servers, skipped_http) = mcp_servers_json(&req.mcp_servers, supports_http);
        if !skipped_http.is_empty() {
            on_event(EngineEvent::Warning(format!(
                "ACP agent has no HTTP MCP support; skipped remote servers: {}",
                skipped_http.join(", ")
            )));
        }
        io.send(2, "session/new", json!({
            "cwd": req.cwd.clone().unwrap_or_else(|| ".".to_string()),
            "mcpServers": mcp_servers
        }))
        .await?;
        let session_resp = io.await_id(2, &on_event, req.approval, &cancel).await?;
        if let Some(err) = session_resp.get("error") {
            return Err(EngineError::protocol(format!("ACP session/new failed: {}", err)));
        }
        let session_id = session_resp
            .pointer("/result/sessionId")
            .and_then(|v| v.as_str())
            .or(req.resume.as_deref())
            .map(str::to_string)
            .ok_or_else(|| EngineError::protocol("ACP session/new returned no sessionId"))?;
        on_event(EngineEvent::SessionStarted {
            session_id: Some(session_id.clone()),
            model: req.model.clone(),
        });

        // 3. session/prompt — final_text accumulates from notifications.
        let mut outcome = EngineOutcome {
            session_id: Some(session_id.clone()),
            ..Default::default()
        };
        let mut final_text = String::new();
        let mut reasoning = String::new();

        io.send(3, "session/prompt", json!({
            "sessionId": session_id,
            "prompt": [{ "type": "text", "text": req.prompt }]
        }))
        .await?;

        // Read to the prompt response, capturing streamed text as we go.
        loop {
            let line = tokio::select! {
                _ = cancel.cancelled() => {
                    let _ = child.kill().await;
                    return Err(EngineError::cancelled());
                }
                line = io.reader.next_line() => line
                    .map_err(|e| EngineError::protocol(format!("ACP read error: {}", e)))?
                    .ok_or_else(|| EngineError::protocol("ACP process closed mid-turn"))?,
            };
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let Ok(frame) = serde_json::from_str::<Value>(line) else {
                continue;
            };

            if frame.get("method").is_none() && frame.get("id").and_then(|v| v.as_u64()) == Some(3) {
                if let Some(err) = frame.get("error") {
                    return Err(EngineError::upstream(format!("ACP prompt error: {}", err)));
                }
                outcome.stop_reason = frame
                    .pointer("/result/stopReason")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                outcome.ok = true;
                break;
            }

            if frame.get("method").is_some() && frame.get("id").is_some() {
                handle_server_request(&io, &frame, req.approval, &on_event).await;
                continue;
            }

            if let Some(update) = frame.pointer("/params/update") {
                handle_update(update, &mut final_text, &mut reasoning, &on_event);
            }
        }

        let _ = child.kill().await;
        outcome.final_text = final_text;
        outcome.reasoning = reasoning;
        Ok(outcome)
    }
}

/// Map an ACP `session/update` payload to normalized events.
fn handle_update(
    update: &Value,
    final_text: &mut String,
    reasoning: &mut String,
    on_event: &EngineCallback,
) {
    let kind = update.get("sessionUpdate").and_then(|v| v.as_str()).unwrap_or("");
    match kind {
        "agent_message_chunk" => {
            if let Some(text) = update.pointer("/content/text").and_then(|v| v.as_str()) {
                if !text.is_empty() {
                    final_text.push_str(text);
                    on_event(EngineEvent::TextDelta(text.to_string()));
                }
            }
        }
        "agent_thought_chunk" => {
            if let Some(text) = update.pointer("/content/text").and_then(|v| v.as_str()) {
                if !text.is_empty() {
                    reasoning.push_str(text);
                    on_event(EngineEvent::ReasoningDelta(text.to_string()));
                }
            }
        }
        "tool_call" => {
            let id = update.get("toolCallId").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let name = update
                .get("title")
                .and_then(|v| v.as_str())
                .or_else(|| update.get("kind").and_then(|v| v.as_str()))
                .unwrap_or("tool")
                .to_string();
            on_event(EngineEvent::Status("running_tool".to_string()));
            on_event(EngineEvent::ToolStarted { id, name, summary: None });
        }
        "tool_call_update" => {
            let id = update.get("toolCallId").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let status = update.get("status").and_then(|v| v.as_str()).unwrap_or("completed");
            if status == "completed" || status == "failed" {
                on_event(EngineEvent::Status("thinking".to_string()));
                on_event(EngineEvent::ToolFinished { id, ok: status == "completed" });
            }
        }
        _ => {}
    }
}

/// Answer a `session/request_permission` server request according to policy.
async fn handle_server_request(
    io: &AcpIo,
    frame: &Value,
    approval: EngineApproval,
    on_event: &EngineCallback,
) {
    let method = frame.get("method").and_then(|v| v.as_str()).unwrap_or("");
    let id = frame.get("id").cloned().unwrap_or(Value::Null);

    if method != "session/request_permission" {
        let _ = io
            .write(&json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": "unsupported" } }))
            .await;
        return;
    }

    let options = frame
        .pointer("/params/options")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let pick = |prefer: &[&str]| -> Option<String> {
        for want in prefer {
            for opt in &options {
                let kind = opt.get("kind").and_then(|v| v.as_str()).unwrap_or("");
                if kind.eq_ignore_ascii_case(want) {
                    return opt.get("optionId").and_then(|v| v.as_str()).map(str::to_string);
                }
            }
        }
        None
    };

    let chosen = match approval {
        EngineApproval::Full => pick(&["allow_always", "allow_once"]),
        EngineApproval::Auto => pick(&["allow_once", "allow_always"]),
        EngineApproval::Ask => None,
    };

    let reply = match chosen {
        Some(option_id) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "outcome": { "outcome": "selected", "optionId": option_id } }
        }),
        None => {
            on_event(EngineEvent::Warning(
                "Denied an ACP permission request (fail-closed)".to_string(),
            ));
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "outcome": { "outcome": "cancelled" } }
            })
        }
    };
    let _ = io.write(&reply).await;
}

/// Instances configured through `RAVENBOT_ACP_ENGINES`.
pub fn configured_engines() -> Vec<AcpEngine> {
    let Ok(raw) = std::env::var("RAVENBOT_ACP_ENGINES") else {
        return Vec::new();
    };
    let Ok(list) = serde_json::from_str::<Vec<Value>>(&raw) else {
        tracing::warn!("RAVENBOT_ACP_ENGINES is not valid JSON; ignoring");
        return Vec::new();
    };
    list.into_iter()
        .filter_map(|entry| {
            let id = entry.get("id")?.as_str()?.to_string();
            let command = entry.get("command")?.as_str()?.to_string();
            let display = entry
                .get("display_name")
                .and_then(|v| v.as_str())
                .unwrap_or(&id)
                .to_string();
            let args = entry
                .get("args")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
                .unwrap_or_default();
            Some(AcpEngine::new(id, display, command, args))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_events_are_normalized() {
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let ev = events.clone();
        let cb: EngineCallback = Arc::new(move |e| ev.lock().unwrap().push(e));
        let mut text = String::new();
        let mut reason = String::new();

        handle_update(
            &json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": "hi" } }),
            &mut text,
            &mut reason,
            &cb,
        );
        handle_update(
            &json!({ "sessionUpdate": "agent_thought_chunk", "content": { "type": "text", "text": "hm" } }),
            &mut text,
            &mut reason,
            &cb,
        );
        assert_eq!(text, "hi");
        assert_eq!(reason, "hm");
        assert!(events.lock().unwrap().iter().any(|e| matches!(e, EngineEvent::TextDelta(t) if t == "hi")));
    }

    fn stdio_server() -> EngineMcpServer {
        EngineMcpServer {
            name: "git".into(),
            transport: "stdio".into(),
            command: "uvx".into(),
            args: vec!["mcp-server-git".into()],
            env: std::collections::HashMap::from([
                ("B_KEY".to_string(), "2".to_string()),
                ("A_KEY".to_string(), "1".to_string()),
            ]),
            url: None,
            headers: std::collections::HashMap::new(),
        }
    }

    fn http_server() -> EngineMcpServer {
        EngineMcpServer {
            name: "remote".into(),
            transport: "http".into(),
            url: Some("https://mcp.example.com/api".into()),
            headers: std::collections::HashMap::from([
                ("Authorization".to_string(), "Bearer tok".to_string()),
                ("X-Trace".to_string(), "on".to_string()),
            ]),
            ..Default::default()
        }
    }

    #[test]
    fn mcp_servers_json_stdio_shape() {
        let (servers, skipped) = mcp_servers_json(&[stdio_server()], false);
        assert!(skipped.is_empty());
        assert_eq!(
            servers[0],
            json!({
                "name": "git",
                "command": "uvx",
                "args": ["mcp-server-git"],
                "env": [{"name": "A_KEY", "value": "1"}, {"name": "B_KEY", "value": "2"}],
            })
        );
    }

    #[test]
    fn mcp_servers_json_http_shape_and_downgrade() {
        let (servers, skipped) = mcp_servers_json(&[http_server()], true);
        assert!(skipped.is_empty());
        assert_eq!(
            servers[0],
            json!({
                "type": "http",
                "name": "remote",
                "url": "https://mcp.example.com/api",
                "headers": [
                    {"name": "Authorization", "value": "Bearer tok"},
                    {"name": "X-Trace", "value": "on"},
                ],
            })
        );
        // Without HTTP capability the remote server is skipped, stdio survives.
        let (servers, skipped) =
            mcp_servers_json(&[stdio_server(), http_server()], false);
        assert_eq!(servers.len(), 1);
        assert_eq!(skipped, vec!["remote".to_string()]);
    }

    #[test]
    fn mcp_servers_json_skips_broken_entries() {
        let no_url = EngineMcpServer {
            name: "bad".into(),
            transport: "http".into(),
            ..Default::default()
        };
        let no_command = EngineMcpServer {
            name: "empty".into(),
            transport: "stdio".into(),
            ..Default::default()
        };
        let (servers, skipped) = mcp_servers_json(&[no_url, no_command, stdio_server()], true);
        assert_eq!(servers.len(), 1);
        assert_eq!(skipped, vec!["bad".to_string(), "empty".to_string()]);
    }

    // One test owns the process env var so parallel tests can't race on it.
    #[test]
    fn configured_engines_parse_and_reject_bad_json() {
        std::env::set_var(
            "RAVENBOT_ACP_ENGINES",
            r#"[{"id":"gemini","display_name":"Gemini","command":"gemini","args":["--acp"]}]"#,
        );
        let engines = configured_engines();
        assert_eq!(engines.len(), 1);
        assert_eq!(engines[0].id(), "gemini");
        assert_eq!(engines[0].display_name(), "Gemini");

        std::env::set_var("RAVENBOT_ACP_ENGINES", "not json");
        assert!(configured_engines().is_empty());

        std::env::remove_var("RAVENBOT_ACP_ENGINES");
    }
}
