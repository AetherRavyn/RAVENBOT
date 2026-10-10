//! Bounded team control plane exposed over MCP.
//!
//! Mirrors Grok Bot's MCP control plane so external agents (Claude Desktop,
//! Cursor, Claude Code, …) can *run the fleet*: inspect bots/channels/offices,
//! create and configure agents, read and search transcripts, staff an office,
//! switch a bot's model, and send work — while deliberately NOT exposing
//! approvals, deletions, credentials, or computer lifecycle.
//!
//! Every tool name is prefixed `ravenbot_` and dispatched before skill lookup.

use ravenbot_db::Database;
use serde_json::{json, Value};
use std::time::Duration;
use uuid::Uuid;

/// Tool schemas advertised alongside the native skills in `tools/list`.
pub fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "ravenbot_list_bots",
            "description": "List every agent in the RAVENBOT fleet (id, name, rank, specialty, model, status).",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "ravenbot_list_offices",
            "description": "List offices (multi-agent teams) with their members, goal and policy.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "ravenbot_list_channels",
            "description": "List channels (contexts with shared instructions and working folder).",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "ravenbot_read_messages",
            "description": "Read a compact page of a thread's transcript (most recent messages).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "thread_id": { "type": "string", "description": "Thread id" },
                    "limit": { "type": "integer", "description": "Max messages (default 30, max 100)" }
                },
                "required": ["thread_id"]
            }
        }),
        json!({
            "name": "ravenbot_search_messages",
            "description": "Search message transcripts for a query and return matching snippets.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "limit": { "type": "integer", "description": "Max hits (default 20, max 50)" }
                },
                "required": ["query"]
            }
        }),
        json!({
            "name": "ravenbot_create_bot",
            "description": "Create a new agent. Defaults to the owner's configured default model.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "description": { "type": "string" },
                    "rank": { "type": "string" },
                    "specialty": { "type": "string" },
                    "system_prompt": { "type": "string" },
                    "provider": { "type": "string" },
                    "model": { "type": "string" },
                    "skills": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["name"]
            }
        }),
        json!({
            "name": "ravenbot_update_bot",
            "description": "Update an agent's model, system prompt, skills, rank or specialty.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "bot_id": { "type": "string" },
                    "provider": { "type": "string" },
                    "model": { "type": "string" },
                    "system_prompt": { "type": "string" },
                    "skills": { "type": "array", "items": { "type": "string" } },
                    "rank": { "type": "string" },
                    "specialty": { "type": "string" }
                },
                "required": ["bot_id"]
            }
        }),
        json!({
            "name": "ravenbot_assign_bot_to_office",
            "description": "Add an existing agent to an office with a rank and specialty.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "chatroom_id": { "type": "string" },
                    "bot_id": { "type": "string" },
                    "rank": { "type": "string" },
                    "specialty": { "type": "string" }
                },
                "required": ["chatroom_id", "bot_id"]
            }
        }),
        json!({
            "name": "ravenbot_send_task",
            "description": "Send a task to an agent by name or id and wait for its answer.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "bot": { "type": "string", "description": "Agent name or id" },
                    "message": { "type": "string" },
                    "timeout_secs": { "type": "integer", "description": "Max wait (default 600, max 1800)" }
                },
                "required": ["bot", "message"]
            }
        }),
        json!({
            "name": "ravenbot_kill_switch_status",
            "description": "Report whether the headless kill switch is active.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
    ]
}

/// Whether `name` is one of the team control tools.
pub fn is_team_tool(name: &str) -> bool {
    matches!(
        name,
        "ravenbot_list_bots"
            | "ravenbot_list_offices"
            | "ravenbot_list_channels"
            | "ravenbot_read_messages"
            | "ravenbot_search_messages"
            | "ravenbot_create_bot"
            | "ravenbot_update_bot"
            | "ravenbot_assign_bot_to_office"
            | "ravenbot_send_task"
            | "ravenbot_kill_switch_status"
    )
}

fn ok(value: Value) -> Value {
    json!({
        "content": [{ "type": "text", "text": serde_json::to_string_pretty(&value).unwrap_or_default() }],
        "isError": false
    })
}

fn err(message: impl Into<String>) -> Value {
    json!({
        "content": [{ "type": "text", "text": message.into() }],
        "isError": true
    })
}

fn kill_switch_active() -> bool {
    std::env::var("RAVENBOT_KILL_SWITCH")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push('…');
    out
}

