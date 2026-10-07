//! RAVENBOT - A Sovereign, Local-First, Rust-Native Multi-Agent Desktop OS
//!
//! This crate provides the Tauri shell and IPC handlers.

use ravenbot_core::*;
use ravenbot_db::Database;
use ravenbot_models::ModelDiscovery;
use ravenbot_runtime::graph::TaskGraph;
use std::sync::Arc;
use tauri::Emitter;
use tauri::Manager;
use tauri::State;
use uuid::Uuid;

mod office_org;

/// Application state shared across handlers
pub struct AppState {
    pub db: Database,
    pub runtime: Arc<ravenbot_runtime::Runtime>,
    pub scheduler: Arc<ravenbot_scheduler::Scheduler>,
    pub model_discovery: Arc<ModelDiscovery>,
    /// Base URL of the running webhook receiver (set during setup).
    pub webhook_base_url: std::sync::RwLock<String>,
}

// Tauri commands (IPC handlers)

/// An inline attachment sent from the composer (paste/drop/pick). Images carry
/// a base64 payload the model can see; other files are preserved and surfaced
/// to the model by name/type rather than being silently dropped.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct AttachmentInput {
    pub name: String,
    pub mime: String,
    /// Base64-encoded file data (no data-URI prefix)
    pub data: String,
}

fn build_image_attachments(
    attachments: Vec<AttachmentInput>,
) -> Vec<ravenbot_core::Attachment> {
    attachments
        .into_iter()
        .map(|a| {
            let is_image = a.mime.trim().to_lowercase().starts_with("image/");
            ravenbot_core::Attachment {
                id: Uuid::new_v4(),
                name: a.name,
                mime_type: a.mime,
                size: a.data.len() as u64,
                path: String::new(),
                data: Some(a.data),
                is_image,
            }
        })
        .collect()
}

/// Local-first default provider/model for newly created bots (Ollama unless
/// RAVENBOT_DEFAULT_PROVIDER / RAVENBOT_DEFAULT_MODEL override it).
fn default_bot_model() -> (String, String) {
    let provider = std::env::var("RAVENBOT_DEFAULT_PROVIDER")
        .ok()
        .filter(|p| !p.trim().is_empty())
        .unwrap_or_else(|| "ollama".to_string());
    let model = std::env::var("RAVENBOT_DEFAULT_MODEL")
        .ok()
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| match provider.as_str() {
            "openrouter" => "anthropic/claude-3.5-sonnet".to_string(),
            "anthropic" => "claude-3-5-sonnet-20241022".to_string(),
            "openai" => "gpt-4o".to_string(),
            _ => "llama3.1".to_string(),
        });
    (provider, model)
}

/// Resolve the model new agents should start on: the owner's configured
/// default (Settings → default model) first, then env, then the local default.
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
    default_bot_model()
}

#[tauri::command]
async fn create_bot(
    state: State<'_, AppState>,
    name: String,
    description: String,
    avatar_url: Option<String>,
    avatar_style: Option<String>,
) -> Result<Bot, String> {
    let mut bot = Bot::new(name, description);
    if let Some(url) = avatar_url {
        bot.avatar_url = Some(url);
    }
    if let Some(style) = avatar_style {
        bot.avatar_style = Some(style);
    }
    let (default_provider, default_model) = resolve_default_model(state.db.pool()).await;
    bot.config.model_provider = default_provider;
    bot.config.model_id = default_model;
    
    ravenbot_db::queries::BotQueries::insert(state.db.pool(), &bot)
        .await
        .map_err(|e| e.to_string())?;
    
    Ok(bot)
}

