//! RAVENBOT as an MCP server over stdio
//!
//! Exposes every built-in RAVENBOT skill as an MCP tool so **external**
//! agents (Claude Code, GROK, anything MCP-speaking) can drive RAVENBOT's
//! fleet: web search, shell, files, git, image generation, memory, offices.
//!
//! Protocol: JSON-RPC 2.0, line-delimited JSON on stdin/stdout (MCP stdio).

use ravenbot_db::Database;
use ravenbot_skills::{SkillContext, SkillRegistry};
use std::sync::Arc;
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use uuid::Uuid;

/// Where the headless MCP server looks for the RAVENBOT database.
/// Mirrors the desktop app's `app_data_dir/ravenbot.db` when possible.
pub fn default_db_path() -> PathBuf {
    if let Ok(p) = std::env::var("RAVENBOT_DB") {
        return PathBuf::from(p);
    }
    let dir = if let Some(dir) = dirs_data_dir() {
        dir.join("ravenbot")
    } else {
        PathBuf::from(".")
    };
    dir.join("ravenbot.db")
}

fn dirs_data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        std::env::var("HOME").ok().map(|h| PathBuf::from(h).join("Library/Application Support"))
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA").ok().map(|p| PathBuf::from(p))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::env::var("XDG_DATA_HOME")
            .ok()
            .map(PathBuf::from)
            .or_else(|| std::env::var("HOME").ok().map(|h| PathBuf::from(h).join(".local/share")))
    }
}

/// Run the MCP server loop over stdio. Returns when stdin closes.
pub async fn run_stdio() -> Result<(), String> {
    let db_path = default_db_path();
    if let Some(parent) = db_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create data dir: {}", e))?;
    }
    let db = Database::new(&db_path)
        .await
        .map_err(|e| format!("Failed to open database at {:?}: {}", db_path, e))?;
    let registry = Arc::new(SkillRegistry::new_builtin());

    tracing::info!(db = %db_path.display(), tools = registry.list().len(), "MCP server ready");

    let stdin = BufReader::new(tokio::io::stdin());
    let mut stdout = tokio::io::stdout();

    let mut lines = stdin.lines();
    while let Some(line) = lines.next_line().await.map_err(|e| e.to_string())? {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some(req) = serde_json::from_str::<serde_json::Value>(line).ok() else {
            continue;
        };

        let method = req.get("method").and_then(|v| v.as_str()).unwrap_or("");
        let id = req.get("id").cloned();

        // Notifications (no id) get no response
        let response = match handle(&db, &registry, method, &req).await {
            Some(result) if id.is_some() => serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": result
            }),
            None if id.is_some() => serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": "Method not found" }
            }),
            _ => continue,
        };

        stdout
            .write_all(format!("{}\n", response).as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        stdout.flush().await.map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// Handle a request. Returns `Some(result)` for known methods,
/// `None` for unknown methods, and `None` without an id for notifications.
async fn handle(
    db: &Database,
    registry: &SkillRegistry,
    method: &str,
    req: &serde_json::Value,
) -> Option<serde_json::Value> {
    match method {
        "initialize" => Some(serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": {
                "name": "ravenbot",
                "version": env!("CARGO_PKG_VERSION"),
                "description": "RAVENBOT sovereign agent fleet: web search, shell, files, git, image generation, memory, computer control"
            }
        })),
        "notifications/initialized" | "initialized" => None,
        "ping" => Some(serde_json::json!({})),
        "tools/list" => Some(serde_json::json!({
            "tools": registry.list().iter().map(|skill| serde_json::json!({
                "name": skill.id(),
                "description": skill.description(),
                "inputSchema": skill.input_schema()
            })).collect::<Vec<_>>()
        })),
        "tools/call" => Some(call_tool(db, registry, req).await),
        _ => None,
    }
}

/// Execute a skill as an MCP tool call.
async fn call_tool(
    db: &Database,
    registry: &SkillRegistry,
    req: &serde_json::Value,
) -> serde_json::Value {
    let name = req.pointer("/params/name").and_then(|v| v.as_str()).unwrap_or("");
    let args = req.pointer("/params/arguments").cloned().unwrap_or(serde_json::json!({}));

    let Some(skill) = registry.get(name) else {
        return serde_json::json!({
            "content": [{ "type": "text", "text": format!("Unknown tool: {}", name) }],
            "isError": true
        });
    };

    // Honor the headless kill switch env so operators can pause remote drives
    if std::env::var("RAVENBOT_KILL_SWITCH")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        return serde_json::json!({
            "content": [{ "type": "text", "text": "RAVENBOT kill switch active: tool calls paused" }],
            "isError": true
        });
    }

    let context = SkillContext {
        bot_id: Uuid::new_v4(),
        run_id: Uuid::new_v4(),
        thread_id: Uuid::new_v4(),
    };

    tracing::info!(tool = %name, "MCP tool call");
    let _ = db; // database opened for parity with the desktop app; tools use env/keychain

    let result = skill.execute(&context, args).await;
    let (ok, payload) = match result {
        Ok(r) => (r.success, r.output),
        Err(e) => (false, serde_json::json!({ "error": e.to_string() })),
    };

    serde_json::json!({
        "content": [{ "type": "text", "text": serde_json::to_string(&payload).unwrap_or_default() }],
        "isError": !ok
    })
}