fn message_text(message: &ravenbot_core::Message) -> String {
    match &message.content {
        ravenbot_core::MessageContent::Text { text, .. } => text.clone(),
        ravenbot_core::MessageContent::Checklist { text, items } => {
            let mut out = text.clone().unwrap_or_default();
            for item in items {
                out.push_str(&format!("\n- {} ({:?})", item.label, item.status));
            }
            out
        }
        ravenbot_core::MessageContent::ToolCall { tool_name, .. } => {
            format!("[tool call: {tool_name}]")
        }
        ravenbot_core::MessageContent::ToolResult { tool_name, .. } => {
            format!("[tool result: {tool_name}]")
        }
    }
}

fn role_str(role: &ravenbot_core::MessageRole) -> String {
    serde_json::to_value(role)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

/// Dispatch a team tool call. Returns `None` when the name is not a team tool.
pub async fn call(db: &Database, name: &str, args: &Value) -> Option<Value> {
    if !is_team_tool(name) {
        return None;
    }
    if kill_switch_active() {
        return Some(err("RAVENBOT kill switch is active: team operations are paused."));
    }
    let pool = db.pool();

    let result = match name {
        "ravenbot_list_bots" => {
            match ravenbot_db::queries::BotQueries::list(pool).await {
                Ok(bots) => ok(json!({
                    "count": bots.len(),
                    "bots": bots.iter().map(|b| json!({
                        "id": b.id,
                        "name": b.name,
                        "rank": b.rank,
                        "specialty": b.specialty,
                        "provider": b.config.model_provider,
                        "model": b.config.model_id,
                        "is_orchestrator": b.is_orchestrator,
                        "skills": b.skills,
                        "status": b.status,
                    })).collect::<Vec<_>>()
                })),
                Err(e) => err(format!("Failed to list bots: {e}")),
            }
        }
        "ravenbot_list_offices" => {
            match ravenbot_db::queries::ChatRoomQueries::list(pool).await {
                Ok(rooms) => {
                    let mut offices = Vec::new();
                    for room in rooms {
                        let members = ravenbot_db::queries::ChatRoomQueries::list_members(pool, room.id)
                            .await
                            .unwrap_or_default();
                        offices.push(json!({
                            "id": room.id,
                            "name": room.name,
                            "description": room.description,
                            "template": room.office_template,
                            "goal": room.goal,
                            "policy": room.policy,
                            "members": members.iter().map(|m| json!({
                                "bot_id": m.bot_id,
                                "rank": m.rank,
                                "specialty": m.specialty,
                            })).collect::<Vec<_>>()
                        }));
                    }
                    ok(json!({ "count": offices.len(), "offices": offices }))
                }
                Err(e) => err(format!("Failed to list offices: {e}")),
            }
        }
        "ravenbot_list_channels" => {
            match ravenbot_db::queries::ChannelQueries::list(pool).await {
                Ok(channels) => ok(json!({
                    "count": channels.len(),
                    "channels": channels.iter().map(|c| json!({
                        "id": c.id,
                        "name": c.name,
                        "description": c.description,
                        "instructions": c.instructions,
                        "working_folder": c.working_folder,
                    })).collect::<Vec<_>>()
                })),
                Err(e) => err(format!("Failed to list channels: {e}")),
            }
        }
        "ravenbot_read_messages" => {
            let Some(thread_id) = args.get("thread_id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok()) else {
                return Some(err("ravenbot_read_messages requires a valid 'thread_id'"));
            };
            let limit = args
                .get("limit")
                .and_then(|v| v.as_u64())
                .unwrap_or(30)
                .clamp(1, 100) as usize;
            match ravenbot_db::queries::MessageQueries::list_by_thread(pool, thread_id).await {
                Ok(messages) => {
                    let total = messages.len();
                    let start = total.saturating_sub(limit);
                    let page: Vec<Value> = messages[start..]
                        .iter()
                        .map(|m| json!({
                            "id": m.id,
                            "role": role_str(&m.role),
                            "sender": m.sender_name,
                            "text": truncate(&message_text(m), 1200),
                            "created_at": m.created_at,
                        }))
                        .collect();
                    ok(json!({ "thread_id": thread_id, "total": total, "messages": page }))
                }
                Err(e) => err(format!("Failed to read messages: {e}")),
            }
        }
        "ravenbot_search_messages" => {
            let Some(query) = args.get("query").and_then(|v| v.as_str()).filter(|q| !q.trim().is_empty()) else {
                return Some(err("ravenbot_search_messages requires a non-empty 'query'"));
            };
            let limit = args
                .get("limit")
                .and_then(|v| v.as_u64())
                .unwrap_or(20)
                .clamp(1, 50) as u32;
            match ravenbot_db::queries::SearchQueries::messages(pool, query, limit).await {
                Ok(hits) => ok(json!({
                    "query": query,
                    "hits": hits.iter().map(|(message, thread_title)| json!({
                        "message_id": message.id,
                        "thread_id": message.thread_id,
                        "thread_title": thread_title,
                        "role": role_str(&message.role),
                        "snippet": truncate(&message_text(message), 400),
                    })).collect::<Vec<_>>()
                })),
                Err(e) => err(format!("Search failed: {e}")),
            }
        }
        "ravenbot_create_bot" => {
            let Some(name) = args.get("name").and_then(|v| v.as_str()).filter(|n| !n.trim().is_empty()) else {
                return Some(err("ravenbot_create_bot requires 'name'"));
            };
            let description = args.get("description").and_then(|v| v.as_str()).unwrap_or("");
            let mut bot = ravenbot_core::Bot::new(name, description);
            if let Some(rank) = args.get("rank").and_then(|v| v.as_str()) {
                bot.rank = Some(rank.to_string());
            }
            if let Some(specialty) = args.get("specialty").and_then(|v| v.as_str()) {
                bot.specialty = Some(specialty.to_string());
            }
            if let Some(prompt) = args.get("system_prompt").and_then(|v| v.as_str()) {
                bot.config.custom_prompt = Some(prompt.to_string());
            }
            if let Some(skills) = args.get("skills").and_then(|v| v.as_array()) {
                bot.skills = skills.iter().filter_map(|s| s.as_str().map(str::to_string)).collect();
            }
            let (provider, model) = resolve_default_model(pool).await;
            bot.config.model_provider = args
                .get("provider")
                .and_then(|v| v.as_str())
                .unwrap_or(&provider)
                .to_string();
            bot.config.model_id = args
                .get("model")
                .and_then(|v| v.as_str())
                .unwrap_or(&model)
                .to_string();
            match ravenbot_db::queries::BotQueries::insert(pool, &bot).await {
                Ok(()) => ok(json!({ "created": true, "bot_id": bot.id, "name": bot.name, "provider": bot.config.model_provider, "model": bot.config.model_id })),
                Err(e) => err(format!("Failed to create bot: {e}")),
            }
        }
        "ravenbot_update_bot" => {
            let Some(bot_id) = args.get("bot_id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok()) else {
                return Some(err("ravenbot_update_bot requires a valid 'bot_id'"));
            };
            let existing = ravenbot_db::queries::BotQueries::get(pool, bot_id).await;
            let Some(mut bot) = existing.ok().flatten() else {
                return Some(err("Bot not found"));
            };
            if let Some(provider) = args.get("provider").and_then(|v| v.as_str()) {
                bot.config.model_provider = provider.to_string();
            }
            if let Some(model) = args.get("model").and_then(|v| v.as_str()) {
                bot.config.model_id = model.to_string();
            }
            if let Some(prompt) = args.get("system_prompt").and_then(|v| v.as_str()) {
                bot.config.custom_prompt = Some(prompt.to_string());
            }
            if let Some(skills) = args.get("skills").and_then(|v| v.as_array()) {
                bot.skills = skills.iter().filter_map(|s| s.as_str().map(str::to_string)).collect();
            }
            if let Some(rank) = args.get("rank").and_then(|v| v.as_str()) {
                bot.rank = Some(rank.to_string());
            }
            if let Some(specialty) = args.get("specialty").and_then(|v| v.as_str()) {
                bot.specialty = Some(specialty.to_string());
            }
            match ravenbot_db::queries::BotQueries::update(pool, &bot).await {
                Ok(()) => ok(json!({ "updated": true, "bot_id": bot.id, "provider": bot.config.model_provider, "model": bot.config.model_id })),
                Err(e) => err(format!("Failed to update bot: {e}")),
            }
        }
        "ravenbot_assign_bot_to_office" => {
            let Some(chatroom_id) = args.get("chatroom_id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok()) else {
                return Some(err("ravenbot_assign_bot_to_office requires a valid 'chatroom_id'"));
            };
            let Some(bot_id) = args.get("bot_id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok()) else {
                return Some(err("ravenbot_assign_bot_to_office requires a valid 'bot_id'"));
            };
            let rank = args.get("rank").and_then(|v| v.as_str()).unwrap_or("Member").to_string();
            let specialty = args
                .get("specialty")
                .and_then(|v| v.as_str())
                .unwrap_or("Generalist")
                .to_string();
            let member = ravenbot_core::ChatRoomMember {
                chatroom_id,
                bot_id,
                rank,
                specialty,
                joined_at: chrono::Utc::now(),
            };
            match ravenbot_db::queries::ChatRoomQueries::add_member(pool, &member).await {
                Ok(()) => ok(json!({ "assigned": true, "chatroom_id": chatroom_id, "bot_id": bot_id })),
                Err(e) => err(format!("Failed to assign bot: {e}")),
            }
        }
        "ravenbot_send_task" => {
            let target = args
                .get("bot")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let message = args
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if target.is_empty() || message.is_empty() {
                return Some(err("ravenbot_send_task requires 'bot' and 'message'"));
            }
            // Resolve to a real bot first (fail fast with a clear error).
            let resolved = if let Ok(id) = Uuid::parse_str(&target) {
                ravenbot_db::queries::BotQueries::get(pool, id).await.ok().flatten()
            } else {
                ravenbot_db::queries::BotQueries::list(pool)
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .find(|b| b.name.eq_ignore_ascii_case(&target))
            };
            let Some(bot) = resolved else {
                return Some(err(format!("No agent named '{target}'")));
            };
            let timeout_secs = args
                .get("timeout_secs")
                .and_then(|v| v.as_u64())
                .unwrap_or(600)
                .clamp(30, 1800);
            match send_task_subprocess(&bot.id, &message, timeout_secs).await {
                Ok(output) => ok(json!({
                    "bot_id": bot.id,
                    "bot_name": bot.name,
                    "output": output,
                })),
                Err(e) => err(e),
            }
        }
        "ravenbot_kill_switch_status" => ok(json!({
            "active": kill_switch_active(),
            "env": "RAVENBOT_KILL_SWITCH",
        })),
        _ => return None,
    };
    Some(result)
}

/// Run one headless turn in a child process (same binary, same database).
async fn send_task_subprocess(bot_id: &Uuid, message: &str, timeout_secs: u64) -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot resolve executable: {e}"))?;
    let mut cmd = tokio::process::Command::new(exe);
    cmd.arg("run")
        .arg("--bot")
        .arg(bot_id.to_string())
        .arg("--message")
        .arg(message)
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(timeout_secs), cmd.output())
        .await
        .map_err(|_| format!("task timed out after {timeout_secs}s"))?
        .map_err(|e| format!("failed to run task: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if output.status.success() {
        Ok(if stdout.is_empty() { "(no output)".to_string() } else { stdout })
    } else {
        Err(format!(
            "task failed (exit {}): {}",
            output.status.code().unwrap_or(-1),
            if stderr.is_empty() { stdout } else { stderr }
        ))
    }
}