#[tauri::command]
async fn list_bots(state: State<'_, AppState>) -> Result<Vec<Bot>, String> {
    ravenbot_db::queries::BotQueries::list(state.db.pool())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_bot(state: State<'_, AppState>, bot_id: Uuid) -> Result<Option<Bot>, String> {
    ravenbot_db::queries::BotQueries::get(state.db.pool(), bot_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn update_bot(state: State<'_, AppState>, bot: Bot) -> Result<(), String> {
    ravenbot_db::queries::BotQueries::update(state.db.pool(), &bot)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_bot(state: State<'_, AppState>, bot_id: Uuid) -> Result<(), String> {
    ravenbot_db::queries::BotQueries::delete(state.db.pool(), bot_id)
        .await
        .map_err(|e| e.to_string())
}

/// Contact state for the fleet list: pinned / hidden / last read.
#[tauri::command]
async fn list_bot_contacts(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    ravenbot_db::queries::BotContactQueries::ensure_rows(state.db.pool())
        .await
        .map_err(|e| e.to_string())?;
    let rows = ravenbot_db::queries::BotContactQueries::list(state.db.pool())
        .await
        .map_err(|e| e.to_string())?;
    Ok(serde_json::Value::Array(
        rows.into_iter()
            .map(|(bot_id, pinned, hidden, last_read_at)| {
                serde_json::json!({
                    "bot_id": bot_id,
                    "pinned": pinned,
                    "hidden": hidden,
                    "last_read_at": last_read_at,
                })
            })
            .collect(),
    ))
}

#[tauri::command]
async fn set_bot_pinned(
    state: State<'_, AppState>,
    bot_id: Uuid,
    pinned: bool,
) -> Result<(), String> {
    ravenbot_db::queries::BotContactQueries::set_pinned(state.db.pool(), bot_id, pinned)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_bot_hidden(
    state: State<'_, AppState>,
    bot_id: Uuid,
    hidden: bool,
) -> Result<(), String> {
    ravenbot_db::queries::BotContactQueries::set_hidden(state.db.pool(), bot_id, hidden)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn mark_bot_read(state: State<'_, AppState>, bot_id: Uuid) -> Result<(), String> {
    ravenbot_db::queries::BotContactQueries::mark_read(state.db.pool(), bot_id)
        .await
        .map_err(|e| e.to_string())
}

/// Persist manual sidebar order: each bot id gets sort_order = its position.
#[tauri::command]
async fn reorder_bots(state: State<'_, AppState>, ordered_ids: Vec<Uuid>) -> Result<(), String> {
    ravenbot_db::queries::BotQueries::reorder(state.db.pool(), &ordered_ids)
        .await
        .map_err(|e| e.to_string())
}

/// Unread assistant-message count per bot (drives sidebar badges).
#[tauri::command]
async fn get_unread_counts(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    ravenbot_db::queries::BotContactQueries::ensure_rows(state.db.pool())
        .await
        .map_err(|e| e.to_string())?;
    let rows = ravenbot_db::queries::BotContactQueries::unread_counts(state.db.pool())
        .await
        .map_err(|e| e.to_string())?;
    let mut map = serde_json::Map::new();
    for (bot_id, count) in rows {
        if count > 0 {
            map.insert(bot_id.to_string(), serde_json::json!(count));
        }
    }
    Ok(serde_json::Value::Object(map))
}

/// Clone an agent (identity, model, skills, permissions) as an independent bot.
#[tauri::command]
async fn duplicate_bot(state: State<'_, AppState>, bot_id: Uuid) -> Result<Bot, String> {
    let source = ravenbot_db::queries::BotQueries::get(state.db.pool(), bot_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Bot not found".to_string())?;
    let mut bot = source.clone();
    let now = chrono::Utc::now();
    bot.id = Uuid::new_v4();
    bot.name = format!("{} (copy)", source.name);
    bot.description = source.description.clone();
    bot.is_orchestrator = false;
    bot.delegate_to = Vec::new();
    bot.status = ravenbot_core::BotStatus::Idle;
    bot.created_at = now;
    bot.updated_at = now;
    bot.last_active_at = None;
    ravenbot_db::queries::BotQueries::insert(state.db.pool(), &bot)
        .await
        .map_err(|e| e.to_string())?;
    let _ = ravenbot_db::queries::BotContactQueries::ensure_rows(state.db.pool()).await;
    Ok(bot)
}

#[tauri::command]
async fn create_thread(
    state: State<'_, AppState>,
    bot_id: Uuid,
    title: String,
    ephemeral: Option<bool>,
    channel_id: Option<Uuid>,
) -> Result<Thread, String> {
    let mut thread = if ephemeral.unwrap_or(false) {
        Thread::new_ephemeral(bot_id, title)
    } else {
        Thread::new(bot_id, title)
    };
    thread.channel_id = channel_id;
    ravenbot_db::queries::ThreadQueries::create(state.db.pool(), &thread)
        .await
        .map_err(|e| e.to_string())?;
    Ok(thread)
}

#[tauri::command]
async fn list_threads(
    state: State<'_, AppState>,
    bot_id: Uuid,
) -> Result<Vec<Thread>, String> {
    ravenbot_db::queries::ThreadQueries::list_by_bot(state.db.pool(), bot_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_messages(
    state: State<'_, AppState>,
    thread_id: Uuid,
) -> Result<Vec<Message>, String> {
    ravenbot_db::queries::MessageQueries::list_by_thread(state.db.pool(), thread_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn rename_thread(
    state: State<'_, AppState>,
    thread_id: Uuid,
    title: String,
) -> Result<(), String> {
    ravenbot_db::queries::ThreadQueries::rename(state.db.pool(), thread_id, &title)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_thread(state: State<'_, AppState>, thread_id: Uuid) -> Result<(), String> {
    ravenbot_db::queries::ThreadQueries::delete(state.db.pool(), thread_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn search_messages(
    state: State<'_, AppState>,
    query: String,
    limit: Option<u32>,
) -> Result<Vec<serde_json::Value>, String> {
    let results = ravenbot_db::queries::SearchQueries::messages(
        state.db.pool(),
        &query,
        limit.unwrap_or(20),
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(results
        .into_iter()
        .map(|(message, thread_title)| {
            let snippet = match &message.content {
                ravenbot_core::MessageContent::Text { text, .. } => {
                    let lower = text.to_lowercase();
                    let ql = query.to_lowercase();
                    let pos = lower.find(&ql).unwrap_or(0);
                    let start = pos.saturating_sub(60);
                    let snippet: String = text
                        .chars()
                        .skip(start)
                        .take(160)
                        .collect();
                    snippet
                }
                _ => String::new(),
            };
            serde_json::json!({
                "message_id": message.id.to_string(),
                "thread_id": message.thread_id.to_string(),
                "thread_title": thread_title,
                "role": match message.role {
                    ravenbot_core::MessageRole::User => "user",
                    ravenbot_core::MessageRole::Assistant => "assistant",
                    ravenbot_core::MessageRole::System => "system",
                    ravenbot_core::MessageRole::Tool => "tool",
                },
                "snippet": snippet,
                "created_at": message.created_at.to_rfc3339(),
            })
        })
        .collect())
}

/// Build a stream emitter that forwards runtime StreamEvents to the UI
/// via the `agent-stream` Tauri event channel.
fn make_stream_emitter(app: tauri::AppHandle) -> ravenbot_runtime::StreamEmitter {
    use ravenbot_runtime::StreamEvent;
    Arc::new(move |event: StreamEvent| {
        if let Ok(payload) = serde_json::to_value(&event) {
            let _ = app.emit("agent-stream", payload);
        }
    })
}

/// Serializes office (chatroom) executions so their shared fallback stream
/// emitter and graph execution don't interleave across concurrent calls.
static OFFICE_EXEC_LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();

fn office_exec_lock() -> &'static tokio::sync::Mutex<()> {
    OFFICE_EXEC_LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// Execute a routine instruction: create a thread, insert the instruction
/// as the user turn, and run the bot with streaming wired for that thread.
pub async fn execute_routine_instruction(
    db: &Database,
    runtime: &Arc<ravenbot_runtime::Runtime>,
    app: &tauri::AppHandle,
    routine: &ravenbot_core::Routine,
) -> Result<(), String> {
    let thread = Thread::new(routine.bot_id, format!("Routine: {}", routine.name));
    ravenbot_db::queries::ThreadQueries::create(db.pool(), &thread)
        .await
        .map_err(|e| e.to_string())?;

    let user_msg = Message::user(thread.id, &routine.instruction);
    ravenbot_db::queries::MessageQueries::insert(db.pool(), &user_msg)
        .await
        .map_err(|e| e.to_string())?;

    let mut run = Run::new(routine.bot_id, thread.id);
    ravenbot_db::queries::RunQueries::insert(db.pool(), &run)
        .await
        .map_err(|e| e.to_string())?;

    runtime.set_thread_emitter(thread.id, Some(make_stream_emitter(app.clone())));
    // Routines run unattended: auto-allow approval gates for THIS run (audited)
    // rather than parking for 10 minutes and denying.
    runtime.allow_approvals_for_run(run.id, true);
    let result = runtime.execute_run(&mut run).await;
    runtime.allow_approvals_for_run(run.id, false);
    runtime.set_thread_emitter(thread.id, None);
    result.map_err(|e| e.to_string())?;

    let _ = app.emit(
        "routine-executed",
        serde_json::json!({
            "routine_id": routine.id.to_string(),
            "thread_id": thread.id.to_string(),
            "bot_id": routine.bot_id.to_string(),
            "ok": true
        }),
    );

    Ok(())
}

// ---- Sync (signed bundle export/import) commands ----

#[tauri::command]
async fn export_bot_bundle(
    state: State<'_, AppState>,
    bot_id: Uuid,
    include_memory: Option<bool>,
) -> Result<ravenbot_core::BotBundle, String> {
    let manager = ravenbot_sync::bundle::BundleManager::open(state.db.pool().clone()).await?;
    manager
        .export_bot(bot_id, include_memory.unwrap_or(true))
        .await
}

/// Edit a user message and resend: deletes this turn and everything after it,
/// inserts the edited user turn, and executes a fresh run. This is the
/// "edit message" flow (Grok-style).
#[tauri::command]
async fn edit_and_resend(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    thread_id: Uuid,
    message_id: Uuid,
    content: String,
    attachments: Option<Vec<AttachmentInput>>,
) -> Result<Message, String> {
    let messages = ravenbot_db::queries::MessageQueries::list_by_thread(state.db.pool(), thread_id)
        .await
        .map_err(|e| e.to_string())?;

    let idx = messages
        .iter()
        .position(|m| m.id == message_id)
        .ok_or_else(|| "Message not found".to_string())?;

    if !matches!(messages[idx].role, ravenbot_core::MessageRole::User) {
        return Err("Only user messages can be edited".to_string());
    }

    // Delete this turn and everything after it
    for msg in &messages[idx..] {
        ravenbot_db::queries::MessageQueries::delete(state.db.pool(), msg.id)
            .await
            .map_err(|e| e.to_string())?;
    }

    // Insert the edited user turn (with any new inline image attachments)
    let mut user_msg = Message::user(thread_id, &content);
    if let Some(atts) = attachments {
        user_msg.attachments = build_image_attachments(atts);
    }
    ravenbot_db::queries::MessageQueries::insert(state.db.pool(), &user_msg)
        .await
        .map_err(|e| e.to_string())?;

    let thread = ravenbot_db::queries::ThreadQueries::get(state.db.pool(), thread_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Thread not found".to_string())?;

    let mut run = Run::new(thread.bot_id, thread_id);
    ravenbot_db::queries::RunQueries::insert(state.db.pool(), &run)
        .await
        .map_err(|e| e.to_string())?;

    let result = execute_run_with_stream(&state, &app, &mut run).await;

    if let Err(err) = result {
        let err_str = err.to_string();
        tracing::warn!("Edit-and-resend run error: {}", err_str);
        let error_msg = Message::assistant(
            thread_id,
            format!("⚠️ **Model Error:** {}\n\n{}{}", err_str, error_hint(&err_str), "\n\nYou can also run me fully offline via a local Ollama model."),
        );
        let _ = ravenbot_db::queries::MessageQueries::insert(state.db.pool(), &error_msg).await;
        let _ = app.emit("agent-stream", serde_json::json!({ "kind": "done", "thread_id": thread_id.to_string() }));
        return Ok(error_msg);
    }

    let _ = app.emit("agent-stream", serde_json::json!({ "kind": "done", "thread_id": thread_id.to_string() }));

    let messages = ravenbot_db::queries::MessageQueries::list_by_thread(state.db.pool(), thread_id)
        .await
        .map_err(|e| e.to_string())?;
    messages.last()
        .cloned()
        .ok_or_else(|| "No response generated".to_string())
}

#[tauri::command]
async fn import_bot_bundle(state: State<'_, AppState>, bundle_json: String) -> Result<Uuid, String> {
    let manager = ravenbot_sync::bundle::BundleManager::open(state.db.pool().clone()).await?;
    manager.import_from_json(&bundle_json).await
}

#[tauri::command]
async fn import_bot_bundle_from_file(
    state: State<'_, AppState>,
    path: String,
) -> Result<Uuid, String> {
    let manager = ravenbot_sync::bundle::BundleManager::open(state.db.pool().clone()).await?;
    manager.import_from_file(&path).await
}

// ---- Routines (scheduler) commands ----

#[tauri::command]
async fn create_routine(
    state: State<'_, AppState>,
    bot_id: Uuid,
    name: String,
    schedule: String,
    _description: String,
    instruction: String,
) -> Result<ravenbot_core::Routine, String> {
    let manager = ravenbot_scheduler::routine::RoutineManager::new(state.db.pool().clone());
    manager.create(bot_id, &name, &schedule, &instruction).await
}

#[tauri::command]
async fn get_routine(state: State<'_, AppState>, routine_id: Uuid) -> Result<Option<ravenbot_core::Routine>, String> {
    let manager = ravenbot_scheduler::routine::RoutineManager::new(state.db.pool().clone());
    manager.get(routine_id).await
}

#[tauri::command]
async fn list_routines(state: State<'_, AppState>, bot_id: Uuid) -> Result<Vec<ravenbot_core::Routine>, String> {
    let manager = ravenbot_scheduler::routine::RoutineManager::new(state.db.pool().clone());
    manager.list_for_bot(bot_id).await
}

#[tauri::command]
async fn update_routine(
    state: State<'_, AppState>,
    routine: ravenbot_core::Routine,
) -> Result<(), String> {
    // Validate the schedule before persisting
    ravenbot_scheduler::cron::CronParser::parse(&routine.schedule)?;
    let manager = ravenbot_scheduler::routine::RoutineManager::new(state.db.pool().clone());
    manager.update(&routine).await
}

#[tauri::command]
async fn delete_routine(state: State<'_, AppState>, routine_id: Uuid) -> Result<(), String> {
    let manager = ravenbot_scheduler::routine::RoutineManager::new(state.db.pool().clone());
    manager.delete(routine_id).await
}

#[tauri::command]
async fn get_scheduler_status(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let running = state.scheduler.is_running().await;
    Ok(serde_json::json!({
        "running": running,
        "check_interval_secs": 60,
        "max_concurrent": 5
    }))
}

#[tauri::command]
async fn run_routine_now(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    routine_id: Uuid,
) -> Result<(), String> {
    let manager = ravenbot_scheduler::routine::RoutineManager::new(state.db.pool().clone());
    let routine = manager
        .get(routine_id)
        .await?
        .ok_or_else(|| "Routine not found".to_string())?;
    execute_routine_instruction(&state.db, &state.runtime, &app, &routine).await
}

// ---- Webhook triggers ----

/// Enable (or rotate) a routine's inbound webhook. Returns the URL + secret
/// **once** so the caller can copy them; the secret is never re-exposed.
#[tauri::command]
async fn enable_routine_webhook(
    state: State<'_, AppState>,
    routine_id: Uuid,
) -> Result<serde_json::Value, String> {
    // 32 hex chars from a v4 UUID pair — enough entropy for a bearer secret.
    let secret = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    ravenbot_db::queries::WebhookQueries::set(state.db.pool(), routine_id, Some(&secret), true)
        .await
        .map_err(|e| e.to_string())?;

    let base = state
        .webhook_base_url
        .read()
        .map(|u| u.clone())
        .unwrap_or_else(|_| "http://127.0.0.1:8800".to_string());
    Ok(serde_json::json!({
        "url": format!("{}/hooks/{}", base, secret),
        "secret": secret,
        "header": format!("Bearer {}", secret),
    }))
}

#[tauri::command]
async fn disable_routine_webhook(
    state: State<'_, AppState>,
    routine_id: Uuid,
) -> Result<(), String> {
    ravenbot_db::queries::WebhookQueries::set(state.db.pool(), routine_id, None, false)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn get_routine_webhook_status(
    state: State<'_, AppState>,
    routine_id: Uuid,
) -> Result<serde_json::Value, String> {
    let enabled = ravenbot_db::queries::WebhookQueries::secret(state.db.pool(), routine_id)
        .await
        .map_err(|e| e.to_string())?
        .is_some();
    let base = state
        .webhook_base_url
        .read()
        .map(|u| u.clone())
        .unwrap_or_else(|_| "http://127.0.0.1:8800".to_string());
    Ok(serde_json::json!({ "enabled": enabled, "base_url": base }))
}

// ---- Team packages (install a team from Markdown) ----

/// Parse a team Markdown file and return a review summary, without creating
/// anything. `source_url` is accepted so the UI can show where it came from.
#[tauri::command]
async fn preview_team(markdown: String) -> Result<serde_json::Value, String> {
    let team = TeamPackage::parse(&markdown).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "name": team.name,
        "description": team.description,
        "summary": team.summary(),
        "bots": team.bots.iter().map(|b| serde_json::json!({
            "name": b.name,
            "title": b.title,
            "description": b.description,
            "rank": b.rank,
            "model": b.model,
        })).collect::<Vec<_>>(),
        "office": team.office.as_ref().map(|o| serde_json::json!({
            "name": o.name,
            "template": o.template,
            "goal": o.goal,
        })),
        "routines": team.routines.iter().map(|r| serde_json::json!({
            "name": r.name,
            "bot": r.bot,
            "schedule": r.schedule,
        })).collect::<Vec<_>>(),
    }))
}

/// Fetch a team Markdown file from a URL (marketplace install) and preview it.
#[tauri::command]
async fn fetch_and_preview_team(url: String) -> Result<serde_json::Value, String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("URL must start with http:// or https://".to_string());
    }
    let body = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    preview_team(body).await
}

/// Create the team: bots, an optional office, and paused routines.
#[tauri::command]
async fn import_team(
    state: State<'_, AppState>,
    markdown: String,
) -> Result<serde_json::Value, String> {
    import_team_into(state.db.pool(), &markdown).await
}

/// Core team importer (testable without Tauri state): bots, optional office,
/// paused routines.
async fn import_team_into(
    pool: &sqlx::SqlitePool,
    markdown: &str,
) -> Result<serde_json::Value, String> {
    let team = TeamPackage::parse(markdown).map_err(|e| e.to_string())?;

    // 1. Create bots, remembering name → id for routines.
    let mut name_to_id: std::collections::HashMap<String, Uuid> = std::collections::HashMap::new();
    let mut created_bots = Vec::new();
    for spec in &team.bots {
        let mut bot = Bot::new(spec.name.clone(), spec.description.clone().unwrap_or_default());
        if let Some(title) = &spec.title {
            bot.specialty = Some(title.clone());
        }
        if let Some(rank) = &spec.rank {
            bot.rank = Some(rank.clone());
        }
        if let Some(prompt) = &spec.prompt {
            bot.config.custom_prompt = Some(prompt.clone());
        }
        if let Some(model) = &spec.model {
            // Accept "provider/model" or a bare provider.
            if let Some((provider, id)) = model.split_once('/') {
                bot.config.model_provider = provider.to_string();
                bot.config.model_id = id.to_string();
            } else {
                bot.config.model_provider = model.clone();
            }
        }
        if let Some(style) = &spec.avatar_style {
            bot.avatar_style = Some(style.clone());
        }
        ravenbot_db::queries::BotQueries::insert(pool, &bot)
            .await
            .map_err(|e| e.to_string())?;
        name_to_id.insert(spec.name.to_lowercase(), bot.id);
        created_bots.push(serde_json::json!({ "name": bot.name, "id": bot.id.to_string() }));
    }

    // 2. Optional office binding every bot.
    let mut office_id: Option<Uuid> = None;
    if let Some(office) = &team.office {
        let template = office.template.clone().unwrap_or_else(|| "custom".to_string());
        let mut room = ChatRoom::new(office.name.clone(), office.description.clone().unwrap_or_default(), template);
        room.goal = office.goal.clone();
        room.policy = office.policy.clone();
        ravenbot_db::queries::ChatRoomQueries::create(pool, &room)
            .await
            .map_err(|e| e.to_string())?;
        for bot_id in name_to_id.values() {
            let member = ChatRoomMember {
                chatroom_id: room.id,
                bot_id: *bot_id,
                rank: team
                    .bots
                    .iter()
                    .find(|b| name_to_id.get(&b.name.to_lowercase()) == Some(bot_id))
                    .and_then(|b| b.rank.clone())
                    .unwrap_or_else(|| "specialist".to_string()),
                specialty: team
                    .bots
                    .iter()
                    .find(|b| name_to_id.get(&b.name.to_lowercase()) == Some(bot_id))
                    .and_then(|b| b.title.clone())
                    .unwrap_or_default(),
                joined_at: chrono::Utc::now(),
            };
            ravenbot_db::queries::ChatRoomQueries::add_member(pool, &member)
                .await
                .map_err(|e| e.to_string())?;
        }
        office_id = Some(room.id);
    }

    // 3. Routines — created **disabled** so a freshly imported team never
    //    starts running work unattended (OpenMausBot parity).
    let manager = ravenbot_scheduler::routine::RoutineManager::new(pool.clone());
    let mut created_routines = Vec::new();
    for spec in &team.routines {
        let Some(bot_id) = name_to_id.get(&spec.bot.to_lowercase()).copied() else {
            continue;
        };
        let schedule = spec.schedule.clone().unwrap_or_else(|| "0 9 * * *".to_string());
        if let Ok(mut routine) = manager.create(bot_id, &spec.name, &schedule, &spec.instruction).await {
            routine.is_enabled = false;
            let _ = manager.update(&routine).await;
            created_routines.push(serde_json::json!({ "name": routine.name, "id": routine.id.to_string() }));
        }
    }

    Ok(serde_json::json!({
        "name": team.name,
        "bots": created_bots,
        "office_id": office_id.map(|u| u.to_string()),
        "routines": created_routines,
        "playbook": team.playbook,
    }))
}

// ---- Channels (named contexts) ----

#[tauri::command]
async fn list_channels(state: State<'_, AppState>) -> Result<Vec<serde_json::Value>, String> {
    let channels = ravenbot_db::queries::ChannelQueries::list(state.db.pool())
        .await
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for c in channels {
        let bots = ravenbot_db::queries::ChannelQueries::bots(state.db.pool(), c.id)
            .await
            .unwrap_or_default();
        out.push(serde_json::json!({
            "id": c.id.to_string(),
            "name": c.name,
            "description": c.description,
            "instructions": c.instructions,
            "working_folder": c.working_folder,
            "color": c.color,
            "position": c.position,
            "bot_ids": bots.iter().map(|b| b.to_string()).collect::<Vec<_>>(),
        }));
    }
    Ok(out)
}

#[tauri::command]
async fn create_channel(
    state: State<'_, AppState>,
    name: String,
    description: Option<String>,
    instructions: Option<String>,
    working_folder: Option<String>,
    color: Option<String>,
    bot_ids: Option<Vec<Uuid>>,
) -> Result<String, String> {
    let mut channel = Channel::new(name);
    channel.description = description.unwrap_or_default();
    channel.instructions = instructions.unwrap_or_default();
    channel.working_folder = working_folder;
    channel.color = color;
    ravenbot_db::queries::ChannelQueries::create(state.db.pool(), &channel)
        .await
        .map_err(|e| e.to_string())?;
    if let Some(bots) = bot_ids {
        ravenbot_db::queries::ChannelQueries::set_bots(state.db.pool(), channel.id, &bots)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(channel.id.to_string())
}

#[tauri::command]
async fn update_channel(state: State<'_, AppState>, channel: Channel) -> Result<(), String> {
    ravenbot_db::queries::ChannelQueries::update(state.db.pool(), &channel)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_channel(state: State<'_, AppState>, channel_id: Uuid) -> Result<(), String> {
    ravenbot_db::queries::ChannelQueries::delete(state.db.pool(), channel_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_channel_bots(
    state: State<'_, AppState>,
    channel_id: Uuid,
    bot_ids: Vec<Uuid>,
) -> Result<(), String> {
    ravenbot_db::queries::ChannelQueries::set_bots(state.db.pool(), channel_id, &bot_ids)
        .await
        .map_err(|e| e.to_string())
}

/// Resolve which bot should answer in a channel under `lead` rules.
async fn channel_lead_bot(
    pool: &sqlx::SqlitePool,
    channel_id: Uuid,
    rules: &serde_json::Value,
) -> Option<Uuid> {
    if let Some(id) = rules
        .get("lead_bot_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
    {
        return Some(id);
    }
    let bots = ravenbot_db::queries::ChannelQueries::bots(pool, channel_id)
        .await
        .ok()?;
    let mut first: Option<Uuid> = None;
    for id in bots {
        if first.is_none() {
            first = Some(id);
        }
        if let Ok(Some(bot)) = ravenbot_db::queries::BotQueries::get(pool, id).await {
            if bot.is_orchestrator {
                return Some(id);
            }
            let rank = bot.rank.unwrap_or_default().to_lowercase();
            if rank.contains("lead") || rank.contains("chief") || rank.contains("manager") {
                return Some(id);
            }
        }
    }
    first
}

/// Whether the message @mentions this bot by name (manual responder mode).
async fn channel_bot_mentioned(
    pool: &sqlx::SqlitePool,
    bot_id: Uuid,
    content: &str,
) -> bool {
    if let Ok(Some(bot)) = ravenbot_db::queries::BotQueries::get(pool, bot_id).await {
        let needle = format!("@{}", bot.name.to_lowercase());
        return content.to_lowercase().contains(&needle);
    }
    false
}

#[tauri::command]
async fn send_message(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    thread_id: Uuid,
    content: String,
    attachments: Option<Vec<AttachmentInput>>,
) -> Result<Message, String> {
    // Save user message (with any inline image attachments)
    let mut user_msg = Message::user(thread_id, &content);
    if let Some(atts) = attachments {
        user_msg.attachments = build_image_attachments(atts);
    }
    ravenbot_db::queries::MessageQueries::insert(state.db.pool(), &user_msg)
        .await
        .map_err(|e| e.to_string())?;

    // Get thread to find bot
    let thread = ravenbot_db::queries::ThreadQueries::get(state.db.pool(), thread_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Thread not found".to_string())?;

    // ── Channel responder gate ────────────────────────────────────────────
    // A thread filed under a channel obeys the channel's responder rules:
    // `all` (default) lets this bot answer, `lead` only lets the channel lead
    // answer, `manual` requires an @mention of this bot.
    let channel_id: Option<Uuid> = sqlx::query_scalar::<_, Option<String>>(
        "SELECT channel_id FROM threads WHERE id = ?",
    )
    .bind(thread_id.to_string())
    .fetch_optional(state.db.pool())
    .await
    .ok()
    .flatten()
    .flatten()
    .and_then(|s| Uuid::parse_str(&s).ok());
    if let Some(cid) = channel_id {
        if let Ok(Some(channel)) =
            ravenbot_db::queries::ChannelQueries::get(state.db.pool(), cid).await
        {
            if let Some(rules) = channel.responder_rules.as_ref() {
                let mode = rules
                    .get("mode")
                    .and_then(|v| v.as_str())
                    .unwrap_or("all");
                let allowed = match mode {
                    "lead" => channel_lead_bot(state.db.pool(), cid, rules)
                        .await
                        .map(|lead| lead == thread.bot_id)
                        .unwrap_or(true),
                    "manual" => channel_bot_mentioned(state.db.pool(), thread.bot_id, &content).await,
                    _ => true,
                };
                if !allowed {
                    let note = Message::assistant(
                        thread_id,
                        format!(
                            "_Channel “{}” is in **{}** responder mode, so this agent stayed silent._",
                            channel.name, mode
                        ),
                    );
                    let _ = ravenbot_db::queries::MessageQueries::insert(state.db.pool(), &note).await;
                    let _ = app.emit(
                        "agent-stream",
                        serde_json::json!({ "kind": "done", "thread_id": thread_id.to_string() }),
                    );
                    return Ok(note);
                }
            }
        }
    }

    // Create and execute run
    let mut run = Run::new(thread.bot_id, thread_id);
    ravenbot_db::queries::RunQueries::insert(state.db.pool(), &run)
        .await
        .map_err(|e| e.to_string())?;

    // Execute the run with live streaming to the UI
    let result = execute_run_with_stream(&state, &app, &mut run).await;

    // A resumable pause is a successful, incomplete exit: keep the run's
    // checkpoint alive for `resume_run` and don't emit `done`.
    if result.is_ok() && run.state == RunState::Paused {
        let _ = app.emit("agent-stream", serde_json::json!({
            "kind": "paused",
            "thread_id": thread_id.to_string(),
            "run_id": run.id.to_string(),
            "bot_id": run.bot_id.to_string(),
        }));
        let messages = ravenbot_db::queries::MessageQueries::list_by_thread(state.db.pool(), thread_id)
            .await
            .map_err(|e| e.to_string())?;
        return messages
            .last()
            .cloned()
            .ok_or_else(|| "Run paused".to_string());
    }

    if let Err(err) = result {
        let err_str = err.to_string();
        tracing::warn!("Run execution error: {}", err_str);

        let error_msg = Message::assistant(
            thread_id,
            format!("⚠️ **Model Error:** {}\n\n{}{}", err_str, error_hint(&err_str), "\n\nYou can also run me fully offline via a local Ollama model."),
        );
        let _ = ravenbot_db::queries::MessageQueries::insert(state.db.pool(), &error_msg).await;
        let _ = app.emit("agent-stream", serde_json::json!({ "kind": "done", "thread_id": thread_id.to_string() }));
        return Ok(error_msg);
    }

    let _ = app.emit("agent-stream", serde_json::json!({ "kind": "done", "thread_id": thread_id.to_string() }));

    // Return the last message (assistant response)
    let messages = ravenbot_db::queries::MessageQueries::list_by_thread(state.db.pool(), thread_id)
        .await
        .map_err(|e| e.to_string())?;

    messages.last()
        .cloned()
        .ok_or_else(|| "No response generated".to_string())
}

/// Execute a run with streaming wired to the UI for the given thread.
async fn execute_run_with_stream(
    state: &State<'_, AppState>,
    app: &tauri::AppHandle,
    run: &mut Run,
) -> Result<(), String> {
    // Let the UI track the active run so its Pause/Resume control can target
    // this run specifically (resumable checkpoint), not the global kill switch.
    let _ = app.emit("agent-stream", serde_json::json!({
        "kind": "run_started",
        "thread_id": run.thread_id.to_string(),
        "run_id": run.id.to_string(),
    }));
    state.runtime.set_thread_emitter(run.thread_id, Some(make_stream_emitter(app.clone())));
    let result = state.runtime.execute_run(run).await;
    state.runtime.set_thread_emitter(run.thread_id, None);
    result.map_err(|e| e.to_string())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CatalogModel {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_free: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_vision: Option<bool>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CatalogProvider {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub description: String,
    pub key_placeholder: String,
    pub key_url: String,
    pub key_env: String,
    pub supports_discovery: bool,
    pub keyless: bool,
    pub default_model: String,
    pub fallback_models: Vec<CatalogModel>,
}

/// Fetch complete model provider catalog
#[tauri::command]
async fn get_model_catalog() -> Result<Vec<CatalogProvider>, String> {
    let m = |id: &str, name: &str| CatalogModel {
        id: id.to_string(),
        name: name.to_string(),
        is_free: None,
        supports_vision: None,
    };
    let mv = |id: &str, name: &str, vision: bool| CatalogModel {
        id: id.to_string(),
        name: name.to_string(),
        is_free: None,
        supports_vision: Some(vision),
    };
    let mf = |id: &str, name: &str| CatalogModel {
        id: id.to_string(),
        name: name.to_string(),
        is_free: Some(true),
        supports_vision: None,
    };

    Ok(vec![
        // 1. Command Code
        CatalogProvider {
            id: "commandcode".to_string(),
            name: "Command Code".to_string(),
            icon: "⚡".to_string(),
            description: "OpenAI & Anthropic proxy, high-rate limits, free models".to_string(),
            key_placeholder: "cc_... or rc_...".to_string(),
            key_url: "https://commandcode.ai".to_string(),
            key_env: "COMMANDCODE_API_KEY".to_string(),
            supports_discovery: true,
            keyless: false,
            default_model: "claude-sonnet-4-6".to_string(),
            fallback_models: vec![
                mv("claude-sonnet-4-6", "Claude Sonnet 4 (CC)", true),
                m("deepseek/deepseek-v4-flash", "DeepSeek V4 Flash (CC)"),
                mf("longcat-2.0:free", "LongCat 2.0 (Free)"),
            ],
        },
        // 2. OpenCode
        CatalogProvider {
            id: "opencode".to_string(),
            name: "OpenCode".to_string(),
            icon: "💻".to_string(),
            description: "OpenCode Zen coding agent & multi-model LLM gateway".to_string(),
            key_placeholder: "sk-...".to_string(),
            key_url: "https://opencode.ai/auth".to_string(),
            key_env: "OPENCODE_API_KEY".to_string(),
            supports_discovery: true,
            keyless: false,
            default_model: "claude-sonnet-4-5".to_string(),
            fallback_models: vec![
                mv("claude-sonnet-4-5", "Claude Sonnet 4.5", true),
                mv("gpt-5", "GPT-5", true),
                m("deepseek-v4-pro", "DeepSeek V4 Pro"),
                m("gemini-3-flash", "Gemini 3 Flash"),
            ],
        },
        // 3. Anthropic Claude
        CatalogProvider {
            id: "anthropic".to_string(),
            name: "Anthropic Claude".to_string(),
            icon: "🧠".to_string(),
            description: "Claude 3.7 Sonnet (Thinking), Claude 3.5 Sonnet, Claude 3 Opus".to_string(),
            key_placeholder: "sk-ant-api03-...".to_string(),
            key_url: "https://console.anthropic.com".to_string(),
            key_env: "ANTHROPIC_API_KEY".to_string(),
            supports_discovery: true,
            keyless: false,
            default_model: "claude-3-7-sonnet-20250219".to_string(),
            fallback_models: vec![
                mv("claude-3-7-sonnet-20250219", "Claude 3.7 Sonnet (Hybrid Thinking)", true),
                mv("claude-3-5-sonnet-20241022", "Claude 3.5 Sonnet", true),
                mv("claude-3-5-haiku-20241022", "Claude 3.5 Haiku", true),
                mv("claude-3-opus-20240229", "Claude 3 Opus", true),
            ],
        },
        // 4. Cline
        CatalogProvider {
            id: "cline".to_string(),
            name: "Cline".to_string(),
            icon: "🤖".to_string(),
            description: "Autonomous coding agent gateway & multi-model routing".to_string(),
            key_placeholder: "sk-...".to_string(),
            key_url: "https://cline.bot".to_string(),
            key_env: "CLINE_API_KEY".to_string(),
            supports_discovery: true,
            keyless: false,
            default_model: "claude-sonnet-4-5".to_string(),
            fallback_models: vec![
                mv("claude-sonnet-4-5", "Claude Sonnet 4.5", true),
                mv("openai/gpt-4o", "GPT-4o", true),
                m("deepseek/deepseek-chat", "DeepSeek Chat"),
            ],
        },
        // 5. OpenRouter (9 router)
        CatalogProvider {
            id: "openrouter".to_string(),
            name: "OpenRouter".to_string(),
            icon: "🌐".to_string(),
            description: "300+ models, unified API, pay-as-you-go aggregator".to_string(),
            key_placeholder: "sk-or-v1-...".to_string(),
            key_url: "https://openrouter.ai".to_string(),
            key_env: "OPENROUTER_API_KEY".to_string(),
            supports_discovery: true,
            keyless: false,
            default_model: "anthropic/claude-3.5-sonnet".to_string(),
            fallback_models: vec![
                mv("anthropic/claude-3.5-sonnet", "Claude 3.5 Sonnet", true),
                mv("openai/gpt-4o", "GPT-4o", true),
                m("deepseek/deepseek-chat", "DeepSeek V3"),
                m("deepseek/deepseek-r1", "DeepSeek R1"),
                m("meta-llama/llama-3.3-70b-instruct", "Llama 3.3 70B"),
            ],
        },
        // 6. OpenAI
        CatalogProvider {
            id: "openai".to_string(),
            name: "OpenAI".to_string(),
            icon: "🟢".to_string(),
            description: "GPT-4o, GPT-4o mini, o1 reasoning & o3-mini".to_string(),
            key_placeholder: "sk-proj-...".to_string(),
            key_url: "https://platform.openai.com".to_string(),
            key_env: "OPENAI_API_KEY".to_string(),
            supports_discovery: true,
            keyless: false,
            default_model: "gpt-4o".to_string(),
            fallback_models: vec![
                mv("gpt-4o", "GPT-4o", true),
                mv("gpt-4o-mini", "GPT-4o Mini", true),
                m("o1", "o1 Reasoning"),
                m("o3-mini", "o3-mini"),
                mv("gpt-4-turbo", "GPT-4 Turbo", true),
            ],
        },
        // 7. Google Gemini
        CatalogProvider {
            id: "gemini".to_string(),
            name: "Google Gemini".to_string(),
            icon: "✨".to_string(),
            description: "Gemini 2.0 Flash, Gemini 2.5 Pro with 1M-2M context".to_string(),
            key_placeholder: "AIzaSy...".to_string(),
            key_url: "https://aistudio.google.com".to_string(),
            key_env: "GEMINI_API_KEY".to_string(),
            supports_discovery: true,
            keyless: false,
            default_model: "gemini-2.0-flash".to_string(),
            fallback_models: vec![
                mv("gemini-2.0-flash", "Gemini 2.0 Flash", true),
                mv("gemini-2.5-pro", "Gemini 2.5 Pro", true),
                mv("gemini-1.5-pro", "Gemini 1.5 Pro", true),
                mv("gemini-1.5-flash", "Gemini 1.5 Flash", true),
            ],
        },
        // 8. DeepSeek
        CatalogProvider {
            id: "deepseek".to_string(),
            name: "DeepSeek".to_string(),
            icon: "🐋".to_string(),
            description: "DeepSeek V3 671B MoE and DeepSeek R1 reasoning models".to_string(),
            key_placeholder: "sk-...".to_string(),
            key_url: "https://platform.deepseek.com".to_string(),
            key_env: "DEEPSEEK_API_KEY".to_string(),
            supports_discovery: true,
            keyless: false,
            default_model: "deepseek-chat".to_string(),
            fallback_models: vec![
                m("deepseek-chat", "DeepSeek V3"),
                m("deepseek-reasoner", "DeepSeek R1 Reasoning"),
            ],
        },
        // 9. Groq
        CatalogProvider {
            id: "groq".to_string(),
            name: "Groq LPU".to_string(),
            icon: "🚀".to_string(),
            description: "Ultra-fast low-latency inference on custom LPUs".to_string(),
            key_placeholder: "gsk_...".to_string(),
            key_url: "https://console.groq.com".to_string(),
            key_env: "GROQ_API_KEY".to_string(),
            supports_discovery: true,
            keyless: false,
            default_model: "llama-3.3-70b-versatile".to_string(),
            fallback_models: vec![
                m("llama-3.3-70b-versatile", "Llama 3.3 70B Versatile"),
                m("deepseek-r1-distill-llama-70b", "DeepSeek R1 Distill Llama 70B"),
                m("mixtral-8x7b-32768", "Mixtral 8x7B"),
            ],
        },
        // 10. xAI Grok
        CatalogProvider {
            id: "xai".to_string(),
            name: "xAI Grok".to_string(),
            icon: "🌌".to_string(),
            description: "Grok 2, Grok 2 Vision & Grok real-time frontier models".to_string(),
            key_placeholder: "xai-...".to_string(),
            key_url: "https://console.x.ai".to_string(),
            key_env: "XAI_API_KEY".to_string(),
            supports_discovery: true,
            keyless: false,
            default_model: "grok-2-latest".to_string(),
            fallback_models: vec![
                m("grok-2-latest", "Grok 2"),
                mv("grok-2-vision-1212", "Grok 2 Vision", true),
                m("grok-beta", "Grok Beta"),
            ],
        },
        // 11. Mistral AI
        CatalogProvider {
            id: "mistral".to_string(),
            name: "Mistral AI".to_string(),
            icon: "🌪️".to_string(),
            description: "Mistral Large, Mistral Small, Codestral & Pixtral".to_string(),
            key_placeholder: "...".to_string(),
            key_url: "https://console.mistral.ai".to_string(),
            key_env: "MISTRAL_API_KEY".to_string(),
            supports_discovery: true,
            keyless: false,
            default_model: "mistral-large-latest".to_string(),
            fallback_models: vec![
                m("mistral-large-latest", "Mistral Large"),
                m("mistral-small-latest", "Mistral Small"),
                m("codestral-latest", "Codestral"),
            ],
        },
        // 12. Together AI
        CatalogProvider {
            id: "together".to_string(),
            name: "Together AI".to_string(),
            icon: "🤝".to_string(),
            description: "Fast inference engine for leading open source LLMs".to_string(),
            key_placeholder: "...".to_string(),
            key_url: "https://api.together.xyz".to_string(),
            key_env: "TOGETHER_API_KEY".to_string(),
            supports_discovery: true,
            keyless: false,
            default_model: "meta-llama/Llama-3.3-70B-Instruct-Turbo".to_string(),
            fallback_models: vec![
                m("meta-llama/Llama-3.3-70B-Instruct-Turbo", "Llama 3.3 70B Turbo"),
                m("deepseek-ai/DeepSeek-R1", "DeepSeek R1 (Together)"),
                m("Qwen/Qwen2.5-72B-Instruct-Turbo", "Qwen 2.5 72B Turbo"),
            ],
        },
        // 13. Perplexity AI
        CatalogProvider {
            id: "perplexity".to_string(),
            name: "Perplexity AI".to_string(),
            icon: "🔍".to_string(),
            description: "Sonar search models with live internet citations & grounding".to_string(),
            key_placeholder: "pplx-...".to_string(),
            key_url: "https://perplexity.ai".to_string(),
            key_env: "PERPLEXITY_API_KEY".to_string(),
            supports_discovery: true,
            keyless: false,
            default_model: "llama-3.1-sonar-large-128k-online".to_string(),
            fallback_models: vec![
                m("llama-3.1-sonar-large-128k-online", "Sonar Large 128K"),
                m("llama-3.1-sonar-small-128k-online", "Sonar Small 128K"),
                m("sonar-reasoning", "Sonar Reasoning"),
            ],
        },
        // 14. Cohere
        CatalogProvider {
            id: "cohere".to_string(),
            name: "Cohere".to_string(),
            icon: "🌲".to_string(),
            description: "Command R+ & Command R enterprise multilingual & tool models".to_string(),
            key_placeholder: "...".to_string(),
            key_url: "https://cohere.com".to_string(),
            key_env: "COHERE_API_KEY".to_string(),
            supports_discovery: true,
            keyless: false,
            default_model: "command-r-plus".to_string(),
            fallback_models: vec![
                m("command-r-plus", "Command R+"),
                m("command-r", "Command R"),
            ],
        },
        // 15. Xiaomi MiMo
        CatalogProvider {
            id: "mimo".to_string(),
            name: "Xiaomi MiMo".to_string(),
            icon: "📱".to_string(),
            description: "High-efficiency multilingual reasoning models & edge AI".to_string(),
            key_placeholder: "...".to_string(),
            key_url: "https://api.mimo.mi.com".to_string(),
            key_env: "MIMO_API_KEY".to_string(),
            supports_discovery: true,
            keyless: false,
            default_model: "mimo-v2.5".to_string(),
            fallback_models: vec![
                m("mimo-v2.5", "MiMo V2.5"),
                m("mimo-v2.5-pro", "MiMo V2.5 Pro"),
            ],
        },
        // 16. Ollama
        CatalogProvider {
            id: "ollama".to_string(),
            name: "Ollama (Local)".to_string(),
            icon: "🟢".to_string(),
            description: "100% sovereign offline inference".to_string(),
            key_placeholder: "http://localhost:11434".to_string(),
            key_url: "https://ollama.com".to_string(),
            key_env: "OLLAMA_HOST".to_string(),
            supports_discovery: true,
            keyless: true,
            default_model: "llama3.1".to_string(),
            fallback_models: vec![
                m("llama3.1", "Llama 3.1"),
                m("llama3.1:8b", "Llama 3.1 8B"),
                m("mistral", "Mistral 7B"),
                m("deepseek-r1:8b", "DeepSeek R1 8B"),
            ],
        },
        // 17. TokenRouter
        CatalogProvider {
            id: "tokenrouter".to_string(),
            name: "TokenRouter".to_string(),
            icon: "🧭".to_string(),
            description: "Unified multi-model hub — OpenAI/Claude/Gemini-compatible with global routing & failover".to_string(),
            key_placeholder: "sk-...".to_string(),
            key_url: "https://www.tokenrouter.com/console/token".to_string(),
            key_env: "TOKENROUTER_API_KEY".to_string(),
            supports_discovery: true,
            keyless: false,
            default_model: "deepseek/deepseek-v4.1-flash".to_string(),
            fallback_models: vec![
                m("deepseek/deepseek-v4.1-flash", "DeepSeek V4.1 Flash"),
                m("z-ai/glm-5.3", "GLM 5.3"),
                m("qwen/qwen3.8-max", "Qwen 3.8 Max"),
                m("moonshotai/kimi-k2.7-code", "Kimi K2.7 Code"),
                mv("openai/gpt-5-mini", "GPT-5 Mini", true),
                m("x-ai/grok-4.6", "Grok 4.6"),
                mf("z-ai/glm-5.3-free", "GLM 5.3 (Free)"),
            ],
        },
    ])
}

/// Fetch live models for a provider using ModelDiscovery
#[tauri::command]
async fn fetch_provider_models(
    state: State<'_, AppState>,
    provider: String,
) -> Result<Vec<serde_json::Value>, String> {
    let (key, custom) = {
        let manager = state.runtime.provider_manager().lock().await;
        (
            manager.get_api_key(&provider).map(|s| s.to_string()),
            manager
                .custom_spec(&provider)
                .map(|s| (s.base_url.clone(), s.kind.clone())),
        )
    };
    let models = match custom {
        Some((base_url, kind)) => {
            state
                .model_discovery
                .fetch_models_for(&provider, key.as_deref(), &base_url, &kind)
                .await
        }
        None => {
            state
                .model_discovery
                .fetch_models(&provider, key.as_deref(), true)
                .await
        }
    }
    .map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(&models).map_err(|e| e.to_string())?.as_array().cloned().unwrap_or_default())
}

/// Fetch all provider models in parallel
#[tauri::command]
async fn fetch_all_provider_models(
    state: State<'_, AppState>,
) -> Result<std::collections::HashMap<String, Vec<ravenbot_models::DiscoveredModel>>, String> {
    let api_keys = {
        let manager = state.runtime.provider_manager().lock().await;
        manager.get_api_keys().clone()
    };
    let mut models = state.model_discovery.fetch_all_providers(&api_keys).await;

    // User-defined providers get discovered through their saved base URL/kind
    let customs = {
        let manager = state.runtime.provider_manager().lock().await;
        manager
            .custom_provider_ids()
            .into_iter()
            .filter_map(|id| {
                let spec = manager.custom_spec(&id)?;
                Some((id, spec.base_url.clone(), spec.kind.clone()))
            })
            .collect::<Vec<_>>()
    };
    for (id, base_url, kind) in customs {
        let key = api_keys.get(&id).cloned();
        if let Ok(list) = state
            .model_discovery
            .fetch_models_for(&id, key.as_deref(), &base_url, &kind)
            .await
        {
            if !list.is_empty() {
                models.insert(id, list);
            }
        }
    }
    Ok(models)
}

/// Set and persist an API key for a provider
#[tauri::command]
async fn set_provider_api_key(
    state: State<'_, AppState>,
    provider: String,
    api_key: String,
) -> Result<(), String> {
    let p = provider.trim().to_lowercase();
    let trimmed_key = api_key.trim();

    // Save to database
    ravenbot_db::queries::ProviderKeyQueries::set(state.db.pool(), &p, trimmed_key)
        .await
        .map_err(|e| e.to_string())?;

    // Also persist alias if claude/anthropic or grok/xai
    if p == "claude" {
        let _ = ravenbot_db::queries::ProviderKeyQueries::set(state.db.pool(), "anthropic", trimmed_key).await;
    } else if p == "anthropic" {
        let _ = ravenbot_db::queries::ProviderKeyQueries::set(state.db.pool(), "claude", trimmed_key).await;
    } else if p == "grok" {
        let _ = ravenbot_db::queries::ProviderKeyQueries::set(state.db.pool(), "xai", trimmed_key).await;
    } else if p == "xai" {
        let _ = ravenbot_db::queries::ProviderKeyQueries::set(state.db.pool(), "grok", trimmed_key).await;
    }

    // Update in runtime memory
    let mut manager = state.runtime.provider_manager().lock().await;
    if trimmed_key.is_empty() {
        manager.remove_api_key(&p);
        if p == "claude" { manager.remove_api_key("anthropic"); }
        if p == "anthropic" { manager.remove_api_key("claude"); }
        if p == "grok" { manager.remove_api_key("xai"); }
        if p == "xai" { manager.remove_api_key("grok"); }
    } else {
        manager.set_api_key(&p, trimmed_key.to_string());
        if p == "claude" { manager.set_api_key("anthropic", trimmed_key.to_string()); }
        if p == "anthropic" { manager.set_api_key("claude", trimmed_key.to_string()); }
        if p == "grok" { manager.set_api_key("xai", trimmed_key.to_string()); }
        if p == "xai" { manager.set_api_key("grok", trimmed_key.to_string()); }
    }

    tracing::info!("Provider API key updated for: {}", p);
    Ok(())
}

/// Backwards compatibility setter
#[tauri::command]
async fn set_api_key(
    state: State<'_, AppState>,
    provider: String,
    api_key: String,
) -> Result<(), String> {
    set_provider_api_key(state, provider, api_key).await
}

/// Backwards compatibility check
#[tauri::command]
async fn check_api_key(
    state: State<'_, AppState>,
    provider: String,
) -> Result<bool, String> {
    let manager = state.runtime.provider_manager().lock().await;
    Ok(manager.has_key(&provider))
}

/// Get list of configured provider IDs
#[tauri::command]
async fn get_configured_providers(
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let db_keys = ravenbot_db::queries::ProviderKeyQueries::list(state.db.pool())
        .await
        .map_err(|e| e.to_string())?;
    let mut configured: std::collections::HashSet<String> = db_keys.into_keys().collect();

    let mem_keys = {
        let manager = state.runtime.provider_manager().lock().await;
        manager.configured_providers()
    };
    for k in mem_keys {
        configured.insert(k);
    }

    // Ensure aliases stay in sync
    if configured.contains("anthropic") {
        configured.insert("claude".to_string());
    }
    if configured.contains("claude") {
        configured.insert("anthropic".to_string());
    }
    if configured.contains("xai") {
        configured.insert("grok".to_string());
    }
    if configured.contains("grok") {
        configured.insert("xai".to_string());
    }

    let mut result: Vec<String> = configured.into_iter().collect();
    result.sort();
    Ok(result)
}

/// Get Ollama base URL
#[tauri::command]
async fn get_ollama_url(
    state: State<'_, AppState>,
) -> Result<String, String> {
    if let Ok(Some(url)) = ravenbot_db::queries::AppSettingsQueries::get(state.db.pool(), "ollama_url").await {
        if !url.trim().is_empty() {
            return Ok(url);
        }
    }
    let manager = state.runtime.provider_manager().lock().await;
    if let Some(u) = manager.get_base_url("ollama") {
        return Ok(u.to_string());
    }
    if let Ok(u) = std::env::var("OLLAMA_HOST") {
        if !u.trim().is_empty() {
            return Ok(u.trim().to_string());
        }
    }
    if let Ok(u) = std::env::var("OLLAMA_URL") {
        if !u.trim().is_empty() {
            return Ok(u.trim().to_string());
        }
    }
    Ok("http://localhost:11434".to_string())
}

/// Set and persist Ollama base URL
#[tauri::command]
async fn set_ollama_url(
    state: State<'_, AppState>,
    url: String,
) -> Result<String, String> {
    let u = url.trim().trim_end_matches('/').to_string();
    ravenbot_db::queries::AppSettingsQueries::set(state.db.pool(), "ollama_url", &u)
        .await
        .map_err(|e| e.to_string())?;
    let mut manager = state.runtime.provider_manager().lock().await;
    manager.set_base_url("ollama", u.clone());
    Ok(u)
}

// ── Custom providers (P8) ────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Deserialize)]
pub struct CustomProviderInput {
    pub id: String,
    pub display_name: String,
    /// "openai" | "anthropic" | "ollama"
    pub kind: String,
    pub base_url: String,
    /// Write-only. None = leave the stored key untouched; Some("") = clear it.
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub default_model: String,
    #[serde(default = "default_true")]
    pub supports_tools: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, serde::Serialize)]
pub struct CustomProviderInfo {
    pub id: String,
    pub display_name: String,
    pub kind: String,
    pub base_url: String,
    pub default_model: String,
    pub supports_tools: bool,
    pub enabled: bool,
    pub has_key: bool,
}

/// Custom ids must be lowercase slugs and must never shadow a built-in.
fn validate_custom_provider_id(id: &str) -> Result<(), String> {
    let chars: Vec<char> = id.chars().collect();
    if chars.is_empty() || chars.len() > 32 {
        return Err("Provider id must be 1-32 characters".to_string());
    }
    if !chars[0].is_ascii_lowercase() {
        return Err("Provider id must start with a lowercase letter".to_string());
    }
    for c in &chars[1..] {
        if !(c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_' || *c == '-') {
            return Err("Provider id may only contain a-z, 0-9, '-' and '_'".to_string());
        }
    }
    if ravenbot_models::manager::is_reserved_provider_name(id) {
        return Err(format!("'{id}' is a built-in provider name and cannot be used as a custom id"));
    }
    Ok(())
}

fn validate_http_base_url(url: &str) -> Result<String, String> {
    let trimmed = url.trim().trim_end_matches('/').to_string();
    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        return Err("Base URL must start with http:// or https://".to_string());
    }
    Ok(trimmed)
}

/// List user-defined providers (never includes API keys)
#[tauri::command]
async fn list_custom_providers(
    state: State<'_, AppState>,
) -> Result<Vec<CustomProviderInfo>, String> {
    let rows = ravenbot_db::queries::CustomProviderQueries::list(state.db.pool())
        .await
        .map_err(|e| e.to_string())?;
    let manager = state.runtime.provider_manager().lock().await;
    Ok(rows
        .into_iter()
        .map(|(id, display_name, kind, base_url, default_model, supports_tools, enabled)| {
            CustomProviderInfo {
                has_key: manager.has_key(&id),
                id,
                display_name,
                kind,
                base_url,
                default_model,
                supports_tools,
                enabled,
            }
        })
        .collect())
}

/// Create or update a user-defined provider and hydrate it into the manager
#[tauri::command]
async fn upsert_custom_provider(
    state: State<'_, AppState>,
    provider: CustomProviderInput,
) -> Result<(), String> {
    let id = provider.id.trim().to_lowercase();
    validate_custom_provider_id(&id)?;
    let kind = provider.kind.trim().to_lowercase();
    if !matches!(kind.as_str(), "openai" | "anthropic" | "ollama") {
        return Err(format!("Unknown provider kind: {kind}"));
    }
    let base_url = validate_http_base_url(&provider.base_url)?;
    let display_name = provider.display_name.trim().to_string();
    if display_name.is_empty() {
        return Err("Display name is required".to_string());
    }
    let default_model = provider.default_model.trim().to_string();

    ravenbot_db::queries::CustomProviderQueries::upsert(
        state.db.pool(),
        &id,
        &display_name,
        &kind,
        &base_url,
        &default_model,
        provider.supports_tools,
        provider.enabled,
    )
    .await
    .map_err(|e| e.to_string())?;

    if let Some(key) = provider.api_key.as_deref() {
        ravenbot_db::queries::ProviderKeyQueries::set(state.db.pool(), &id, key)
            .await
            .map_err(|e| e.to_string())?;
        let mut manager = state.runtime.provider_manager().lock().await;
        let trimmed = key.trim();
        if trimmed.is_empty() {
            manager.remove_api_key(&id);
        } else {
            manager.set_api_key(&id, trimmed.to_string());
        }
    }

    {
        let mut manager = state.runtime.provider_manager().lock().await;
        if provider.enabled {
            manager.register_custom(ravenbot_models::manager::CustomProviderSpec {
                id: id.clone(),
                display_name,
                kind,
                base_url,
                default_model,
                supports_tools: provider.supports_tools,
            });
        } else {
            manager.remove_custom(&id);
        }
    }
    state.model_discovery.clear_cache(Some(&id)).await;
    tracing::info!("Custom provider upserted: {}", id);
    Ok(())
}

/// Delete a user-defined provider. Returns the number of bots that were using
/// it; refuses unless `force` when bots still reference it.
#[tauri::command]
async fn delete_custom_provider(
    state: State<'_, AppState>,
    id: String,
    force: Option<bool>,
) -> Result<usize, String> {
    let id = id.trim().to_lowercase();
    let in_use = ravenbot_db::queries::BotQueries::list(state.db.pool())
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|b| b.config.model_provider == id)
        .count();
    if in_use > 0 && !force.unwrap_or(false) {
        return Err(format!("Provider is used by {in_use} bot(s). Confirm to delete anyway."));
    }
    ravenbot_db::queries::CustomProviderQueries::delete(state.db.pool(), &id)
        .await
        .map_err(|e| e.to_string())?;
    let _ = ravenbot_db::queries::ProviderKeyQueries::delete(state.db.pool(), &id).await;
    {
        let mut manager = state.runtime.provider_manager().lock().await;
        manager.remove_custom(&id);
        manager.remove_api_key(&id);
    }
    state.model_discovery.clear_cache(Some(&id)).await;
    Ok(in_use)
}

/// One-token completion against the saved definition + key
#[tauri::command]
async fn test_custom_provider(
    state: State<'_, AppState>,
    id: String,
) -> Result<String, String> {
    let id = id.trim().to_lowercase();
    let provider = {
        let manager = state.runtime.provider_manager().lock().await;
        manager
            .create_provider_from_str(&id)
            .map_err(|e| e.to_string())?
    };
    let messages = vec![ravenbot_models::Message::text("user", "Reply with the single word: ok")];
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        provider.complete(&messages, &[], 0.0, 16),
    )
    .await
    .map_err(|_| "Request timed out after 30s".to_string())?
    .map_err(|e| e.to_string())?;
    Ok(response.content.unwrap_or_default())
}

/// Set (or clear, with an empty string) a built-in provider's base URL override
#[tauri::command]
async fn set_provider_base_url(
    state: State<'_, AppState>,
    provider: String,
    base_url: String,
) -> Result<(), String> {
    let p = provider.trim().to_lowercase();
    if p.is_empty() {
        return Err("Provider is required".to_string());
    }
    let trimmed = if base_url.trim().is_empty() {
        String::new()
    } else {
        validate_http_base_url(&base_url)?
    };
    ravenbot_db::queries::ProviderBaseUrlQueries::set(state.db.pool(), &p, &trimmed)
        .await
        .map_err(|e| e.to_string())?;
    {
        let mut manager = state.runtime.provider_manager().lock().await;
        manager.set_base_url(&p, trimmed);
    }
    state.model_discovery.clear_cache(Some(&p)).await;
    Ok(())
}

/// Current base URL override for a provider ("" = compiled-in default)
#[tauri::command]
async fn get_provider_base_url(
    state: State<'_, AppState>,
    provider: String,
) -> Result<String, String> {
    let p = provider.trim().to_lowercase();
    if let Some(u) = ravenbot_db::queries::ProviderBaseUrlQueries::get(state.db.pool(), &p)
        .await
        .map_err(|e| e.to_string())?
    {
        return Ok(u);
    }
    let manager = state.runtime.provider_manager().lock().await;
    Ok(manager.get_base_url(&p).unwrap_or_default().to_string())
}

/// Probe Ollama endpoint for installed models
#[tauri::command]
async fn probe_ollama(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let url = get_ollama_url(state.clone()).await?;
    let models = state
        .model_discovery
        .fetch_models("ollama", None, true)
        .await
        .map_err(|e| format!("Failed to connect to Ollama at {}: {}", url, e))?;

    Ok(serde_json::json!({
        "base_url": url,
        "count": models.len(),
        "models": models
    }))
}

/// Get global default model
#[tauri::command]
async fn get_default_model(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let db_p = ravenbot_db::queries::AppSettingsQueries::get(state.db.pool(), "default_provider")
        .await
        .ok()
        .flatten();
    let db_m = ravenbot_db::queries::AppSettingsQueries::get(state.db.pool(), "default_model")
        .await
        .ok()
        .flatten();

    if let (Some(p), Some(m)) = (db_p, db_m) {
        if !p.is_empty() && !m.is_empty() {
            return Ok(serde_json::json!({ "provider": p, "model": m, "source": "settings" }));
        }
    }

    let env_p = std::env::var("RAVENBOT_DEFAULT_PROVIDER").ok().filter(|s| !s.trim().is_empty());
    let env_m = std::env::var("RAVENBOT_DEFAULT_MODEL").ok().filter(|s| !s.trim().is_empty());
    if let Some(p) = env_p {
        let m = env_m.unwrap_or_else(|| match p.as_str() {
            "openrouter" => "anthropic/claude-3.5-sonnet".to_string(),
            "anthropic" => "claude-3-7-sonnet-20250219".to_string(),
            "openai" => "gpt-4o".to_string(),
            _ => "llama3.1".to_string(),
        });
        return Ok(serde_json::json!({ "provider": p, "model": m, "source": "env" }));
    }

    Ok(serde_json::json!({ "provider": "ollama", "model": "llama3.1", "source": "default" }))
}

/// Set global default model
#[tauri::command]
async fn set_default_model(
    state: State<'_, AppState>,
    provider: String,
    model: String,
) -> Result<(), String> {
    ravenbot_db::queries::AppSettingsQueries::set(state.db.pool(), "default_provider", provider.trim())
        .await
        .map_err(|e| e.to_string())?;
    ravenbot_db::queries::AppSettingsQueries::set(state.db.pool(), "default_model", model.trim())
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Set approval mode for a bot
#[tauri::command]
async fn set_approval_mode(
    state: State<'_, AppState>,
    bot_id: Uuid,
    mode: String,
) -> Result<String, String> {
    let mode_val = mode.trim().to_lowercase();
    sqlx::query("UPDATE bots SET approval_mode = ?, updated_at = datetime('now') WHERE id = ?")
        .bind(&mode_val)
        .bind(bot_id.to_string())
        .execute(state.db.pool())
        .await
        .map_err(|e| e.to_string())?;
    Ok(mode_val)
}

/// Get approval mode for a bot
#[tauri::command]
async fn get_approval_mode(
    state: State<'_, AppState>,
    bot_id: Uuid,
) -> Result<String, String> {
    let row = sqlx::query_as::<_, (Option<String>,)>(
        "SELECT approval_mode FROM bots WHERE id = ?"
    )
    .bind(bot_id.to_string())
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| e.to_string())?;

    Ok(row.and_then(|r| r.0).unwrap_or_else(|| "ask".to_string()))
}

/// Pending approval requests for a thread (inline Allow/Deny cards).
#[tauri::command]
async fn list_pending_approvals(
    state: State<'_, AppState>,
    thread_id: Uuid,
) -> Result<Vec<ApprovalRequest>, String> {
    ravenbot_db::queries::ApprovalQueries::list_pending_for_thread(state.db.pool(), thread_id)
        .await
        .map_err(|e| e.to_string())
}

/// Pending approvals anywhere under one bot.
///
/// A run does not stay on the thread the user has open: a delegated child runs
/// on its own thread and parks its approvals there. Scoping to the open thread
/// — which is all the caller could ask for — meant that after a reload nothing
/// could find those approvals, so the run waited for a decision no screen could
/// show or make. From outside it looked like the app had stopped working
/// mid-run, with no error and no spinner to explain it.
#[tauri::command]
async fn list_pending_approvals_for_bot(
    state: State<'_, AppState>,
    bot_id: Uuid,
) -> Result<Vec<ApprovalRequest>, String> {
    ravenbot_db::queries::ApprovalQueries::list_pending_for_bot(state.db.pool(), bot_id)
        .await
        .map_err(|e| e.to_string())
}

/// Decide a parked approval (allow/deny). Returns true if it was still pending.
#[tauri::command]
async fn decide_approval(
    state: State<'_, AppState>,
    approval_id: Uuid,
    allowed: bool,
    note: Option<String>,
) -> Result<bool, String> {
    let decided =
        ravenbot_db::queries::ApprovalQueries::decide(state.db.pool(), approval_id, allowed, note.as_deref())
            .await
            .map_err(|e| e.to_string())?;
    Ok(decided)
}

/// A bot's live todo list (the runtime-native `todo` tool). Surfaced on the
/// office board so each teammate's self-tracked checklist is visible.
#[tauri::command]
async fn list_bot_todos(
    state: State<'_, AppState>,
    bot_id: Uuid,
) -> Result<Vec<serde_json::Value>, String> {
    let rows: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT id, task, done FROM bot_todos WHERE bot_id = ? ORDER BY created_at ASC LIMIT 60",
    )
    .bind(bot_id.to_string())
    .fetch_all(state.db.pool())
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|(id, task, done)| serde_json::json!({ "id": id, "task": task, "done": done != 0 }))
        .collect())
}

/// Persist the office board snapshot for a room thread so the kanban/DAG
/// survives room re-opens and app restarts. Stored as one JSON value in
/// `app_settings` keyed by thread (rooms are few; upsert keeps one row each).
#[tauri::command]
async fn save_office_board(
    state: State<'_, AppState>,
    thread_id: Uuid,
    goal: String,
    nodes: serde_json::Value,
) -> Result<(), String> {
    if !nodes.is_array() {
        return Err("nodes must be an array".to_string());
    }
    let payload = serde_json::json!({ "goal": goal, "nodes": nodes });
    let encoded = serde_json::to_string(&payload).map_err(|e| e.to_string())?;
    if encoded.len() > 64 * 1024 {
        return Err("board snapshot too large".to_string());
    }
    ravenbot_db::queries::AppSettingsQueries::set(
        state.db.pool(),
        &format!("office_board:{}", thread_id),
        &encoded,
    )
    .await
    .map_err(|e| e.to_string())
}

/// Last persisted office board for a room thread, if any. The payload gains
/// `liveNodeThreads`: node threads whose latest run is still non-terminal,
/// so the UI can tell a genuinely-working card from one orphaned by a restart.
#[tauri::command]
async fn get_office_board(
    state: State<'_, AppState>,
    thread_id: Uuid,
) -> Result<Option<serde_json::Value>, String> {
    let raw = ravenbot_db::queries::AppSettingsQueries::get(
        state.db.pool(),
        &format!("office_board:{}", thread_id),
    )
    .await
    .map_err(|e| e.to_string())?;
    let Some(raw) = raw else { return Ok(None) };
    let mut payload: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    let mut node_threads: Vec<String> = Vec::new();
    if let Some(nodes) = payload.get("nodes").and_then(|v| v.as_array()) {
        for n in nodes {
            if let Some(s) = n.get("nodeThreadId").and_then(|v| v.as_str()) {
                if Uuid::parse_str(s).is_ok() {
                    node_threads.push(s.to_string());
                }
            }
        }
    }
    let live: Vec<String> = if node_threads.is_empty() {
        Vec::new()
    } else {
        let sql = format!(
            "SELECT DISTINCT thread_id FROM runs WHERE thread_id IN ({}) \
             AND state NOT IN ('completed','failed','cancelled')",
            vec!["?"; node_threads.len()].join(",")
        );
        let mut q = sqlx::query_as::<_, (String,)>(&sql);
        for id in &node_threads {
            q = q.bind(id);
        }
        q.fetch_all(state.db.pool())
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|(t,)| t)
            .collect()
    };
    if let Some(obj) = payload.as_object_mut() {
        obj.insert("liveNodeThreads".to_string(), serde_json::json!(live));
    }
    Ok(Some(payload))
}

/// Pending `ask_user` questions for a thread (inline answer cards).
#[tauri::command]
async fn list_pending_questions(
    state: State<'_, AppState>,
    thread_id: Uuid,
) -> Result<Vec<QuestionRequest>, String> {
    ravenbot_db::queries::QuestionQueries::list_pending_for_thread(state.db.pool(), thread_id)
        .await
        .map_err(|e| e.to_string())
}

/// Pending questions anywhere under one bot — the same argument as
/// `list_pending_approvals_for_bot`: a parked `ask_user` on a child thread is
/// invisible to a thread-scoped lookup, and an invisible question never gets
/// answered, so the run never resumes.
#[tauri::command]
async fn list_pending_questions_for_bot(
    state: State<'_, AppState>,
    bot_id: Uuid,
) -> Result<Vec<QuestionRequest>, String> {
    ravenbot_db::queries::QuestionQueries::list_pending_for_bot(state.db.pool(), bot_id)
        .await
        .map_err(|e| e.to_string())
}

/// Answer a parked question. Returns true if it was still pending.
#[tauri::command]
async fn answer_question(
    state: State<'_, AppState>,
    question_id: Uuid,
    answer: String,
) -> Result<bool, String> {
    ravenbot_db::queries::QuestionQueries::answer(state.db.pool(), question_id, &answer)
        .await
        .map_err(|e| e.to_string())
}

/// Cooperatively cancel a running run (stops at the next step boundary).
#[tauri::command]
async fn cancel_run(state: State<'_, AppState>, run_id: Uuid) -> Result<(), String> {
    state.runtime.request_cancel(run_id);
    Ok(())
}

/// Request a resumable pause at the run's next tool-round boundary.
#[tauri::command]
async fn pause_run(state: State<'_, AppState>, run_id: Uuid) -> Result<(), String> {
    state.runtime.request_pause(run_id);
    Ok(())
}

/// Resume a paused run from its checkpoint, streaming as it re-enters the loop.
#[tauri::command]
async fn resume_run(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    run_id: Uuid,
) -> Result<Message, String> {
    let mut run = ravenbot_db::queries::RunQueries::get(state.db.pool(), run_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Run not found".to_string())?;
    if run.state != RunState::Paused {
        return Err("Run is not paused".to_string());
    }
    execute_run_with_stream(&state, &app, &mut run).await?;
    let _ = app.emit("agent-stream", serde_json::json!({ "kind": "done", "thread_id": run.thread_id.to_string() }));
    let messages = ravenbot_db::queries::MessageQueries::list_by_thread(state.db.pool(), run.thread_id)
        .await
        .map_err(|e| e.to_string())?;
    messages
        .last()
        .cloned()
        .ok_or_else(|| "No response generated".to_string())
}

/// A captured screen frame for the desktop panel.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScreenCapture {
    pub width: u32,
    pub height: u32,
    pub data_url: String,
    pub timestamp: String,
}

/// Capture the current screen (for the desktop panel / live preview).
#[tauri::command]
async fn capture_screen() -> Result<ScreenCapture, String> {
    let capture = ravenbot_vision::ScreenshotCapture::new();
    let shot = capture.capture().await.map_err(|e| e.to_string())?;
    Ok(ScreenCapture {
        width: shot.width,
        height: shot.height,
        data_url: shot.to_data_url(),
        timestamp: shot.timestamp.to_rfc3339(),
    })
}

/// Whether real desktop input injection is available, and via which tools.
#[derive(Debug, Clone, serde::Serialize)]
pub struct InputBackendInfo {
    pub available: bool,
    pub summary: String,
}

#[tauri::command]
async fn get_input_backend() -> Result<InputBackendInfo, String> {
    let injector = ravenbot_vision::InputInjector::detect();
    Ok(InputBackendInfo {
        available: injector.is_available(),
        summary: injector.backend_summary(),
    })
}

/// Transcribe recorded audio (base64) to text using the real STT engine
/// (OpenAI Whisper API, or local faster-whisper). Honest errors when neither
/// is configured — this is what powers microphone input on Linux + macOS,
/// where the browser Web Speech API is not available.
#[tauri::command]
async fn transcribe_audio(
    audio_base64: String,
    format: Option<String>,
) -> Result<serde_json::Value, String> {
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(audio_base64.trim())
        .map_err(|e| format!("Invalid base64 audio: {}", e))?;
    let fmt = format
        .as_deref()
        .map(str::trim)
        .filter(|f| !f.is_empty())
        .unwrap_or("webm");
    let result = ravenbot_vision::AudioTranscriber::new()
        .transcribe(&bytes, fmt)
        .await
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "text": result.text,
        "language": result.language,
        "confidence": result.confidence,
        "duration_secs": result.duration_secs,
    }))
}

/// Synthesize text to speech and return base64 WAV for the UI to play. Uses
/// the bot's configured TTS engine (HF Inference API or the local model).
#[tauri::command]
async fn synthesize_speech(
    state: State<'_, AppState>,
    text: String,
    voice: Option<String>,
    rate: Option<f32>,
) -> Result<serde_json::Value, String> {
    use base64::Engine as _;
    if text.trim().is_empty() {
        return Err("Nothing to speak".to_string());
    }
    let options = ravenbot_vision::audio::tts::VoiceOptions {
        voice: voice
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .unwrap_or("default")
            .to_string(),
        rate: rate.filter(|r| *r > 0.0).unwrap_or(1.0),
        ..Default::default()
    };
    // Prefer the owner's saved OpenAI key (multi-voice engine) over the env.
    let openai_key = ravenbot_db::queries::ProviderKeyQueries::get(state.db.pool(), "openai")
        .await
        .ok()
        .flatten()
        .or_else(|| std::env::var("OPENAI_API_KEY").ok())
        .filter(|k| !k.trim().is_empty());
    let audio = ravenbot_vision::TextToSpeech::new()
        .with_voice(options)
        .with_openai_key(openai_key)
        .synthesize(&text)
        .await
        .map_err(|e| e.to_string())?;
    let audio_b64 = base64::engine::general_purpose::STANDARD.encode(&audio);
    Ok(serde_json::json!({
        "audio_b64": audio_b64,
        "format": "wav",
    }))
}

/// List the text-to-speech voices available with the current configuration.
/// Only real engine voices are returned — `multi_voice` is false when the
/// active engine has a single voice.
#[tauri::command]
async fn list_tts_voices(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let has_openai = ravenbot_db::queries::ProviderKeyQueries::get(state.db.pool(), "openai")
        .await
        .ok()
        .flatten()
        .or_else(|| std::env::var("OPENAI_API_KEY").ok())
        .map(|k| !k.trim().is_empty())
        .unwrap_or(false);
    let has_hf = ["HUGGINGFACE_INFERENCE_TOKEN", "HF_TOKEN"]
        .iter()
        .any(|k| std::env::var(k).map(|v| !v.trim().is_empty()).unwrap_or(false));

    let mut voices = vec![serde_json::json!({
        "id": "default",
        "name": "Engine default",
        "provider": "auto"
    })];
    if has_openai {
        for v in ravenbot_vision::audio::tts::OPENAI_VOICES {
            let mut name = v.to_string();
            if let Some(first) = name.get_mut(0..1) {
                first.make_ascii_uppercase();
            }
            voices.push(serde_json::json!({ "id": v, "name": name, "provider": "openai" }));
        }
    }

    Ok(serde_json::json!({
        "engine": if has_openai { "openai" } else if has_hf { "huggingface" } else { "local" },
        "multi_voice": has_openai,
        "voices": voices,
    }))
}

/// Report what this machine can do for the Computer panel: screen preview,
/// host control (with the platform safety policy), and Docker desktops.
#[tauri::command]
async fn computer_capabilities() -> Result<serde_json::Value, String> {
    let injector = ravenbot_vision::InputInjector::detect();
    let session = if cfg!(target_os = "linux") {
        std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "unknown".to_string())
    } else if cfg!(target_os = "macos") {
        "aqua".to_string()
    } else {
        std::env::consts::OS.to_string()
    };
    let wayland = cfg!(target_os = "linux")
        && (session.eq_ignore_ascii_case("wayland")
            || std::env::var("WAYLAND_DISPLAY")
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false));
    let allow_wayland = std::env::var("RAVENBOT_ALLOW_WAYLAND_CONTROL")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let host_control_policy = if wayland {
        if allow_wayland { "opt_in_required" } else { "blocked" }
    } else {
        "opt_in_required"
    };

    let docker_version = ravenbot_sandbox::desktop::docker_version().await;
    let image = ravenbot_sandbox::desktop::desktop_image();
    let image_present = if docker_version.is_some() {
        ravenbot_sandbox::desktop::image_present(&image).await
    } else {
        false
    };

    Ok(serde_json::json!({
        "os": std::env::consts::OS,
        "session": session,
        "preview": true,
        "host_control": injector.is_available(),
        "host_control_backend": injector.backend_summary(),
        "host_control_policy": host_control_policy,
        "docker": {
            "available": docker_version.is_some(),
            "version": docker_version,
            "image": image,
            "image_present": image_present,
        },
    }))
}

/// Status of a bot's isolated Docker desktop (None when never created).
#[tauri::command]
async fn bot_desktop_status(bot_id: Uuid) -> Result<serde_json::Value, String> {
    let session = ravenbot_sandbox::desktop::status(&bot_id).await?;
    Ok(serde_json::to_value(session).unwrap_or(serde_json::Value::Null))
}

/// Start (or reuse) a bot's isolated Docker desktop.
#[tauri::command]
async fn start_bot_desktop(bot_id: Uuid) -> Result<serde_json::Value, String> {
    let session = ravenbot_sandbox::desktop::start(&bot_id, 2.0, 2048).await?;
    Ok(serde_json::to_value(session).unwrap_or(serde_json::Value::Null))
}

/// Stop and remove a bot's isolated desktop (workspace volume is kept).
#[tauri::command]
async fn stop_bot_desktop(bot_id: Uuid) -> Result<(), String> {
    ravenbot_sandbox::desktop::stop(&bot_id).await
}

/// Report the effective command-isolation backend for a sandbox tier.
#[tauri::command]
async fn get_sandbox_report(tier: Option<String>) -> Result<ravenbot_sandbox::SandboxReport, String> {
    let tier = match tier.as_deref().map(str::trim).map(str::to_lowercase).as_deref() {
        Some("host") => SandboxTier::Host,
        Some("docker") => SandboxTier::Docker,
        _ => SandboxTier::OsLevel,
    };
    Ok(ravenbot_sandbox::SandboxRunner::from_tier(tier).report())
}

/// List what is inside a workspace, so a person can see the agents' work.
///
/// The confinement on this side is on the way *in*: `resolve_path` plus
/// `confined()`. Nothing in the app showed the result of it, so a user set a
/// folder, watched an agent run, and had no way to find out whether it produced
/// anything. This is a read of directories the user already pointed us at, and
/// it is bounded — depth, entry count, no symlink following, hidden files
/// opt-in — and says so when a bound bites rather than returning a short tree
/// that reads as complete.
#[tauri::command]
async fn browse_workspace(path: String, show_hidden: Option<bool>) -> Result<ravenbot_runtime::tree::Tree, String> {
    let root = ravenbot_core::expand_home(path.trim());
    if root.as_os_str().is_empty() {
        return Err("No folder given".to_string());
    }
    // Cheap and synchronous, but a big tree is real I/O, so it must not sit on
    // the async runtime's core threads.
    tauri::async_runtime::spawn_blocking(move || {
        ravenbot_runtime::tree::list(&root, show_hidden.unwrap_or(false))
    })
    .await
    .map_err(|e| e.to_string())
}

/// A one-line description of a workspace, for a header.
#[tauri::command]
async fn summarize_workspace(path: String) -> Result<String, String> {
    let root = ravenbot_core::expand_home(path.trim());
    Ok(ravenbot_runtime::tree::summarize(&root))
}

/// The folder an agent works in when nothing else is set.
///
/// Without this, "inherits the office workspace" is not a thing the user can
/// look at — the default case has no path to show them, so the one workspace
/// that is created automatically is the one workspace they can never inspect.
#[tauri::command]
async fn default_workspace_for(name: String) -> Result<String, String> {
    Ok(ravenbot_core::office_workspace(&name).to_string_lossy().to_string())
}

/// Reveal a workspace in the system file manager.
///
/// The browser is for looking; this is for the case where the user wants the
/// folder in their own tools, which is the natural thing to want once they can
/// see that the agents are working in it.
#[tauri::command]
async fn open_workspace_in_file_manager(
    app: tauri::AppHandle,
    path: String,
) -> Result<(), String> {
    let root = ravenbot_core::expand_home(path.trim());
    if !root.is_dir() {
        return Err(format!("{} is not a folder", root.display()));
    }
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .reveal_item_in_dir(&root)
        .map_err(|e| format!("Could not open the file manager: {e}"))
}

/// Detect installed agent engines (Claude Code, Codex, configured ACP agents).
#[tauri::command]
async fn list_engines() -> Result<Vec<ravenbot_engines::EngineInfo>, String> {
    Ok(ravenbot_engines::list_engines().await)
}

/// Set which engine a bot runs on (`"native"`, `"claude"`, `"codex"`, …).
#[tauri::command]
async fn set_bot_engine(
    state: State<'_, AppState>,
    bot_id: Uuid,
    engine: String,
) -> Result<String, String> {
    let mut bot = ravenbot_db::queries::BotQueries::get(state.db.pool(), bot_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Bot not found".to_string())?;
    bot.config.engine = engine.trim().to_string();
    ravenbot_db::queries::BotQueries::update(state.db.pool(), &bot)
        .await
        .map_err(|e| e.to_string())?;
    Ok(bot.config.engine)
}

/// Auth → configure key; rate limit → wait; network → check connectivity;
/// everything else → how to fix the config.
fn error_hint(err: &str) -> &'static str {
    let e = err.to_lowercase();
    if e.contains("free tier") && e.contains("opencode") {
        "Fix: OpenCode's free tier only works inside the OpenCode CLI. Set this agent's Execution Engine to **OpenCode** (uses your installed CLI), or pick a paid model/provider."
    } else if e.contains("missing session") || e.contains("missingsessionid") {
        "Fix: this provider only allows this model inside its own client. Switch the agent's Execution Engine to that CLI, or pick another model."
    } else if e.contains("api key") || e.contains("unauthorized") || e.contains("401") || e.contains("403") {
        "Fix: configure the API key for this provider in **Settings (⌘,) → API Keys**."
    } else if e.contains("rate limited") || e.contains("429") {
        "Fix: the provider is rate-limiting you — wait a moment and resend (RAVENBOT already retried once)."
    } else if e.contains("internal server error") || e.contains("500") || e.contains("502") || e.contains("503") {
        "Fix: the provider returned a server error (their side). Try again, or pick a different model/provider for this agent."
    } else if e.contains("timed out") || e.contains("connection") || e.contains("sending request") {
        "Fix: check your network connection, or switch the bot to a local Ollama model in its settings."
    } else if e.contains("unknown provider") {
        "Fix: pick a valid model provider in the bot settings (openrouter, anthropic, openai, ollama)."
    } else if e.contains("no local model") || e.contains("local inference") {
        "Fix: set a local model path in **Settings (⌘,) → Local Models**, or pick a hosted provider for this bot."
    } else {
        "Fix: review the bot's provider/model settings, then resend."
    }
}

/// Re-run the last user message: removes the trailing assistant message
/// (if any) and executes a fresh run without duplicating the user turn.
#[tauri::command]
async fn regenerate_message(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    thread_id: Uuid,
) -> Result<Message, String> {
    let messages = ravenbot_db::queries::MessageQueries::list_by_thread(state.db.pool(), thread_id)
        .await
        .map_err(|e| e.to_string())?;

    let _last_user = messages
        .iter()
        .rev()
        .find(|m| matches!(m.role, ravenbot_core::MessageRole::User))
        .cloned()
        .ok_or_else(|| "No user message to regenerate from".to_string())?;

    // Remove the trailing assistant message(s) so the run produces a fresh one
    for msg in messages.iter().rev() {
        if matches!(msg.role, ravenbot_core::MessageRole::Assistant) {
            ravenbot_db::queries::MessageQueries::delete(state.db.pool(), msg.id)
                .await
                .map_err(|e| e.to_string())?;
        } else {
            break;
        }
    }

    let thread = ravenbot_db::queries::ThreadQueries::get(state.db.pool(), thread_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Thread not found".to_string())?;

    let mut run = Run::new(thread.bot_id, thread_id);
    ravenbot_db::queries::RunQueries::insert(state.db.pool(), &run)
        .await
        .map_err(|e| e.to_string())?;

    if let Err(err) = execute_run_with_stream(&state, &app, &mut run).await {
        tracing::warn!("Regenerate run error: {}", err);
        let error_msg = Message::assistant(
            thread_id,
            format!("⚠️ **Model Error:** {}\n\n{}{}", err, error_hint(&err), "\n\nYou can also run me fully offline via a local Ollama model."),
        );
        let _ = ravenbot_db::queries::MessageQueries::insert(state.db.pool(), &error_msg).await;
        let _ = app.emit("agent-stream", serde_json::json!({ "kind": "done", "thread_id": thread_id.to_string() }));
        return Ok(error_msg);
    }

    let _ = app.emit("agent-stream", serde_json::json!({ "kind": "done", "thread_id": thread_id.to_string() }));

    let messages = ravenbot_db::queries::MessageQueries::list_by_thread(state.db.pool(), thread_id)
        .await
        .map_err(|e| e.to_string())?;
    messages.last().cloned().ok_or_else(|| "No response generated".to_string())
}

#[tauri::command]
async fn execute_graph(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    orchestrator_bot_id: Uuid,
    goal: String,
    tasks: Vec<GraphTask>,
    chatroom_id: Option<Uuid>,
) -> Result<GraphResult, String> {
    // When the plan belongs to a room, resolve its roster/thread so the run is
    // posted as a genuine, attributed office conversation. A standalone graph
    // (no chatroom) keeps the old headless behaviour.
    let roster = match chatroom_id {
        Some(cid) => Some(load_office_roster(state.db.pool(), cid).await?),
        None => None,
    };
    let thread_id = match (&roster, chatroom_id) {
        (Some(r), Some(cid)) => {
            Some(ensure_chatroom_thread(state.db.pool(), cid, r.bots[0].1.id).await?)
        }
        _ => None,
    };

    // The lead is the explicitly requested orchestrator when it is a member of
    // this room, otherwise the room's resolved lead.
    let (lead_id, lead_name) = match &roster {
        Some(r) => {
            if orchestrator_bot_id != Uuid::nil()
                && r.bots.iter().any(|(_, b)| b.id == orchestrator_bot_id)
            {
                (orchestrator_bot_id, roster_name(r, orchestrator_bot_id))
            } else {
                (r.lead_id, r.lead_name.clone())
            }
        }
        None => (orchestrator_bot_id, "Team Lead".to_string()),
    };

    // Record the dispatched plan as the client's directive, and let the lead
    // brief the team in the room.
    if let (Some(r), Some(tid)) = (&roster, thread_id) {
        let directive = Message::user(tid, format!("🎯 **Plan Mode** — goal: {}", goal.trim()));
        let _ = ravenbot_db::queries::MessageQueries::insert(state.db.pool(), &directive).await;
        let items: Vec<(String, String, Vec<usize>)> = tasks
            .iter()
            .map(|t| (roster_name(r, t.bot_id), briefing_label(&t.instruction), t.depends_on.clone()))
            .collect();
        if !items.is_empty() {
            let _ = post_bot_message(
                state.db.pool(),
                tid,
                lead_id,
                lead_name.clone(),
                briefing_markdown(&items),
            )
            .await;
        }
    }

    // Create the task graph
    let mut graph = TaskGraph::new(&goal);
    let mut node_ids: Vec<Uuid> = Vec::new();
    for task in &tasks {
        let node_id = graph.add_node(task.bot_id, &task.instruction);
        node_ids.push(node_id);
        for &dep_idx in &task.depends_on {
            if dep_idx < node_ids.len() {
                graph.add_edge(node_ids[dep_idx], node_id);
            }
        }
    }

    let graph = Arc::new(tokio::sync::Mutex::new(graph));
    // Seed the room's live planner board before execution starts.
    if let Some(tid) = thread_id {
        let nodes: Vec<serde_json::Value> = tasks
            .iter()
            .enumerate()
            .map(|(i, task)| {
                serde_json::json!({
                    "node_id": node_ids[i].to_string(),
                    "bot_id": task.bot_id.to_string(),
                    "label": briefing_label(&task.instruction),
                    "depends_on": task
                        .depends_on
                        .iter()
                        .filter(|d| **d < node_ids.len() && **d < i)
                        .map(|d| node_ids[*d].to_string())
                        .collect::<Vec<String>>(),
                })
            })
            .collect();
        let _ = app.emit(
            "agent-stream",
            serde_json::json!({
                "kind": "plan_ready",
                "thread_id": tid.to_string(),
                "goal": truncate_chars(goal.trim(), 200),
                "nodes": nodes,
            }),
        );
    }
    // Project folders for every node in this office (same rule as send_to_chatroom).
    let office_dirs: Vec<String> = match chatroom_id {
        Some(cid) => {
            match ravenbot_db::queries::ChatRoomQueries::get(state.db.pool(), cid).await {
                Ok(Some(r)) if !r.project_folders.is_empty() => r.project_folders.clone(),
                Ok(Some(r)) => vec![ravenbot_runtime::default_project_dir(&r.name)
                    .to_string_lossy()
                    .to_string()],
                _ => Vec::new(),
            }
        }
        None => Vec::new(),
    };
    let node_event_app = app.clone();
    let executor = ravenbot_runtime::executor::GraphExecutor::new(
        state.runtime.clone(),
        state.db.clone(),
    )
    .with_working_dirs(office_dirs)
    .with_node_events(Arc::new(move |v| {
        let _ = node_event_app.emit("agent-stream", v);
    }));

    // Office nodes share one fallback stream emitter, so serialize office runs
    // (and drop the emitter afterwards) to keep live lanes isolated.
    let office_guard = if thread_id.is_some() {
        Some(office_exec_lock().lock().await)
    } else {
        None
    };
    if thread_id.is_some() {
        state.runtime.set_stream_emitter(Some(make_stream_emitter(app.clone())));
    }
    let exec_result = executor.execute(graph.clone()).await;
    if thread_id.is_some() {
        state.runtime.set_stream_emitter(None);
    }
    drop(office_guard);

    let blackboard = exec_result.map_err(|e| e.to_string())?;
    let graph_snapshot = graph.lock().await.clone();
    let checklist = graph_snapshot.to_checklist();

    // Post each node's real output + the lead's synthesis into the room.
    let summary = if let (Some(r), Some(tid)) = (&roster, thread_id) {
        let mut outputs: Vec<(String, String)> = Vec::new();
        for nid in graph_snapshot.ordered_node_ids() {
            let Some(node) = graph_snapshot.nodes.get(&nid) else { continue };
            let Some(output) = &node.output else { continue };
            let name = roster_name(r, node.bot_id);
            // Pure conversation: the bubble carries only the agent's reply;
            // author identity lives in the row gutter, task text on the board.
            let body = output.trim().to_string();
            let _ = post_bot_message(state.db.pool(), tid, node.bot_id, name.clone(), body).await;
            outputs.push((name, output.clone()));
        }

        // Surface failed nodes so a provider error is visible and actionable.
        let failures: Vec<String> = checklist
            .iter()
            .filter(|c| matches!(c.status, ravenbot_core::ChecklistStatus::Failed))
            .map(|c| {
                let who = c
                    .bot_id
                    .map(|id| roster_name(r, id))
                    .unwrap_or_else(|| "Teammate".to_string());
                format!(
                    "• **{}** — {}",
                    who,
                    c.result.clone().unwrap_or_else(|| "unknown error".to_string())
                )
            })
            .collect();
        if !failures.is_empty() {
            let hint = error_hint(failures.first().map(|s| s.as_str()).unwrap_or(""));
            let _ = post_bot_message(
                state.db.pool(),
                tid,
                lead_id,
                lead_name.clone(),
                format!("⚠️ Some teammates hit errors:\n{}\n\n{}", failures.join("\n"), hint),
            )
            .await;
        }

        let final_text = if outputs.is_empty() {
            checklist
                .iter()
                .map(|c| format!("{} {} → {}", match c.status {
                    ravenbot_core::ChecklistStatus::Completed => "✓",
                    ravenbot_core::ChecklistStatus::Failed => "✗",
                    _ => "○",
                }, c.label, c.result.clone().unwrap_or_default()))
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            state.runtime.synthesize_office(lead_id, &goal, &outputs).await
        };
        let _ = post_bot_message(
            state.db.pool(),
            tid,
            lead_id,
            lead_name.clone(),
            final_text.clone(),
        )
        .await;
        Some(final_text)
    } else {
        None
    };

    // Close the run only after the room holds the full persisted conversation
    // (matches send_to_chatroom; the UI holds live lanes until this arrives).
    if let Some(tid) = thread_id {
        let _ = app.emit("agent-stream", serde_json::json!({ "kind": "done", "thread_id": tid.to_string() }));
    }

    Ok(GraphResult {
        goal,
        thread_id,
        checklist,
        blackboard_data: blackboard.data,
        summary,
    })
}

/// Ask the room's lead to draft a task plan for a goal, so Plan mode can be
/// seeded by the CEO rather than only hand-authored. Returns the tasks
/// (assignee bot ids) plus an optional clarifying question.
#[tauri::command]
async fn draft_office_plan(
    state: State<'_, AppState>,
    chatroom_id: Uuid,
    goal: String,
) -> Result<serde_json::Value, String> {
    if goal.trim().is_empty() {
        return Err("Describe the goal first.".to_string());
    }
    let roster = load_office_roster(state.db.pool(), chatroom_id).await?;
    let room = ravenbot_db::queries::ChatRoomQueries::get(state.db.pool(), chatroom_id)
        .await
        .map_err(|e| e.to_string())?;
    let plan = state
        .runtime
        .plan_office(
            roster.lead_id,
            room.as_ref().and_then(|r| r.goal.as_deref()),
            room.as_ref().and_then(|r| r.policy.as_deref()),
            &roster.members,
            &goal,
        )
        .await;
    let Some(plan) = plan else {
        return Ok(serde_json::json!({
            "tasks": [],
            "question": null,
            "lead_id": roster.lead_id,
            "lead_name": roster.lead_name,
        }));
    };
    let tasks: Vec<serde_json::Value> = plan
        .tasks
        .iter()
        .map(|t| {
            let member = ravenbot_runtime::orchestrator::resolve_member(&roster.members, &t.bot);
            let bot_id = member.map(|m| m.bot_id).unwrap_or(roster.lead_id);
            serde_json::json!({
                "bot_id": bot_id,
                "bot_name": member.map(|m| m.name.clone()).unwrap_or_else(|| t.bot.clone()),
                "instruction": t.instruction,
                "depends_on": t.depends_on,
            })
        })
        .collect();
    Ok(serde_json::json!({
        "tasks": tasks,
        "question": plan.question,
        "lead_id": roster.lead_id,
        "lead_name": roster.lead_name,
    }))
}

#[tauri::command]
async fn pause_all(state: State<'_, AppState>) -> Result<(), String> {
    state.runtime.trigger_kill_switch("User triggered pause all").await;
    tracing::info!("All bots paused via kill switch");
    Ok(())
}

#[tauri::command]
async fn resume_all(state: State<'_, AppState>) -> Result<(), String> {
    state.runtime.release_kill_switch().await;
    tracing::info!("All bots resumed");
    Ok(())
}

#[tauri::command]
async fn get_status(state: State<'_, AppState>) -> Result<StatusInfo, String> {
    let bots = ravenbot_db::queries::BotQueries::list(state.db.pool())
        .await
        .map_err(|e| e.to_string())?;
    
    let kill_switch_active = state.runtime.is_paused().await;
    
    Ok(StatusInfo {
        active_bots: bots.len() as u32,
        running_tasks: 0,
        session_tokens: 0,
        session_cost: 0.0,
        kill_switch_active,
    })
}

#[tauri::command]
async fn trigger_kill_switch(
    state: State<'_, AppState>,
    reason: String,
) -> Result<(), String> {
    state.runtime.trigger_kill_switch(reason).await;
    Ok(())
}

#[tauri::command]
async fn release_kill_switch(
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.runtime.release_kill_switch().await;
    Ok(())
}

#[tauri::command]
async fn get_kill_switch_status(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let status = state.runtime.kill_switch().status().await;
    Ok(serde_json::json!({
        "state": format!("{:?}", status.state),
        "reason": status.reason,
        "triggered_at": status.triggered_at.map(|dt| dt.to_rfc3339()),
    }))
}

#[tauri::command]
async fn list_all_skills(state: State<'_, AppState>) -> Result<Vec<serde_json::Value>, String> {
    let reg = state.runtime.skill_registry();
    Ok(reg.list().iter().map(|s| serde_json::json!({
        // The real wire shape, not `{:?}`. The settings screen reads these to
        // show which capabilities an equipped skill needs, and it has to be able
        // to name the variant; a Debug string would have to be taken apart again
        // in the UI to learn that `FileSystem { paths: ["/"] }` means the file
        // system at all.
        "id": s.id(),
        "name": s.name(),
        "description": s.description(),
        "permissions": s.required_permissions()
    })).collect())
}

// ——— Plugins (in-app, no mocks) ———
#[tauri::command]
async fn sync_plugins(state: State<'_, AppState>) -> Result<usize, String> {
    let store = ravenbot_plugins::store::PluginStore::new(state.db.pool().clone());
    store.ensure_tables().await?;
    // Clean: no mock seeding — only real user-added plugins via import_openapi_plugin
    let existing = store.list_plugins(None).await?;
    Ok(existing.len())
}
#[tauri::command]
async fn list_plugins(state: State<'_, AppState>, query: Option<String>) -> Result<Vec<(String,String,String,String)>, String> {
    let store = ravenbot_plugins::store::PluginStore::new(state.db.pool().clone());
    store.ensure_tables().await?;
    store.list_plugins(query.as_deref()).await
}
#[tauri::command]
async fn list_bot_plugins(state: State<'_, AppState>, bot_id: Uuid) -> Result<Vec<String>, String> {
    let store = ravenbot_plugins::store::PluginStore::new(state.db.pool().clone());
    store.ensure_tables().await?;
    store.list_bot_plugins(bot_id).await
}
#[tauri::command]
async fn toggle_bot_plugin(state: State<'_, AppState>, bot_id: Uuid, plugin_id: String, enabled: bool) -> Result<(), String> {
    let store = ravenbot_plugins::store::PluginStore::new(state.db.pool().clone());
    store.ensure_tables().await?;
    store.set_bot_plugin(bot_id, &plugin_id, enabled).await
}
/// Enable/disable a plugin for EVERY bot (MCP-style global scope, P8).
#[tauri::command]
async fn toggle_plugin_global(state: State<'_, AppState>, plugin_id: String, enabled: bool) -> Result<(), String> {
    let store = ravenbot_plugins::store::PluginStore::new(state.db.pool().clone());
    store.ensure_tables().await?;
    store.set_plugin_global(&plugin_id, enabled).await
}
#[tauri::command]
async fn list_global_plugins(state: State<'_, AppState>) -> Result<Vec<(String,String,String,String)>, String> {
    let store = ravenbot_plugins::store::PluginStore::new(state.db.pool().clone());
    store.ensure_tables().await?;
    store.list_global_plugins().await
}
#[tauri::command]
async fn import_openapi_plugin(state: State<'_, AppState>, manifest_url: String) -> Result<String, String> {
    // Fetch OpenAPI and create a single plugin entry
    let client = reqwest::Client::new();
    let txt = client.get(&manifest_url).send().await.map_err(|e| e.to_string())?.text().await.map_err(|e| e.to_string())?;
    let id = format!("openapi_{}", uuid::Uuid::new_v4());
    let store = ravenbot_plugins::store::PluginStore::new(state.db.pool().clone());
    store.ensure_tables().await?;
    sqlx::query("INSERT OR REPLACE INTO plugins (id, name, description, logo, openapi_spec, enabled, created_at) VALUES (?, ?, ?, ?, ?, 1, ?)")
        .bind(&id).bind(&manifest_url).bind("Custom OpenAPI").bind("").bind(&txt).bind(chrono::Utc::now().to_rfc3339())
        .execute(state.db.pool()).await.map_err(|e| e.to_string())?;
    Ok(id)
}

// ——— MCP — 60+ servers as native tools, when necessary the bot will use them ———
#[tauri::command]
async fn list_mcp_servers(state: State<'_, AppState>, category: Option<String>) -> Result<Vec<ravenbot_mcp::McpServerSummary>, String> {
    let reg = ravenbot_mcp::McpRegistry::new(state.db.pool().clone());
    reg.ensure_tables().await?;
    reg.list_server_summaries(category.as_deref()).await
}
#[tauri::command]
async fn toggle_mcp_server(state: State<'_, AppState>, server_id: String, enabled: bool) -> Result<(), String> {
    let reg = ravenbot_mcp::McpRegistry::new(state.db.pool().clone());
    reg.ensure_tables().await?;
    reg.set_server_enabled(&server_id, enabled).await
}
#[tauri::command]
async fn toggle_bot_mcp_server(state: State<'_, AppState>, bot_id: Uuid, server_id: String, enabled: bool) -> Result<(), String> {
    let reg = ravenbot_mcp::McpRegistry::new(state.db.pool().clone());
    reg.ensure_tables().await?;
    reg.set_bot_server(bot_id, &server_id, enabled).await
}
#[tauri::command]
async fn list_bot_mcp_servers(state: State<'_, AppState>, bot_id: Uuid) -> Result<Vec<String>, String> {
    let reg = ravenbot_mcp::McpRegistry::new(state.db.pool().clone());
    reg.ensure_tables().await?;
    reg.list_bot_servers(bot_id).await
}
#[tauri::command]
async fn save_custom_mcp_server(state: State<'_, AppState>, server: ravenbot_mcp::McpServerConfig) -> Result<(), String> {
    let reg = ravenbot_mcp::McpRegistry::new(state.db.pool().clone());
    reg.ensure_tables().await?;
    reg.save_custom_server(server).await
}
#[tauri::command]
async fn delete_mcp_server(state: State<'_, AppState>, server_id: String) -> Result<(), String> {
    let reg = ravenbot_mcp::McpRegistry::new(state.db.pool().clone());
    reg.ensure_tables().await?;
    reg.delete_server(&server_id).await
}
#[tauri::command]
async fn get_mcp_server_env(state: State<'_, AppState>, server_id: String) -> Result<std::collections::HashMap<String, String>, String> {
    let reg = ravenbot_mcp::McpRegistry::new(state.db.pool().clone());
    reg.ensure_tables().await?;
    reg.get_server_env(&server_id).await
}
#[tauri::command]
async fn save_mcp_server_env(state: State<'_, AppState>, server_id: String, env: std::collections::HashMap<String, String>) -> Result<(), String> {
    let reg = ravenbot_mcp::McpRegistry::new(state.db.pool().clone());
    reg.ensure_tables().await?;
    reg.save_server_env(&server_id, env).await
}
#[tauri::command]
async fn test_mcp_server(state: State<'_, AppState>, server_id: String) -> Result<ravenbot_mcp::McpTestResult, String> {
    let reg = ravenbot_mcp::McpRegistry::new(state.db.pool().clone());
    reg.ensure_tables().await?;
    reg.test_server(&server_id).await
}
#[tauri::command]
async fn batch_assign_bot_mcp(state: State<'_, AppState>, server_id: String, bot_ids: Vec<Uuid>) -> Result<(), String> {
    let reg = ravenbot_mcp::McpRegistry::new(state.db.pool().clone());
    reg.ensure_tables().await?;
    reg.batch_assign_bot_servers(&server_id, bot_ids).await
}
#[tauri::command]
async fn batch_set_bot_mcp(state: State<'_, AppState>, bot_id: Uuid, server_ids: Vec<String>) -> Result<(), String> {
    let reg = ravenbot_mcp::McpRegistry::new(state.db.pool().clone());
    reg.ensure_tables().await?;
    reg.batch_set_bot_servers(bot_id, server_ids).await
}

// ——— Budgets — real enforcement + tracking ———

#[tauri::command]
async fn get_bot_budget(
    state: State<'_, AppState>,
    bot_id: Uuid,
) -> Result<serde_json::Value, String> {
    let budgets = ravenbot_governance::BudgetManager::new(state.db.pool().clone());
    let budget = budgets.get_budget(bot_id).await.map_err(|e| e.to_string())?;
    let (tokens_used, cost_used) = budgets.get_usage(bot_id).await.map_err(|e| e.to_string())?;
    let check = budgets.check_budget(bot_id).await.map_err(|e| e.to_string())?;

    Ok(serde_json::json!({
        "budget": budget.map(|b| serde_json::json!({
            "limit": match &b.limit {
                ravenbot_core::BudgetLimit::Unlimited => serde_json::json!({"kind": "unlimited"}),
                ravenbot_core::BudgetLimit::Tokens { max } => serde_json::json!({"kind": "tokens", "max": max}),
                ravenbot_core::BudgetLimit::Cost { max } => serde_json::json!({"kind": "cost", "max": max}),
            },
            "period": match b.period {
                ravenbot_core::BudgetPeriod::Hourly => "hourly",
                ravenbot_core::BudgetPeriod::Daily => "daily",
                ravenbot_core::BudgetPeriod::Weekly => "weekly",
                ravenbot_core::BudgetPeriod::Monthly => "monthly",
                ravenbot_core::BudgetPeriod::Total => "total",
            },
        })),
        "tokens_used": tokens_used,
        "cost_used": cost_used,
        "percentage_used": check.percentage_used,
        "allowed": check.allowed,
        "should_warn": check.should_warn,
    }))
}

#[tauri::command]
async fn set_bot_budget(
    state: State<'_, AppState>,
    bot_id: Uuid,
    kind: String,
    max: f64,
    period: String,
) -> Result<(), String> {
    let limit = match kind.as_str() {
        "tokens" => ravenbot_core::BudgetLimit::Tokens { max: max as u64 },
        "cost" => ravenbot_core::BudgetLimit::Cost { max },
        _ => ravenbot_core::BudgetLimit::Unlimited,
    };
    let budget_period = match period.as_str() {
        "hourly" => ravenbot_core::BudgetPeriod::Hourly,
        "daily" => ravenbot_core::BudgetPeriod::Daily,
        "weekly" => ravenbot_core::BudgetPeriod::Weekly,
        "monthly" => ravenbot_core::BudgetPeriod::Monthly,
        _ => ravenbot_core::BudgetPeriod::Total,
    };
    let budgets = ravenbot_governance::BudgetManager::new(state.db.pool().clone());
    budgets
        .set_budget(&ravenbot_core::Budget::new(bot_id, limit, budget_period))
        .await
}

#[tauri::command]
async fn reset_bot_budget(state: State<'_, AppState>, bot_id: Uuid) -> Result<(), String> {
    let budgets = ravenbot_governance::BudgetManager::new(state.db.pool().clone());
    budgets.reset_usage(bot_id).await
}

/// Lifetime usage for a bot (summed over all its runs) — so the telemetry
/// pill survives app restarts instead of resetting with the session.
#[tauri::command]
async fn get_session_usage(state: State<'_, AppState>, bot_id: Uuid) -> Result<serde_json::Value, String> {
    let row: Option<(i64, f64)> = sqlx::query_as(
        "SELECT COALESCE(SUM(r.tokens_consumed), 0), COALESCE(SUM(r.cost_estimate), 0.0)
         FROM runs r
         JOIN threads t ON t.id = r.thread_id
         WHERE t.bot_id = ?",
    )
    .bind(bot_id.to_string())
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| e.to_string())?
    .or(Some((0, 0.0)));

    let (tokens, cost) = row.unwrap_or((0, 0.0));
    Ok(serde_json::json!({ "tokens": tokens, "cost": cost }))
}

// ——— ChatRoom / Office Team ———

#[tauri::command]
async fn create_chatroom(
    state: State<'_, AppState>,
    name: String,
    description: String,
    office_template: String,
    avatar_url: Option<String>,
    avatar_style: Option<String>,
    // An existing folder to work in. Omit to get a fresh room created under
    // `~/RAVENBOT/projects/`.
    project_folder: Option<String>,
    goal: Option<String>,
    policy: Option<String>,
) -> Result<ChatRoom, String> {
    let mut room = ChatRoom::new(name, description, office_template.clone());
    if let Some(url) = avatar_url { room.avatar_url = Some(url); }
    if let Some(style) = avatar_style { room.avatar_style = Some(style); }
    room.goal = goal.map(|g| g.trim().to_string()).filter(|g| !g.is_empty());
    // An office with no policy is an office where every agent invents its own
    // standards, so a new one starts from the template's. The user can edit or
    // clear it afterwards; `write_policy` keeps POLICY.md in step either way.
    room.policy = policy
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .or_else(|| Some(office_org::template_policy(&office_template).to_string()));

    // An office always gets a room, at creation rather than lazily at first
    // send. `project_folder` lets a user point the office at a directory they
    // already have; otherwise we mint `~/RAVENBOT/projects/<slug>/` and seed
    // the charter files the agents read on their first turn.
    match project_folder.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        Some(chosen) => {
            // The user's folder is the office. Do NOT also mint a default
            // room — that would leave a stray empty directory and give the
            // agents a charter pointing somewhere they cannot write.
            let path = ravenbot_core::expand_home(chosen);
            ravenbot_core::ensure_dir(&path);
            ravenbot_runtime::workspace::seed_at(&path, &room, &[]);
            room.project_folders = vec![path.to_string_lossy().to_string()];
        }
        None => {
            let ws = ravenbot_runtime::workspace::seed(&room, &[]);
            room.project_folders = ravenbot_runtime::workspace::project_folders(&ws);
        }
    }

    ravenbot_db::queries::ChatRoomQueries::create(state.db.pool(), &room).await.map_err(|e| e.to_string())?;
    Ok(room)
}

/// Re-seed an office's charter files from the current room + roster.
///
/// Called whenever the roster, goal, or policy changes, so `OFFICE.md` in the
/// workspace always matches what the app displays. Best-effort: a workspace
/// that cannot be written must not fail the edit that triggered it.
async fn refresh_office_workspace(state: &AppState, chatroom_id: Uuid) {
    let Ok(Some(room)) = ravenbot_db::queries::ChatRoomQueries::get(state.db.pool(), chatroom_id).await
    else {
        return;
    };
    let members = ravenbot_db::queries::ChatRoomQueries::list_members(state.db.pool(), chatroom_id)
        .await
        .unwrap_or_default();

    let roster: Vec<ravenbot_runtime::workspace::RosterEntry> = {
        let mut out = Vec::with_capacity(members.len());
        for m in members {
            let Ok(Some(bot)) = ravenbot_db::queries::BotQueries::get(state.db.pool(), m.bot_id).await
            else {
                continue;
            };
            let mut entry = ravenbot_runtime::workspace::RosterEntry::new(
                bot.name.clone(),
                if m.rank.trim().is_empty() { bot.rank.clone().unwrap_or_default() } else { m.rank.clone() },
                if m.specialty.trim().is_empty() { bot.specialty.clone().unwrap_or_default() } else { m.specialty.clone() },
                bot.is_orchestrator,
            );
            // The roster records how each member works, not only their title,
            // so a teammate reading the folder sees the mandate.
            if let Some(p) = bot.config.custom_prompt.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
                entry = entry.with_mandate(p);
            }
            out.push(entry);
        }
        out
    };

    // Honour a user-chosen folder: seed there rather than in the default room.
    if let Some(first) = room.project_folders.first().map(String::as_str) {
        ravenbot_runtime::workspace::seed_at(&ravenbot_core::expand_home(first), &room, &roster);
    } else {
        ravenbot_runtime::workspace::seed(&room, &roster);
    }
}

#[tauri::command]
async fn list_chatrooms(state: State<'_, AppState>) -> Result<Vec<ChatRoom>, String> {
    ravenbot_db::queries::ChatRoomQueries::list(state.db.pool()).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_chatroom(state: State<'_, AppState>, chatroom_id: Uuid) -> Result<Option<ChatRoom>, String> {
    ravenbot_db::queries::ChatRoomQueries::get(state.db.pool(), chatroom_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn add_member_to_chatroom(
    state: State<'_, AppState>,
    chatroom_id: Uuid,
    bot_id: Uuid,
    rank: String,
    specialty: String,
) -> Result<(), String> {
    let member = ChatRoomMember { chatroom_id, bot_id, rank, specialty, joined_at: chrono::Utc::now() };
    ravenbot_db::queries::ChatRoomQueries::add_member(state.db.pool(), &member).await.map_err(|e| e.to_string())?;
    refresh_office_workspace(&state, chatroom_id).await;
    Ok(())
}

#[tauri::command]
async fn list_chatroom_members(state: State<'_, AppState>, chatroom_id: Uuid) -> Result<Vec<ChatRoomMember>, String> {
    ravenbot_db::queries::ChatRoomQueries::list_members(state.db.pool(), chatroom_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn update_chatroom(state: State<'_, AppState>, room: ChatRoom) -> Result<(), String> {
    // Keep the policy document in the workspace in step with the field.
    ravenbot_runtime::workspace::write_policy(&room);
    let result =
        ravenbot_db::queries::ChatRoomQueries::update(state.db.pool(), &room).await.map_err(|e| e.to_string());
    if result.is_ok() {
        refresh_office_workspace(&state, room.id).await;
    }
    result
}

#[tauri::command]
async fn delete_chatroom(state: State<'_, AppState>, chatroom_id: Uuid) -> Result<(), String> {
    ravenbot_db::queries::ChatRoomQueries::delete(state.db.pool(), chatroom_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn remove_chatroom_member(state: State<'_, AppState>, chatroom_id: Uuid, bot_id: Uuid) -> Result<(), String> {
    ravenbot_db::queries::ChatRoomQueries::remove_member(state.db.pool(), chatroom_id, bot_id).await.map_err(|e| e.to_string())?;
    refresh_office_workspace(&state, chatroom_id).await;
    Ok(())
}

#[tauri::command]
async fn update_chatroom_member(state: State<'_, AppState>, chatroom_id: Uuid, bot_id: Uuid, rank: String, specialty: String) -> Result<(), String> {
    ravenbot_db::queries::ChatRoomQueries::update_member(state.db.pool(), chatroom_id, bot_id, &rank, &specialty).await.map_err(|e| e.to_string())?;
    refresh_office_workspace(&state, chatroom_id).await;
    Ok(())
}

#[tauri::command]
async fn create_bot_for_office(
    state: State<'_, AppState>,
    name: String,
    description: String,
    rank: String,
    specialty: String,
    avatar_url: Option<String>,
    avatar_style: Option<String>,
    chatroom_id: Option<Uuid>,
) -> Result<Bot, String> {
    let mut bot = Bot::new(name, description);
    if let Some(url) = avatar_url { bot.avatar_url = Some(url); }
    if let Some(style) = avatar_style { bot.avatar_style = Some(style); }
    let (default_provider, default_model) = resolve_default_model(state.db.pool()).await;
    bot.config.model_provider = default_provider;
    bot.config.model_id = default_model;
    bot.rank = Some(rank.clone());
    bot.specialty = Some(specialty.clone());
    ravenbot_db::queries::BotQueries::insert(state.db.pool(), &bot).await.map_err(|e| e.to_string())?;
    if let Some(cid) = chatroom_id {
        let member = ChatRoomMember { chatroom_id: cid, bot_id: bot.id, rank, specialty, joined_at: chrono::Utc::now() };
        ravenbot_db::queries::ChatRoomQueries::add_member(state.db.pool(), &member).await.map_err(|e| e.to_string())?;
    }
    Ok(bot)
}

/// The built-in role blueprint for an office template (CEO + specialists).
#[tauri::command]
async fn default_office_org(office_template: String) -> Result<Vec<office_org::RoleSpec>, String> {
    Ok(office_org::default_org(&office_template))
}

/// Result of staffing an office: which agents were freshly created vs which
/// existing fleet agents were reused (so the same agent is never duplicated).
#[derive(serde::Serialize)]
struct ProvisionOutcome {
    created: Vec<Bot>,
    reused: Vec<Bot>,
}

/// Find an existing fleet agent that should fill a role instead of creating a
/// duplicate. Preference order: exact name → exact rank+specialty → specialty.
fn find_reusable_agent(
    bots: &[Bot],
    used: &std::collections::HashSet<Uuid>,
    role: &office_org::RoleSpec,
) -> Option<Bot> {
    let name = role.name.trim().to_lowercase();
    let rank = role.rank.trim().to_lowercase();
    let specialty = role.specialty.trim().to_lowercase();
    let candidates = || bots.iter().filter(|b| !used.contains(&b.id));

    candidates()
        .find(|b| b.name.trim().to_lowercase() == name)
        .or_else(|| {
            candidates().find(|b| {
                b.rank.as_deref().unwrap_or("").trim().to_lowercase() == rank
                    && b.specialty.as_deref().unwrap_or("").trim().to_lowercase() == specialty
            })
        })
        .or_else(|| {
            if specialty.is_empty() {
                None
            } else {
                candidates().find(|b| {
                    b.specialty.as_deref().unwrap_or("").trim().to_lowercase() == specialty
                })
            }
        })
        .or_else(|| {
            // Rank-only match as a last resort for role titles like "CEO".
            if rank.is_empty() {
                None
            } else {
                candidates().find(|b| {
                    b.rank.as_deref().unwrap_or("").trim().to_lowercase() == rank
                })
            }
        })
        .cloned()
}

/// Staff an office from a role blueprint. Existing fleet agents that match a
/// role are reused (added to the office) instead of being duplicated; only
/// genuinely new roles create new agents. Each bot keeps its own model, skills,
/// MCP servers and connectors, all editable by the owner.
#[tauri::command]
async fn provision_office_org(
    state: State<'_, AppState>,
    chatroom_id: Uuid,
    roles: Vec<office_org::RoleSpec>,
) -> Result<ProvisionOutcome, String> {
    if roles.is_empty() {
        return Err("No roles to provision".to_string());
    }
    if roles.iter().filter(|r| r.is_lead).count() > 1 {
        return Err("Only one lead (CEO) role is allowed".to_string());
    }

    let all_bots = ravenbot_db::queries::BotQueries::list(state.db.pool())
        .await
        .map_err(|e| e.to_string())?;
    let existing_members = ravenbot_db::queries::ChatRoomQueries::list_members(state.db.pool(), chatroom_id)
        .await
        .map_err(|e| e.to_string())?;
    let mut used: std::collections::HashSet<Uuid> =
        existing_members.iter().map(|m| m.bot_id).collect();

    // ── The office half of every brief ──
    //
    // The template knows what a Coder owns; only the room knows where the
    // office works, who else is on the team, and what the rules are. Compose
    // both together once, here, so every agent is provisioned with a brief
    // that names its own folder instead of one that only names a job.
    let room = ravenbot_db::queries::ChatRoomQueries::get(state.db.pool(), chatroom_id)
        .await
        .ok()
        .flatten();
    let office = {
        let mut ctx = office_org::OfficeContext::new(
            room.as_ref().map(|r| r.name.clone()).unwrap_or_else(|| "Office".into()),
        )
        .with_goal(room.as_ref().and_then(|r| r.goal.clone()))
        .with_policy(room.as_ref().and_then(|r| r.policy.clone()))
        .with_workspace(room.as_ref().and_then(|r| r.project_folders.first().cloned()));
        ctx = ctx.with_roster(
            roles
                .iter()
                .map(|r| office_org::Teammate::new(
                    r.name.clone(),
                    r.rank.clone(),
                    r.specialty.clone(),
                    r.is_lead,
                ))
                .collect(),
        );
        ctx
    };

    let (default_provider, default_model) = resolve_default_model(state.db.pool()).await;
    let mut created = Vec::new();
    let mut reused = Vec::new();

    for raw_role in roles {
        // Common skills and the office context are attached before the brief is
        // written, so the stored prompt matches what the agent can actually do.
        let role = office_org::with_common_skills(raw_role);
        let brief = office_org::compose_system_prompt(&office, &role);

        let bot = if let Some(mut existing) = find_reusable_agent(&all_bots, &used, &role) {
            // Fill only what's missing — never clobber the owner's settings.
            let mut changed = false;
            if existing.config.model_provider.trim().is_empty() {
                existing.config.model_provider = default_provider.clone();
                existing.config.model_id = default_model.clone();
                changed = true;
            }
            // Only a blank or persona-sized prompt is replaced. An agent whose
            // prompt someone has since written out is left alone, even if it
            // names the wrong office.
            let needs_brief = existing
                .config
                .custom_prompt
                .as_deref()
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(|p| !office_org::mandate_is_substantive(p))
                .unwrap_or(true);
            if needs_brief && office_org::mandate_is_substantive(&brief) {
                existing.config.custom_prompt = Some(brief.clone());
                changed = true;
            }
            if existing.skills.is_empty() && !role.skills.is_empty() {
                existing.skills = role.skills.clone();
                changed = true;
            }
            if role.is_lead && !existing.is_orchestrator {
                existing.is_orchestrator = true;
                changed = true;
            }
            if changed {
                ravenbot_db::queries::BotQueries::update(state.db.pool(), &existing)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            reused.push(existing.clone());
            existing
        } else {
            let mut bot = Bot::new(role.name.clone(), role.specialty.clone());
            bot.rank = Some(role.rank.clone());
            bot.specialty = Some(role.specialty.clone());
            bot.is_orchestrator = role.is_lead;
            bot.config.model_provider = default_provider.clone();
            bot.config.model_id = default_model.clone();
            if role.is_lead {
                bot.avatar_color = "#8b5cf6".to_string();
            }
            bot.config.custom_prompt = Some(brief.clone());
            if !role.skills.is_empty() {
                bot.skills = role.skills.clone();
            }
            if let Some(style) = role.avatar_style.as_deref().filter(|s| !s.trim().is_empty()) {
                bot.avatar_url = Some(Bot::dicebear_url(&role.name, style));
                bot.avatar_style = Some(style.to_string());
            }
            ravenbot_db::queries::BotQueries::insert(state.db.pool(), &bot)
                .await
                .map_err(|e| e.to_string())?;
            created.push(bot.clone());
            bot
        };

        used.insert(bot.id);
        let member = ChatRoomMember {
            chatroom_id,
            bot_id: bot.id,
            rank: role.rank,
            specialty: role.specialty,
            joined_at: chrono::Utc::now(),
        };
        ravenbot_db::queries::ChatRoomQueries::add_member(state.db.pool(), &member)
            .await
            .map_err(|e| e.to_string())?;
    }

    Ok(ProvisionOutcome { created, reused })
}

/// Ask the office's CEO (or any available bot when the office is empty) to
/// propose the team needed for a brief. Falls back to the built-in blueprint
/// when no model is configured or the model output is unusable.
#[tauri::command]
async fn draft_office_org(
    state: State<'_, AppState>,
    chatroom_id: Uuid,
    brief: String,
) -> Result<serde_json::Value, String> {
    let room = ravenbot_db::queries::ChatRoomQueries::get(state.db.pool(), chatroom_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Office not found")?;
    let members = ravenbot_db::queries::ChatRoomQueries::list_members(state.db.pool(), chatroom_id)
        .await
        .map_err(|e| e.to_string())?;

    // The CEO sees the whole fleet, not just this office, so it can reuse
    // agents that already exist anywhere in the app instead of duplicating them.
    let fleet = ravenbot_db::queries::BotQueries::list(state.db.pool())
        .await
        .unwrap_or_default();
    let member_ids: std::collections::HashSet<Uuid> = members.iter().map(|m| m.bot_id).collect();
    let mut existing: Vec<(String, String)> = fleet
        .iter()
        .map(|b| {
            let rank = b.rank.clone().unwrap_or_default();
            let label = if member_ids.contains(&b.id) {
                format!("{rank} · in this office")
            } else {
                format!("{rank} · available in fleet")
            };
            (b.name.clone(), label)
        })
        .collect();
    if existing.is_empty() {
        existing.push(("(none — empty fleet)".to_string(), String::new()));
    }

    // Pick the model to think with: the office lead, else any orchestrator,
    // else any bot at all.
    let mut thinker: Option<Bot> = None;
    for m in &members {
        if let Ok(Some(bot)) = ravenbot_db::queries::BotQueries::get(state.db.pool(), m.bot_id).await {
            if bot.is_orchestrator {
                thinker = Some(bot);
                break;
            }
            if thinker.is_none() {
                thinker = Some(bot);
            }
        }
    }
    if thinker.is_none() {
        if let Ok(bots) = ravenbot_db::queries::BotQueries::list(state.db.pool()).await {
            thinker = bots.into_iter().find(|b| b.is_orchestrator).or(None);
            if thinker.is_none() {
                // Any bot can act as the hiring manager if no orchestrator exists.
                if let Ok(bots) = ravenbot_db::queries::BotQueries::list(state.db.pool()).await {
                    thinker = bots.into_iter().next();
                }
            }
        }
    }

    let fallback = || {
        serde_json::json!({
            "roles": office_org::default_org(&room.office_template),
            "question": serde_json::Value::Null,
            "source": "template",
        })
    };

    let Some(thinker) = thinker else {
        return Ok(fallback());
    };

    let prompt = office_org::org_prompt(
        &room.name,
        room.goal.as_deref(),
        &brief,
        &existing,
    );
    match state
        .runtime
        .complete_as_bot(
            thinker.id,
            "You are the CEO of an AI office. Output only valid JSON, with no \
             prose before or after it.",
            &prompt,
            // Room for a real brief. 1600 tokens truncated the roster to two or
            // three roles, because each one now carries an artifact, a way to
            // prove the work, and a handoff. 6000 fits a 7-role office with
            // room to spare and is still a bounded, non-streaming call.
            6000,
        )
        .await
    {
        Ok(text) => match office_org::parse_org(&text) {
            Some(org) => Ok(serde_json::json!({
                "roles": org.roles,
                "question": org.question,
                "source": "ceo",
            })),
            None => Ok(fallback()),
        },
        Err(e) => {
            tracing::warn!(error = %e, "CEO org proposal failed; using template blueprint");
            Ok(fallback())
        }
    }
}

#[tauri::command]
async fn get_chatroom_thread(state: State<'_, AppState>, chatroom_id: Uuid) -> Result<Option<Uuid>, String> {
    let row: Option<(String,)> = sqlx::query_as("SELECT thread_id FROM chatroom_threads WHERE chatroom_id = ?")
        .bind(chatroom_id.to_string()).fetch_optional(state.db.pool()).await.map_err(|e| e.to_string())?;
    Ok(row.and_then(|r| Uuid::parse_str(&r.0).ok()))
}
#[tauri::command]
async fn add_office_memory(state: State<'_, AppState>, chatroom_id: Uuid, content: String, category: String, created_by: Option<Uuid>) -> Result<OfficeMemory, String> {
    state.runtime.office_memory().add(chatroom_id, &content, &category, created_by, 0.7).await
}
#[tauri::command]
async fn list_office_memories(state: State<'_, AppState>, chatroom_id: Uuid) -> Result<Vec<OfficeMemory>, String> {
    state.runtime.office_memory().list(chatroom_id).await
}
#[tauri::command]
async fn search_office_memories(state: State<'_, AppState>, chatroom_id: Uuid, query: String) -> Result<Vec<(OfficeMemory, f32)>, String> {
    state.runtime.office_memory().retrieve(chatroom_id, &query, 10, 0.3).await
}
#[tauri::command]
async fn get_agent_intelligence(state: State<'_, AppState>, bot_id: Uuid) -> Result<AgentIntelligence, String> {
    state.runtime.learning().get_intelligence(bot_id).await
}
#[tauri::command]
async fn list_agent_learnings(state: State<'_, AppState>, bot_id: Uuid) -> Result<Vec<AgentLearning>, String> {
    state.runtime.learning().list_learnings(bot_id, 20).await
}

/// A room's working roster: the member rows, their loaded bots, the
/// orchestration view, and the resolved lead. Shared by the natural-language
/// office flow and Plan mode so both attribute work identically.
struct OfficeRoster {
    bots: Vec<(ravenbot_core::ChatRoomMember, Bot)>,
    members: Vec<ravenbot_runtime::orchestrator::OfficeMember>,
    lead_id: Uuid,
    lead_name: String,
}

async fn load_office_roster(
    pool: &sqlx::SqlitePool,
    chatroom_id: Uuid,
) -> Result<OfficeRoster, String> {
    let rows = ravenbot_db::queries::ChatRoomQueries::list_members(pool, chatroom_id)
        .await
        .map_err(|e| e.to_string())?;
    if rows.is_empty() {
        return Err("No bots in this office".to_string());
    }
    let mut bots = Vec::new();
    for m in rows {
        if let Ok(Some(bot)) = ravenbot_db::queries::BotQueries::get(pool, m.bot_id).await {
            bots.push((m, bot));
        }
    }
    if bots.is_empty() {
        return Err("No valid bots in this office".to_string());
    }
    let members: Vec<ravenbot_runtime::orchestrator::OfficeMember> = bots
        .iter()
        .map(|(m, bot)| ravenbot_runtime::orchestrator::OfficeMember {
            bot_id: bot.id,
            name: bot.name.clone(),
            rank: m.rank.clone(),
            specialty: m.specialty.clone(),
        })
        .collect();
    let lead_id = bots
        .iter()
        .find(|(_, bot)| bot.is_orchestrator)
        .map(|(_, bot)| bot.id)
        .or_else(|| {
            bots.iter()
                .find(|(m, _)| {
                    let r = m.rank.to_lowercase();
                    r.contains("lead") || r.contains("chief") || r.contains("manager")
                })
                .map(|(_, bot)| bot.id)
        })
        .unwrap_or(members[0].bot_id);
    let lead_name = bots
        .iter()
        .find(|(_, bot)| bot.id == lead_id)
        .map(|(_, bot)| bot.name.clone())
        .unwrap_or_else(|| "Team Lead".to_string());
    Ok(OfficeRoster { bots, members, lead_id, lead_name })
}

/// Resolve a bot's display name within the roster.
fn roster_name(roster: &OfficeRoster, bot_id: Uuid) -> String {
    roster
        .bots
        .iter()
        .find(|(_, bot)| bot.id == bot_id)
        .map(|(_, bot)| bot.name.clone())
        .unwrap_or_else(|| "Teammate".to_string())
}

/// Get (or create) the shared group thread for a chatroom.
async fn ensure_chatroom_thread(
    pool: &sqlx::SqlitePool,
    chatroom_id: Uuid,
    first_bot: Uuid,
) -> Result<Uuid, String> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT thread_id FROM chatroom_threads WHERE chatroom_id = ?")
            .bind(chatroom_id.to_string())
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;
    if let Some(r) = row {
        if let Ok(id) = Uuid::parse_str(&r.0) {
            return Ok(id);
        }
    }
    let room = ravenbot_db::queries::ChatRoomQueries::get(pool, chatroom_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Chatroom not found")?;
    let thread = Thread::new(first_bot, format!("{} — group", room.name));
    ravenbot_db::queries::ThreadQueries::create(pool, &thread)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("INSERT OR REPLACE INTO chatroom_threads (chatroom_id, thread_id) VALUES (?, ?)")
        .bind(chatroom_id.to_string())
        .bind(thread.id.to_string())
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(thread.id)
}

/// Post an assistant turn attributed to a specific bot (office conversation).
async fn post_bot_message(
    pool: &sqlx::SqlitePool,
    thread_id: Uuid,
    bot_id: Uuid,
    name: String,
    text: String,
) -> Result<(), String> {
    let msg = Message::assistant_from_bot(thread_id, bot_id, name, text);
    ravenbot_db::queries::MessageQueries::insert(pool, &msg)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Build the planner input from the room transcript plus the latest request, so
/// a follow-up (e.g. an answer to the CEO's clarifying question) is understood
/// in context instead of being planned in isolation.
async fn build_planning_request(
    pool: &sqlx::SqlitePool,
    thread_id: Uuid,
    content: &str,
) -> String {
    let history = ravenbot_db::queries::MessageQueries::list_by_thread(pool, thread_id)
        .await
        .unwrap_or_default();
    let mut transcript: Vec<String> = Vec::new();
    for msg in history.iter().rev().take(12).collect::<Vec<_>>().into_iter().rev() {
        let text = match &msg.content {
            ravenbot_core::MessageContent::Text { text, .. } => text.trim().to_string(),
            _ => continue,
        };
        if text.is_empty() {
            continue;
        }
        if matches!(msg.role, ravenbot_core::MessageRole::User) {
            transcript.push(format!("Client: {}", truncate_chars(&text, 600)));
        } else {
            let who = msg.sender_name.clone().unwrap_or_else(|| "Team".to_string());
            transcript.push(format!("{}: {}", who, truncate_chars(&text, 600)));
        }
    }
    if transcript.len() > 1 {
        format!(
            "Conversation so far:\n{}\n\nLatest client request:\n{}",
            transcript.join("\n"),
            content
        )
    } else {
        content.to_string()
    }
}

/// The task portion of a node instruction, for compact briefings. Node
/// instructions are self-contained (`Goal: …\n\nYour task: …`); the briefing
/// should not repeat the goal on every line.
fn briefing_label(instruction: &str) -> String {
    instruction
        .rsplit("\n\nYour task: ")
        .next()
        .unwrap_or(instruction)
        .trim()
        .to_string()
}

/// Render a lead's task briefing: numbered, with assignee and dependencies.
fn briefing_markdown(items: &[(String, String, Vec<usize>)]) -> String {
    let mut lines = vec!["📋 **Briefing** — here's how I'll split this across the team:".to_string()];
    for (i, (who, instruction, deps)) in items.iter().enumerate() {
        let dep = if deps.is_empty() {
            String::new()
        } else {
            format!(
                " _(after {})_",
                deps.iter().map(|d| (d + 1).to_string()).collect::<Vec<_>>().join(", ")
            )
        };
        lines.push(format!("{}. **{}** → {}{}", i + 1, who, instruction.trim(), dep));
    }
    lines.join("\n")
}

#[tauri::command]
async fn send_to_chatroom(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    chatroom_id: Uuid,
    content: String,
    attachments: Option<Vec<AttachmentInput>>,
) -> Result<serde_json::Value, String> {
    let roster = load_office_roster(state.db.pool(), chatroom_id).await?;
    let thread_id =
        ensure_chatroom_thread(state.db.pool(), chatroom_id, roster.bots[0].1.id).await?;
    // Save user message to group thread (with any attachments)
    let mut user_msg = Message::user(thread_id, &content);
    if let Some(atts) = attachments {
        user_msg.attachments = build_image_attachments(atts);
    }
    ravenbot_db::queries::MessageQueries::insert(state.db.pool(), &user_msg)
        .await
        .map_err(|e| e.to_string())?;

    // ── Orchestrate the office with a real planner ────────────────────────
    // A lead bot decomposes the request into a task DAG (who does what, in
    // what order); we execute that, then the lead synthesizes the final
    // answer. If planning fails we fall back to a fan-out of every member.
    let room = ravenbot_db::queries::ChatRoomQueries::get(state.db.pool(), chatroom_id)
        .await
        .map_err(|e| e.to_string())?;
    let goal = room.as_ref().and_then(|r| r.goal.clone());
    let policy = room.as_ref().and_then(|r| r.policy.clone());

    let planning_request = build_planning_request(state.db.pool(), thread_id, &content).await;

    let plan = state
        .runtime
        .plan_office(
            roster.lead_id,
            goal.as_deref(),
            policy.as_deref(),
            &roster.members,
            &planning_request,
        )
        .await;

    // ── CEO clarification loop ────────────────────────────────────────────
    // If the lead lacks enough information, it asks the client directly. We
    // post its question into the room and stop; the client's next message is
    // the answer, and the lead plans from there.
    if let Some(question) = plan.as_ref().and_then(|p| p.question.clone()).filter(|q| !q.trim().is_empty()) {
        let _ = post_bot_message(
            state.db.pool(),
            thread_id,
            roster.lead_id,
            roster.lead_name.clone(),
            format!("❓ I need one thing before I brief the team:\n\n{}", question.trim()),
        )
        .await;
        let _ = app.emit("agent-stream", serde_json::json!({ "kind": "done", "thread_id": thread_id.to_string() }));
        return Ok(serde_json::json!({
            "thread_id": thread_id,
            "awaiting_answer": true,
            "question": question,
            "checklist": [],
            "blackboard": {},
            "summary": question,
        }));
    }

    // The lead briefs the team in the room before work starts, so the client
    // sees the breakdown the moment it happens.
    if let Some(p) = &plan {
        if !p.tasks.is_empty() {
            let items: Vec<(String, String, Vec<usize>)> = p
                .tasks
                .iter()
                .map(|t| {
                    let who = ravenbot_runtime::orchestrator::resolve_member(&roster.members, &t.bot)
                        .map(|m| m.name.clone())
                        .unwrap_or_else(|| t.bot.clone());
                    (who, briefing_label(&t.instruction), t.depends_on.clone())
                })
                .collect();
            let _ = post_bot_message(
                state.db.pool(),
                thread_id,
                roster.lead_id,
                roster.lead_name.clone(),
                briefing_markdown(&items),
            )
            .await;
        }
    }

    // Build the graph from the plan (or the fan-out fallback), keeping a
    // local node→label map so synthesis can name each contributor without
    // polluting the instructions the bots actually receive.
    let mut graph = TaskGraph::new(&content);
    let mut node_ids: Vec<uuid::Uuid> = Vec::new();
    let mut label_for_node: std::collections::HashMap<uuid::Uuid, String> =
        std::collections::HashMap::new();
    // (node_id, bot_id, board label, dependency task indices) for the live
    // planner board — emitted as `plan_ready` before execution starts.
    let mut board_nodes: Vec<(uuid::Uuid, uuid::Uuid, String, Vec<usize>)> = Vec::new();

    if let Some(plan) = &plan {
        for task in &plan.tasks {
            let assignee = ravenbot_runtime::orchestrator::resolve_member(&roster.members, &task.bot)
                .map(|m| m.bot_id)
                .unwrap_or(roster.lead_id);
            let nid = graph.add_node(assignee, task.instruction.clone());
            label_for_node.insert(nid, task.bot.clone());
            node_ids.push(nid);
            board_nodes.push((nid, assignee, briefing_label(&task.instruction), task.depends_on.clone()));
        }
        // Wire dependencies after all nodes exist.
        for (i, task) in plan.tasks.iter().enumerate() {
            for dep in &task.depends_on {
                if *dep < node_ids.len() && *dep < i {
                    graph.add_edge(node_ids[*dep], node_ids[i]);
                }
            }
        }
    } else {
        for (m, bot) in &roster.bots {
            let instruction = format!("[{} - {}] {}", m.rank, m.specialty, content);
            let nid = graph.add_node(bot.id, instruction.clone());
            label_for_node.insert(nid, bot.name.clone());
            node_ids.push(nid);
            board_nodes.push((nid, bot.id, truncate_chars(&instruction, 120), Vec::new()));
        }
    }

    let graph = Arc::new(tokio::sync::Mutex::new(graph));
    // Seed the room's live board (kanban + dependency DAG) before any node
    // starts streaming, so the user sees the plan form immediately.
    {
        let nodes: Vec<serde_json::Value> = board_nodes
            .iter()
            .enumerate()
            .map(|(i, (nid, bot, label, deps))| {
                serde_json::json!({
                    "node_id": nid.to_string(),
                    "bot_id": bot.to_string(),
                    "label": label,
                    "depends_on": deps
                        .iter()
                        .filter(|d| **d < node_ids.len() && **d < i)
                        .map(|d| node_ids[*d].to_string())
                        .collect::<Vec<_>>(),
                })
            })
            .collect();
        let _ = app.emit(
            "agent-stream",
            serde_json::json!({
                "kind": "plan_ready",
                "thread_id": thread_id.to_string(),
                "goal": truncate_chars(content.trim(), 200),
                "nodes": nodes,
            }),
        );
    }
    // Project folders for every node in this office: configured folders, or an
    // auto-created default named after the office.
    let office_dirs: Vec<String> = {
        let configured = room
            .as_ref()
            .map(|r| r.project_folders.clone())
            .unwrap_or_default();
        if configured.is_empty() {
            let name = room
                .as_ref()
                .map(|r| r.name.clone())
                .unwrap_or_else(|| "office".to_string());
            vec![ravenbot_runtime::default_project_dir(&name)
                .to_string_lossy()
                .to_string()]
        } else {
            configured
        }
    };
    let node_event_app = app.clone();
    let executor = ravenbot_runtime::executor::GraphExecutor::new(state.runtime.clone(), state.db.clone())
        .with_working_dirs(office_dirs)
        .with_node_events(Arc::new(move |v| {
            let _ = node_event_app.emit("agent-stream", v);
        }));

    // Office nodes create their own threads, so they use the fallback emitter.
    // Serialize whole-office execution so concurrent offices don't interleave
    // on that shared fallback.
    let office_guard = office_exec_lock().lock().await;
    state.runtime.set_stream_emitter(Some(make_stream_emitter(app.clone())));
    let exec_result = executor.execute(graph.clone()).await;
    state.runtime.set_stream_emitter(None);
    drop(office_guard);
    // NOTE: `done` is emitted only after the final posts below, so the room
    // never blanks between the last token and the persisted conversation.

    match exec_result {
        Ok(blackboard) => {
            let checklist = graph.lock().await.clone().to_checklist();
            // Pull each node's real output (in dependency order), with the bot
            // that produced it, so the room shows a genuine conversation.
            let node_results: Vec<(uuid::Uuid, String, String, String)> = {
                let g = graph.lock().await;
                let mut out = Vec::new();
                for nid in g.ordered_node_ids() {
                    if let Some(node) = g.nodes.get(&nid) {
                        if let Some(result) = &node.output {
                            let label = label_for_node
                                .get(&nid)
                                .cloned()
                                .unwrap_or_else(|| "Task".to_string());
                            out.push((
                                node.bot_id,
                                label,
                                node.instruction.clone(),
                                result.clone(),
                            ));
                        }
                    }
                }
                out
            };

            // Post every specialist's work into the group thread, attributed to
            // the bot that did it (this is what makes the office "talk").
            // Pure conversation: the bubble carries only the agent's reply —
            // the row's author header/gutter identifies who and the board
            // carries what task they were given.
            for (bot_id, name, _instruction, output) in &node_results {
                let _ = post_bot_message(
                    state.db.pool(),
                    thread_id,
                    *bot_id,
                    name.clone(),
                    output.trim().to_string(),
                )
                .await;
            }

            // Surface failed teammates instead of failing silently: a node
            // that hit a provider error leaves no output, which used to look
            // like the task simply never ran.
            let failures: Vec<String> = checklist
                .iter()
                .filter(|c| matches!(c.status, ravenbot_core::ChecklistStatus::Failed))
                .map(|c| {
                    let who = c
                        .bot_id
                        .map(|id| roster_name(&roster, id))
                        .unwrap_or_else(|| "Teammate".to_string());
                    format!(
                        "• **{}** — {}",
                        who,
                        c.result.clone().unwrap_or_else(|| "unknown error".to_string())
                    )
                })
                .collect();
            if !failures.is_empty() {
                let hint = error_hint(failures.first().map(|s| s.as_str()).unwrap_or(""));
                let _ = post_bot_message(
                    state.db.pool(),
                    thread_id,
                    roster.lead_id,
                    roster.lead_name.clone(),
                    format!("⚠️ Some teammates hit errors:\n{}\n\n{}", failures.join("\n"), hint),
                )
                .await;
            }

            // The lead writes the final, integrated answer (also attributed).
            let node_outputs: Vec<(String, String)> = node_results
                .iter()
                .map(|(_, label, _, output)| (label.clone(), output.clone()))
                .collect();
            let final_summary = if node_outputs.is_empty() {
                checklist
                    .iter()
                    .map(|c| format!("{} {} → {}", match c.status {
                        ravenbot_core::ChecklistStatus::Completed => "✓",
                        ravenbot_core::ChecklistStatus::Failed => "✗",
                        _ => "○",
                    }, c.label, c.result.clone().unwrap_or_default()))
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                state.runtime.synthesize_office(roster.lead_id, &content, &node_outputs).await
            };

            let _ = post_bot_message(
                state.db.pool(),
                thread_id,
                roster.lead_id,
                roster.lead_name.clone(),
                final_summary.clone(),
            )
            .await;
            let _ = app.emit("agent-stream", serde_json::json!({ "kind": "done", "thread_id": thread_id.to_string() }));
            Ok(serde_json::json!({
                "thread_id": thread_id,
                "checklist": checklist,
                "blackboard": blackboard.data,
                "summary": final_summary,
                "planned": plan.is_some(),
            }))
        }
        Err(err) => {
            let err_str = err.to_string();
            tracing::warn!("Chatroom execution error: {}", err_str);
            let error_summary = format!("⚠️ **Model Error:** {}\n\n{}{}", err_str, error_hint(&err_str), "\n\nYou can also run the office's bots fully offline via local Ollama models.");
            let _ = post_bot_message(
                state.db.pool(),
                thread_id,
                roster.lead_id,
                roster.lead_name.clone(),
                error_summary.clone(),
            )
            .await;
            let _ = app.emit("agent-stream", serde_json::json!({ "kind": "done", "thread_id": thread_id.to_string() }));
            Ok(serde_json::json!({ "thread_id": thread_id, "checklist": [], "blackboard": {}, "summary": error_summary }))
        }
    }
}

/// Truncate to a char boundary at most `max` characters (safe for UTF-8).
fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push('…');
    out
}


// Input types

#[derive(serde::Deserialize)]
struct GraphTask {
    bot_id: Uuid,
    instruction: String,
    depends_on: Vec<usize>,
}

// Output types

#[derive(serde::Serialize)]
struct StatusInfo {
    active_bots: u32,
    running_tasks: u32,
    session_tokens: u64,
    session_cost: f64,
    kill_switch_active: bool,
}

#[derive(serde::Serialize)]
struct GraphResult {
    goal: String,
    /// Room thread the plan was posted into (None for headless graph runs).
    thread_id: Option<Uuid>,
    checklist: Vec<ChecklistItem>,
    blackboard_data: std::collections::HashMap<String, String>,
    /// Lead's synthesized final answer (None for headless graph runs).
    summary: Option<String>,
}

/// Enable microphone capture on Linux. WebKitGTK disables media streams by
/// default and Wry exposes no setting for it, so `getUserMedia` fails unless we
/// flip the WebKit setting and answer the permission request. macOS/WKWebView
/// only needs the Info.plist usage string (see src-tauri/Info.plist).
#[cfg(target_os = "linux")]
fn enable_linux_media_capture(app: &tauri::AppHandle) {
    use tauri::Manager;
    let Some(window) = app.get_webview_window("main") else {
        tracing::warn!("main webview not found; microphone capture not enabled");
        return;
    };
    let _ = window.with_webview(|webview| {
        use webkit2gtk::{PermissionRequestExt, SettingsExt, WebViewExt};
        let view = webview.inner();
        if let Some(settings) = view.settings() {
            settings.set_enable_media_stream(true);
            settings.set_enable_webrtc(true);
            settings.set_enable_mediasource(true);
        }
        // Single trusted local UI: grant media permission requests.
        view.connect_permission_request(|_, request| {
            request.allow();
            true
        });
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Wayland/WebKitGTK fix for Hyprland/Omarchy (Gdk Error 71: Protocol error)
    // Must be set before any WebKit webview is created.
    // Safe to set unconditionally — WebKit checks these env vars at init.
    if std::env::var("WEBKIT_DISABLE_DMABUF_RENDERER").is_err() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
    if std::env::var("WEBKIT_DISABLE_COMPOSITING_MODE").is_err() {
        // Only set if not already set; 1 disables accelerated compositing which triggers DMABUF on Wayland
        std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
    }

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("ravenbot=debug".parse().unwrap())
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .setup(|app| {
            // Everything RAVENBOT owns lives under one folder (`~/RAVENBOT`),
            // resolved by the same helper the CLI and MCP server use. Deriving
            // it from `app.path().app_data_dir()` here is what let the two
            // drift apart into different databases.
            let db_path = ravenbot_core::default_db_path();

            // An install from before the `~/RAVENBOT` layout has its database
            // in the OS app-data dir. Move it rather than presenting an empty
            // app — the sidecar -wal / -shm files go with it or the moved
            // database is missing committed transactions.
            if let Some(legacy) = ravenbot_core::legacy_db_path() {
                match std::fs::rename(&legacy, &db_path) {
                    Ok(()) => {
                        // `-wal` / `-shm` are named after the database, not
                        // derived from it, so append rather than substitute.
                        for suffix in ["-wal", "-shm"] {
                            let mut side = legacy.clone().into_os_string();
                            side.push(suffix);
                            let _ = std::fs::remove_file(std::path::PathBuf::from(side));
                        }
                        tracing::info!(
                            from = %legacy.display(),
                            to = %db_path.display(),
                            "Moved the existing RAVENBOT database into ~/RAVENBOT"
                        );
                    }
                    Err(e) => tracing::warn!(
                        "Could not move {} to {}: {e}",
                        legacy.display(),
                        db_path.display()
                    ),
                }
            }
            
            // Initialize database + runtime inside Tokio context so Runtime::new can spawn plugin seeding
            let rt = tokio::runtime::Runtime::new().unwrap();
            let (db, runtime) = rt.block_on(async {
                let db = Database::new(&db_path).await.expect("failed to initialize database");

                // Runs are in-memory loops: anything still non-terminal at
                // startup was orphaned by a crash or quit. Close them so
                // liveness queries (e.g. the office board) tell the truth.
                let now = chrono::Utc::now().to_rfc3339();
                match sqlx::query(
                    "UPDATE runs SET state = 'failed', outcome = ?,
                        updated_at = ?, completed_at = ?
                     WHERE state IN ('planning','acting','observing','reflecting','waiting_on_user','paused')",
                )
                .bind(r#"{"Failure":{"error":"app quit while the run was active"}}"#)
                .bind(&now)
                .bind(&now)
                .execute(db.pool())
                .await
                {
                    Ok(r) if r.rows_affected() > 0 => {
                        tracing::warn!(orphaned_runs = r.rows_affected(), "Marked interrupted runs failed at startup");
                    }
                    Err(e) => tracing::warn!("Orphaned-run reconciliation failed: {e}"),
                    _ => {}
                }

                // The same reasoning for agents: a crash mid-run leaves rows
                // saying `thinking`, and the fleet then looks permanently busy
                // with nobody working.
                match ravenbot_db::queries::BotQueries::mark_all_idle(db.pool()).await {
                    Ok(n) if n > 0 => {
                        tracing::info!(bots = n, "Reset agent status at startup");
                    }
                    Ok(_) => {}
                    Err(e) => tracing::warn!("Agent status reset failed: {e}"),
                }

                let runtime = Arc::new(ravenbot_runtime::Runtime::new(db.clone()));

                // Pre-populate saved provider API keys from SQLite into Runtime's ProviderManager
                if let Ok(saved_keys) = ravenbot_db::queries::ProviderKeyQueries::list(db.pool()).await {
                    let mut manager = runtime.provider_manager().lock().await;
                    for (provider, key) in saved_keys {
                        if !key.trim().is_empty() {
                            manager.set_api_key(&provider, key);
                        }
                    }
                }
                // Pre-populate saved Ollama URL if saved
                if let Ok(Some(url)) = ravenbot_db::queries::AppSettingsQueries::get(db.pool(), "ollama_url").await {
                    if !url.trim().is_empty() {
                        let mut manager = runtime.provider_manager().lock().await;
                        manager.set_base_url("ollama", url);
                    }
                }
                // Pre-populate user-defined providers and base URL overrides (P8)
                if let Ok(rows) = ravenbot_db::queries::CustomProviderQueries::list(db.pool()).await {
                    let mut manager = runtime.provider_manager().lock().await;
                    for (id, display_name, kind, base_url, default_model, supports_tools, enabled) in rows {
                        if !enabled {
                            continue;
                        }
                        manager.register_custom(ravenbot_models::manager::CustomProviderSpec {
                            id,
                            display_name,
                            kind,
                            base_url,
                            default_model,
                            supports_tools,
                        });
                    }
                }
                if let Ok(rows) = ravenbot_db::queries::ProviderBaseUrlQueries::list(db.pool()).await {
                    let mut manager = runtime.provider_manager().lock().await;
                    for (provider, url) in rows {
                        manager.set_base_url(&provider, url);
                    }
                }

                (db, runtime)
            });
            
            // Store state
            let routine_manager = Arc::new(ravenbot_scheduler::routine::RoutineManager::new(db.pool().clone()));
            let scheduler = Arc::new(ravenbot_scheduler::Scheduler::new(
                routine_manager,
                ravenbot_scheduler::SchedulerConfig::default(),
            ));
            app.manage(AppState {
                db: db.clone(),
                runtime: runtime.clone(),
                scheduler: scheduler.clone(),
                model_discovery: Arc::new(ravenbot_models::ModelDiscovery::default()),
                webhook_base_url: std::sync::RwLock::new("http://127.0.0.1:8800".to_string()),
            });

            // Voice input on Linux needs WebKit media-stream enabled explicitly.
            #[cfg(target_os = "linux")]
            enable_linux_media_capture(app.handle());

            // Install the routine executor: due routines create a thread and really run
            let exec_app = app.handle().clone();
            let exec_db = db.clone();
            let exec_runtime = runtime.clone();
            let executor: ravenbot_scheduler::scheduler::RoutineExecutor = Arc::new(move |routine| {
                let app = exec_app.clone();
                let db = exec_db.clone();
                let runtime = exec_runtime.clone();
                Box::pin(async move {
                    execute_routine_instruction(&db, &runtime, &app, &routine).await
                })
            });
            tauri::async_runtime::block_on(scheduler.set_executor(executor.clone()));

            // Start the inbound webhook receiver (loopback only). Best-effort:
            // if the port is taken, the app still runs and the UI shows the
            // trigger as unreachable rather than refusing to start.
            let hook_pool = db.pool().clone();
            let hook_executor = executor.clone();
            let hook_state = app.state::<AppState>();
            match tauri::async_runtime::block_on(async {
                ravenbot_scheduler::WebhookServer::start(
                    hook_pool,
                    hook_executor,
                    ravenbot_scheduler::WebhookConfig::default(),
                )
                .await
            }) {
                Ok(server) => {
                    let url = server.url();
                    if let Ok(mut base) = hook_state.webhook_base_url.write() {
                        *base = url.clone();
                    }
                    // Keep the server alive for the app's lifetime.
                    app.manage(server);
                    tracing::info!("Webhook receiver listening at {}", url);
                }
                Err(e) => tracing::warn!("Webhook receiver disabled: {}", e),
            }

            // Start the scheduler tick loop
            tauri::async_runtime::spawn(async move {
                scheduler.start().await;
            });

            tracing::info!("RAVENBOT initialized, database at: {:?}", db_path);
            
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_model_catalog,
            get_configured_providers,
            set_provider_api_key,
            set_api_key,
            check_api_key,
            fetch_provider_models,
            fetch_all_provider_models,
            get_ollama_url,
            set_ollama_url,
            list_custom_providers,
            upsert_custom_provider,
            delete_custom_provider,
            test_custom_provider,
            set_provider_base_url,
            get_provider_base_url,
            probe_ollama,
            get_default_model,
            set_default_model,
            set_approval_mode,
            get_approval_mode,
            list_pending_approvals,
            list_pending_approvals_for_bot,
            decide_approval,
            list_bot_todos,
            save_office_board,
            get_office_board,
            list_pending_questions,
            list_pending_questions_for_bot,
            answer_question,
            cancel_run,
            pause_run,
            resume_run,
            get_sandbox_report,
            computer_capabilities,
            browse_workspace,
            summarize_workspace,
            default_workspace_for,
            open_workspace_in_file_manager,
            bot_desktop_status,
            start_bot_desktop,
            stop_bot_desktop,
            capture_screen,
            get_input_backend,
            transcribe_audio,
            synthesize_speech,
            list_tts_voices,
            list_engines,
            set_bot_engine,
            create_bot,
            list_bots,
            get_bot,
            update_bot,
            delete_bot,
            list_bot_contacts,
            set_bot_pinned,
            set_bot_hidden,
            mark_bot_read,
            reorder_bots,
            get_unread_counts,
            duplicate_bot,
            create_thread,
            list_threads,
            list_messages,
            rename_thread,
            delete_thread,
            search_messages,
            send_message,
            regenerate_message,
            edit_and_resend,
            execute_graph,
            draft_office_plan,
            pause_all,
            resume_all,
            get_status,
            trigger_kill_switch,
            release_kill_switch,
            get_kill_switch_status,
            create_chatroom,
            list_chatrooms,
            get_chatroom,
            add_member_to_chatroom,
            list_chatroom_members,
            get_chatroom_thread,
            send_to_chatroom,
            list_all_skills,
            sync_plugins,
            list_plugins,
            list_bot_plugins,
            toggle_bot_plugin,
            toggle_plugin_global,
            list_global_plugins,
            import_openapi_plugin,
            list_mcp_servers,
            toggle_mcp_server,
            toggle_bot_mcp_server,
            list_bot_mcp_servers,
            save_custom_mcp_server,
            delete_mcp_server,
            get_mcp_server_env,
            save_mcp_server_env,
            test_mcp_server,
            batch_assign_bot_mcp,
            batch_set_bot_mcp,
            update_chatroom,
            delete_chatroom,
            remove_chatroom_member,
            update_chatroom_member,
            create_bot_for_office,
            default_office_org,
            provision_office_org,
            draft_office_org,
            add_office_memory,
            list_office_memories,
            search_office_memories,
            get_agent_intelligence,
            get_bot_budget,
            set_bot_budget,
            reset_bot_budget,
            get_session_usage,
            list_agent_learnings,
            export_bot_bundle,
            import_bot_bundle,
            import_bot_bundle_from_file,
            create_routine,
            get_routine,
            list_routines,
            update_routine,
            delete_routine,
            get_scheduler_status,
            run_routine_now,
            enable_routine_webhook,
            disable_routine_webhook,
            get_routine_webhook_status,
            preview_team,
            fetch_and_preview_team,
            import_team,
            list_channels,
            create_channel,
            update_channel,
            delete_channel,
            set_channel_bots,
        ])
        .run(tauri::generate_context!())
        .expect("error while running ravenbot");
}

#[cfg(test)]
mod tier4_tests {
    use super::*;

    #[tokio::test]
    async fn bot_contacts_pin_hide_and_unread() {
        use ravenbot_db::queries::{BotContactQueries, BotQueries, MessageQueries, ThreadQueries};
        let pool = temp_pool().await;
        let bot = Bot::new("Contact Test", "contact state test");
        BotQueries::insert(&pool, &bot).await.unwrap();
        BotContactQueries::ensure_rows(&pool).await.unwrap();

        // A fresh bot starts read (zero unread).
        let unread_for = |rows: Vec<(uuid::Uuid, i64)>| {
            rows.into_iter().find(|(id, _)| *id == bot.id).map(|(_, c)| c).unwrap_or(0)
        };
        let counts = BotContactQueries::unread_counts(&pool).await.unwrap();
        assert_eq!(unread_for(counts), 0);

        // A new assistant reply becomes unread.
        let thread = Thread::new(bot.id, "contact thread");
        ThreadQueries::create(&pool, &thread).await.unwrap();
        MessageQueries::insert(&pool, &Message::assistant(thread.id, "hello")).await.unwrap();
        let counts = BotContactQueries::unread_counts(&pool).await.unwrap();
        assert_eq!(unread_for(counts), 1);

        // Marking read clears it.
        BotContactQueries::mark_read(&pool, bot.id).await.unwrap();
        let counts = BotContactQueries::unread_counts(&pool).await.unwrap();
        assert_eq!(unread_for(counts), 0);

        // Pin + hide round-trip.
        BotContactQueries::set_pinned(&pool, bot.id, true).await.unwrap();
        BotContactQueries::set_hidden(&pool, bot.id, true).await.unwrap();
        let rows = BotContactQueries::list(&pool).await.unwrap();
        let (_, pinned, hidden, _) = rows.iter().find(|(id, ..)| *id == bot.id).unwrap();
        assert!(*pinned && *hidden);
    }

    #[tokio::test]
    async fn channel_lead_resolution_and_mentions() {
        use ravenbot_db::queries::{BotQueries, ChannelQueries};
        let pool = temp_pool().await;
        let mut lead = Bot::new("Lead", "lead");
        lead.is_orchestrator = true;
        let worker = Bot::new("Worker", "worker");
        BotQueries::insert(&pool, &lead).await.unwrap();
        BotQueries::insert(&pool, &worker).await.unwrap();

        let channel = Channel::new("Test Channel");
        ChannelQueries::create(&pool, &channel).await.unwrap();
        ChannelQueries::set_bots(&pool, channel.id, &[lead.id, worker.id])
            .await
            .unwrap();

        // Auto lead = the orchestrator.
        let rules = serde_json::json!({});
        assert_eq!(channel_lead_bot(&pool, channel.id, &rules).await, Some(lead.id));
        // Explicit lead override wins.
        let rules = serde_json::json!({ "lead_bot_id": worker.id.to_string() });
        assert_eq!(channel_lead_bot(&pool, channel.id, &rules).await, Some(worker.id));
        // Manual mode: @mention by name.
        assert!(channel_bot_mentioned(&pool, worker.id, "hey @Worker please help").await);
        assert!(!channel_bot_mentioned(&pool, worker.id, "no mention here").await);
    }

    #[tokio::test]
    async fn chatroom_edits_persist_through_json_round_trip() {
        use ravenbot_db::queries::ChatRoomQueries;
        let pool = temp_pool().await;
        let room = ChatRoom::new("Old Office", "old desc", "custom");
        ChatRoomQueries::create(&pool, &room).await.unwrap();

        // Mimic the Tauri invoke contract: serialize → deserialize → update.
        let json = serde_json::to_value(&room).unwrap();
        let mut updated: ChatRoom = serde_json::from_value(json).unwrap();
        updated.name = "New Office".into();
        updated.description = "new desc".into();
        updated.office_template = "it-office".into();
        updated.goal = Some("Ship v1".into());
        updated.policy = Some("Be concise".into());
        updated.terms = Some("Terms text".into());
        updated.budget = Some(250.0);
        ChatRoomQueries::update(&pool, &updated).await.unwrap();

        let back = ChatRoomQueries::get(&pool, room.id).await.unwrap().unwrap();
        assert_eq!(back.name, "New Office");
        assert_eq!(back.description, "new desc");
        assert_eq!(back.office_template, "it-office");
        assert_eq!(back.goal.as_deref(), Some("Ship v1"));
        assert_eq!(back.policy.as_deref(), Some("Be concise"));
        assert_eq!(back.terms.as_deref(), Some("Terms text"));
        assert_eq!(back.budget, Some(250.0));
    }

    #[test]
    fn briefing_lists_assignees_and_dependencies() {
        let items = vec![
            ("Growth Marketer".to_string(), "Draft the campaign".to_string(), vec![]),
            ("Chief of Staff".to_string(), "Review it".to_string(), vec![0]),
        ];
        let brief = briefing_markdown(&items);
        assert!(brief.contains("1. **Growth Marketer** → Draft the campaign"));
        assert!(brief.contains("2. **Chief of Staff** → Review it"));
        assert!(brief.contains("_(after 1)_"), "dependency shown: {brief}");
    }

    fn test_role(name: &str, rank: &str, specialty: &str) -> office_org::RoleSpec {
        office_org::RoleSpec {
            name: name.to_string(),
            rank: rank.to_string(),
            specialty: specialty.to_string(),
            system_prompt: None,
            is_lead: false,
            skills: Vec::new(),
            avatar_style: None,
        }
    }

    #[test]
    fn reuses_existing_agent_by_name() {
        let mut bot = Bot::new("Coder", "Implementation");
        bot.rank = Some("Developer".into());
        bot.specialty = Some("Implementation".into());
        let used = std::collections::HashSet::new();
        let role = test_role("Coder", "Developer", "Implementation");
        let found = find_reusable_agent(&[bot.clone()], &used, &role);
        assert_eq!(found.map(|b| b.id), Some(bot.id), "exact name should be reused");
    }

    #[test]
    fn reuses_agent_by_specialty_when_name_differs() {
        let mut bot = Bot::new("Alice", "Implementation");
        bot.specialty = Some("Implementation".into());
        let used = std::collections::HashSet::new();
        let role = test_role("Coder", "Developer", "Implementation");
        let found = find_reusable_agent(&[bot.clone()], &used, &role);
        assert_eq!(found.map(|b| b.id), Some(bot.id), "specialty match should be reused");
    }

    #[test]
    fn never_reuses_an_agent_twice_in_one_run() {
        let mut bot = Bot::new("Coder", "Implementation");
        bot.specialty = Some("Implementation".into());
        let mut used = std::collections::HashSet::new();
        used.insert(bot.id);
        let role = test_role("Coder", "Developer", "Implementation");
        assert!(find_reusable_agent(&[bot], &used, &role).is_none());
    }

    #[test]
    fn creates_new_when_no_existing_agent_matches() {
        let mut bot = Bot::new("Designer", "Visual");
        bot.specialty = Some("Visual".into());
        let used = std::collections::HashSet::new();
        let role = test_role("Coder", "Developer", "Implementation");
        assert!(find_reusable_agent(&[bot], &used, &role).is_none());
    }

    #[test]
    fn truncate_chars_is_utf8_safe() {
        assert_eq!(truncate_chars("hello", 10), "hello");
        assert_eq!(truncate_chars("hello world", 5), "hello…");
        // Multi-byte chars must not panic or split a codepoint.
        assert_eq!(truncate_chars("héllo wörld", 5), "héllo…");
    }

    async fn temp_pool() -> sqlx::SqlitePool {
        let path = std::env::temp_dir().join(format!("ravenbot-tier4-{}.db", Uuid::new_v4()));
        let db = ravenbot_db::Database::new(&path).await.expect("temp db");
        db.pool().clone()
    }

    const TEAM: &str = r#"---
name: Test Team
description: tiny
bots:
  - name: Lead Bot
    title: Lead
    rank: lead
  - name: Worker Bot
    title: Worker
office:
  name: Test Office
  template: it-office
  goal: Ship it
routines:
  - name: Daily standup
    bot: Lead Bot
    schedule: "0 9 * * 1-5"
    instruction: Run standup
---
# Playbook
"#;

    #[tokio::test]
    async fn import_creates_bots_office_and_paused_routines() {
        let pool = temp_pool().await;
        let summary = import_team_into(&pool, TEAM).await.expect("import");

        assert_eq!(summary["bots"].as_array().unwrap().len(), 2);
        assert!(summary["office_id"].as_str().is_some());
        assert_eq!(summary["routines"].as_array().unwrap().len(), 1);

        // Bots actually exist.
        let bots = ravenbot_db::queries::BotQueries::list(&pool).await.unwrap();
        assert_eq!(bots.len(), 2);

        // The routine is created DISABLED (never runs unattended on import).
        let routines: Vec<(i64,)> = sqlx::query_as("SELECT is_enabled FROM routines")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(routines.len(), 1);
        assert_eq!(routines[0].0, 0, "imported routines must be paused");
    }

    #[tokio::test]
    async fn channels_roundtrip_instructions_and_roster() {
        let pool = temp_pool().await;
        // One bot to assign.
        let bot = Bot::new("ChanBot", "channel test");
        ravenbot_db::queries::BotQueries::insert(&pool, &bot).await.unwrap();

        let mut channel = Channel::new("Work");
        channel.instructions = "Be concise.".to_string();
        channel.working_folder = Some("/tmp/work".to_string());
        ravenbot_db::queries::ChannelQueries::create(&pool, &channel).await.unwrap();
        ravenbot_db::queries::ChannelQueries::set_bots(&pool, channel.id, &[bot.id]).await.unwrap();

        let list = ravenbot_db::queries::ChannelQueries::list(&pool).await.unwrap();
        assert_eq!(list.len(), 1);
        let got = &list[0];
        assert_eq!(got.name, "Work");
        assert_eq!(got.instructions, "Be concise.");
        assert_eq!(got.working_folder.as_deref(), Some("/tmp/work"));
        let bots = ravenbot_db::queries::ChannelQueries::bots(&pool, channel.id).await.unwrap();
        assert_eq!(bots, vec![bot.id]);
    }

    #[tokio::test]
    async fn webhook_secret_lookup_only_returns_enabled() {
        let pool = temp_pool().await;
        let bot = Bot::new("HookBot", "hook test");
        ravenbot_db::queries::BotQueries::insert(&pool, &bot).await.unwrap();
        let routine = Routine::new(bot.id, "Hooked", "0 9 * * *", "do work");
        sqlx::query(
            "INSERT INTO routines (id, bot_id, name, description, schedule, instruction, is_enabled, created_at, updated_at)
             VALUES (?, ?, ?, '', ?, ?, 1, datetime('now'), datetime('now'))",
        )
        .bind(routine.id.to_string())
        .bind(routine.bot_id.to_string())
        .bind(&routine.name)
        .bind(&routine.schedule)
        .bind(&routine.instruction)
        .execute(&pool)
        .await
        .unwrap();

        // Enabled → lookup finds it.
        ravenbot_db::queries::WebhookQueries::set(&pool, routine.id, Some("tok"), true)
            .await
            .unwrap();
        assert_eq!(
            ravenbot_db::queries::WebhookQueries::routine_for_secret(&pool, "tok").await.unwrap(),
            Some(routine.id)
        );
        // Disabled → lookup misses even with the secret present.
        ravenbot_db::queries::WebhookQueries::set(&pool, routine.id, Some("tok"), false)
            .await
            .unwrap();
        assert_eq!(
            ravenbot_db::queries::WebhookQueries::routine_for_secret(&pool, "tok").await.unwrap(),
            None
        );
    }
}