/// Owner's configured default model, falling back to the local sovereign one.
async fn resolve_default_model(pool: &sqlx::SqlitePool) -> (String, String) {
    let provider = ravenbot_db::queries::AppSettingsQueries::get(pool, "default_provider")
        .await
        .ok()
        .flatten();
    let model = ravenbot_db::queries::AppSettingsQueries::get(pool, "default_model")
        .await
        .ok()
        .flatten();
    if let (Some(p), Some(m)) = (provider, model) {
        if !p.trim().is_empty() && !m.trim().is_empty() {
            return (p, m);
        }
    }
    ("ollama".to_string(), "llama3.1".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_team_tool_has_a_schema() {
        let defs = tool_definitions();
        assert!(defs.len() >= 10);
        for d in &defs {
            let name = d.get("name").and_then(|v| v.as_str()).unwrap();
            assert!(is_team_tool(name), "{name} should be dispatchable");
            assert!(d.get("inputSchema").is_some(), "{name} needs an input schema");
        }
    }

    #[test]
    fn non_team_tools_are_not_dispatchable() {
        assert!(!is_team_tool("web_search"));
        assert!(!is_team_tool("shell_exec"));
    }

    #[test]
    fn truncate_is_utf8_safe() {
        assert_eq!(truncate("hello", 10), "hello");
        assert!(truncate("héllo wörld", 5).ends_with('…'));
    }
}
