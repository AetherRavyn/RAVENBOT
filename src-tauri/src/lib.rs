//! RAVENBOT - A Sovereign, Local-First, Rust-Native Multi-Agent Desktop OS
//!
//! This crate provides the Tauri shell and IPC handlers.

use ravenbot_core::*;
use ravenbot_db::Database;
use ravenbot_runtime::graph::TaskGraph;
use std::sync::Arc;
use tauri::Emitter;
use tauri::Manager;
use tauri::State;
use uuid::Uuid;

/// Application state shared across handlers
pub struct AppState {
    pub db: Database,
    pub runtime: Arc<ravenbot_runtime::Runtime>,
    pub scheduler: Arc<ravenbot_scheduler::Scheduler>,
}

// Tauri commands (IPC handlers)

/// An inline image attachment sent from the composer (paste/drop)
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ImageAttachment {
    pub name: String,
    pub mime: String,
    /// Base64-encoded image data (no data-URI prefix)
    pub data: String,
}

fn build_image_attachments(
    attachments: Vec<ImageAttachment>,
) -> Vec<ravenbot_core::Attachment> {
    attachments
        .into_iter()
        .map(|a| ravenbot_core::Attachment {
            id: Uuid::new_v4(),
            name: a.name,
            mime_type: a.mime,
            size: a.data.len() as u64,
            path: String::new(),
            data: Some(a.data),
            is_image: true,
        })
        .collect()
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
    // Local-first default: Ollama (sovereign) unless RAVENBOT_DEFAULT_PROVIDER
    // / RAVENBOT_DEFAULT_MODEL override it.
    let default_provider = std::env::var("RAVENBOT_DEFAULT_PROVIDER")
        .ok()
        .filter(|p| !p.trim().is_empty())
        .unwrap_or_else(|| "ollama".to_string());
    let default_model = std::env::var("RAVENBOT_DEFAULT_MODEL")
        .ok()
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| match default_provider.as_str() {
            "openrouter" => "anthropic/claude-3.5-sonnet".to_string(),
            "anthropic" => "claude-3-5-sonnet-20241022".to_string(),
            "openai" => "gpt-4o".to_string(),
            _ => "llama3.1".to_string(),
        });
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

#[tauri::command]
async fn create_thread(
    state: State<'_, AppState>,
    bot_id: Uuid,
    title: String,
    ephemeral: Option<bool>,
) -> Result<Thread, String> {
    let thread = if ephemeral.unwrap_or(false) {
        Thread::new_ephemeral(bot_id, title)
    } else {
        Thread::new(bot_id, title)
    };
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

    runtime.set_stream_emitter(Some(make_stream_emitter(app.clone())));
    let result = runtime.execute_run(&mut run).await;
    runtime.set_stream_emitter(None);
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
    attachments: Option<Vec<ImageAttachment>>,
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

#[tauri::command]
async fn send_message(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    thread_id: Uuid,
    content: String,
    attachments: Option<Vec<ImageAttachment>>,
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

    // Create and execute run
    let mut run = Run::new(thread.bot_id, thread_id);
    ravenbot_db::queries::RunQueries::insert(state.db.pool(), &run)
        .await
        .map_err(|e| e.to_string())?;

    // Execute the run with live streaming to the UI
    let result = execute_run_with_stream(&state, &app, &mut run).await;

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
    state.runtime.set_stream_emitter(Some(make_stream_emitter(app.clone())));
    let result = state.runtime.execute_run(run).await;
    state.runtime.set_stream_emitter(None);
    result.map_err(|e| e.to_string())
}

/// Actionable next step for a model error, keyed by failure class.
/// Auth → configure key; rate limit → wait; network → check connectivity;
/// everything else → how to fix the config.
fn error_hint(err: &str) -> &'static str {
    let e = err.to_lowercase();
    if e.contains("api key") || e.contains("auth") || e.contains("401") || e.contains("403") {
        "Fix: configure the API key for this provider in **Settings (⌘,) → API Keys**."
    } else if e.contains("rate limited") || e.contains("429") {
        "Fix: the provider is rate-limiting you — wait a moment and resend (RAVENBOT already retried once)."
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
    _orchestrator_bot_id: Uuid,
    goal: String,
    tasks: Vec<GraphTask>,
) -> Result<GraphResult, String> {
    // Create the task graph
    let mut graph = TaskGraph::new(&goal);
    
    // Map of task index to node ID for building edges
    let mut node_ids: Vec<Uuid> = Vec::new();
    
    for task in &tasks {
        let node_id = graph.add_node(task.bot_id, &task.instruction);
        node_ids.push(node_id);
        
        // Add dependencies
        for &dep_idx in &task.depends_on {
            if dep_idx < node_ids.len() {
                graph.add_edge(node_ids[dep_idx], node_id);
            }
        }
    }
    
    // Execute the graph
    let graph = Arc::new(tokio::sync::Mutex::new(graph));
    let executor = ravenbot_runtime::executor::GraphExecutor::new(
        state.runtime.clone(),
        state.db.clone(),
    );
    
    let blackboard = executor.execute(graph.clone()).await
        .map_err(|e| e.to_string())?;
    
    // Get final checklist from graph
    let graph_snapshot = graph.lock().await.clone();
    let checklist = graph_snapshot.to_checklist();
    
    Ok(GraphResult {
        goal,
        checklist,
        blackboard_data: blackboard.data,
    })
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
async fn set_api_key(
    state: State<'_, AppState>,
    provider: String,
    api_key: String,
) -> Result<(), String> {
    let mut manager = state.runtime.provider_manager().lock().await;
    manager.set_api_key(&provider, api_key.clone());
    tracing::info!("API key set for provider: {}", provider);
    Ok(())
}

#[tauri::command]
async fn check_api_key(
    state: State<'_, AppState>,
    provider: String,
) -> Result<bool, String> {
    let manager = state.runtime.provider_manager().lock().await;
    Ok(manager.has_key(&provider))
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
        "id": s.id(), "name": s.name(), "description": s.description(), "permissions": s.required_permissions().iter().map(|p| format!("{:?}", p)).collect::<Vec<_>>()
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

#[tauri::command]
async fn get_plugin_connect_link(_state: State<'_, AppState>, app_name: String) -> Result<String, String> {
    Ok(format!("inapp://connect/{} — In-app plugin '{}' is ready once enabled for a bot. No OAuth needed, fully offline.", app_name, app_name))
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
) -> Result<ChatRoom, String> {
    let mut room = ChatRoom::new(name, description, office_template);
    if let Some(url) = avatar_url { room.avatar_url = Some(url); }
    if let Some(style) = avatar_style { room.avatar_style = Some(style); }
    ravenbot_db::queries::ChatRoomQueries::create(state.db.pool(), &room).await.map_err(|e| e.to_string())?;
    Ok(room)
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
    ravenbot_db::queries::ChatRoomQueries::add_member(state.db.pool(), &member).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_chatroom_members(state: State<'_, AppState>, chatroom_id: Uuid) -> Result<Vec<ChatRoomMember>, String> {
    ravenbot_db::queries::ChatRoomQueries::list_members(state.db.pool(), chatroom_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn update_chatroom(state: State<'_, AppState>, room: ChatRoom) -> Result<(), String> {
    ravenbot_db::queries::ChatRoomQueries::update(state.db.pool(), &room).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_chatroom(state: State<'_, AppState>, chatroom_id: Uuid) -> Result<(), String> {
    ravenbot_db::queries::ChatRoomQueries::delete(state.db.pool(), chatroom_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn remove_chatroom_member(state: State<'_, AppState>, chatroom_id: Uuid, bot_id: Uuid) -> Result<(), String> {
    ravenbot_db::queries::ChatRoomQueries::remove_member(state.db.pool(), chatroom_id, bot_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn update_chatroom_member(state: State<'_, AppState>, chatroom_id: Uuid, bot_id: Uuid, rank: String, specialty: String) -> Result<(), String> {
    ravenbot_db::queries::ChatRoomQueries::update_member(state.db.pool(), chatroom_id, bot_id, &rank, &specialty).await.map_err(|e| e.to_string())
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
    bot.rank = Some(rank.clone());
    bot.specialty = Some(specialty.clone());
    ravenbot_db::queries::BotQueries::insert(state.db.pool(), &bot).await.map_err(|e| e.to_string())?;
    if let Some(cid) = chatroom_id {
        let member = ChatRoomMember { chatroom_id: cid, bot_id: bot.id, rank, specialty, joined_at: chrono::Utc::now() };
        ravenbot_db::queries::ChatRoomQueries::add_member(state.db.pool(), &member).await.map_err(|e| e.to_string())?;
    }
    Ok(bot)
}

#[tauri::command]
async fn get_chatroom_thread(state: State<'_, AppState>, chatroom_id: Uuid) -> Result<Option<Uuid>, String> {
    let row: Option<(String,)> = sqlx::query_as("SELECT thread_id FROM chatroom_threads WHERE chatroom_id = ?")
        .bind(chatroom_id.to_string()).fetch_optional(state.db.pool()).await.map_err(|e| e.to_string())?;
    Ok(row.and_then(|r| Uuid::parse_str(&r.0).ok()))
}

// ——— Office Memory & Agent Intelligence (makes agents smarter daily) ———
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

#[tauri::command]
async fn send_to_chatroom(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    chatroom_id: Uuid,
    content: String,
) -> Result<serde_json::Value, String> {
    // Load members and distribute by rank/specialty
    let members = ravenbot_db::queries::ChatRoomQueries::list_members(state.db.pool(), chatroom_id).await.map_err(|e| e.to_string())?;
    if members.is_empty() { return Err("No bots in chatroom".to_string()); }
    // Get or create group thread for chatroom
    let thread_id: Uuid = {
        let row: Option<(String,)> = sqlx::query_as("SELECT thread_id FROM chatroom_threads WHERE chatroom_id = ?")
            .bind(chatroom_id.to_string()).fetch_optional(state.db.pool()).await.map_err(|e| e.to_string())?;
        if let Some(r) = row { Uuid::parse_str(&r.0).unwrap_or_default() } else {
            let first_bot = members[0].bot_id;
            let room = ravenbot_db::queries::ChatRoomQueries::get(state.db.pool(), chatroom_id).await.map_err(|e| e.to_string())?.ok_or("Chatroom not found")?;
            let thread = Thread::new(first_bot, format!("{} — group", room.name));
            ravenbot_db::queries::ThreadQueries::create(state.db.pool(), &thread).await.map_err(|e| e.to_string())?;
            sqlx::query("INSERT INTO chatroom_threads (chatroom_id, thread_id) VALUES (?, ?)")
                .bind(chatroom_id.to_string()).bind(thread.id.to_string())
                .execute(state.db.pool()).await.map_err(|e| e.to_string())?;
            thread.id
        }
    };
    // Save user message to group thread
    let user_msg = Message::user(thread_id, &content);
    ravenbot_db::queries::MessageQueries::insert(state.db.pool(), &user_msg).await.map_err(|e| e.to_string())?;
    // Rank-based orchestration: pick distributor (highest rank = first added) to split tasks
    // Simple heuristic: if content mentions specialty keywords, route to matching specialty, else round-robin by rank
    let specialty_keywords: Vec<(&str, &str)> = vec![("design", "Designer"), ("code", "Senior Dev"), ("test", "QA"), ("infra", "DevOps"), ("research", "Tech Lead")];
    let mut tasks: Vec<(Uuid, String)> = Vec::new();
    for m in &members {
        for (kw, spec) in &specialty_keywords {
            if content.to_lowercase().contains(kw) && m.specialty.to_lowercase().contains(&spec.to_lowercase()) {
                tasks.push((m.bot_id, format!("[{} - {}] {}", m.rank, m.specialty, content)));
                break;
            }
        }
    }
    if tasks.is_empty() {
        // Distribute by rank order
        for m in &members {
            tasks.push((m.bot_id, format!("[{} - {}] {}", m.rank, m.specialty, content)));
        }
    }
    // Execute in parallel via graph — with LIVE streaming to the UI
    // (status rings, tool events, usage telemetry, citations for every node)
    let mut graph = TaskGraph::new(&content);
    let mut node_ids = Vec::new();
    for (bot_id, instr) in &tasks {
        let nid = graph.add_node(*bot_id, instr);
        node_ids.push(nid);
    }
    let graph = Arc::new(tokio::sync::Mutex::new(graph));
    let executor = ravenbot_runtime::executor::GraphExecutor::new(state.runtime.clone(), state.db.clone());

    state.runtime.set_stream_emitter(Some(make_stream_emitter(app.clone())));
    let exec_result = executor.execute(graph.clone()).await;
    state.runtime.set_stream_emitter(None);
    let _ = app.emit("agent-stream", serde_json::json!({ "kind": "done", "thread_id": thread_id.to_string() }));

    match exec_result {
        Ok(blackboard) => {
            let checklist = graph.lock().await.clone().to_checklist();
            // Post combined result to group thread as assistant message from room
            let summary = checklist.iter().map(|c| format!("{} {} → {}", match c.status { ravenbot_core::ChecklistStatus::Completed => "✓", ravenbot_core::ChecklistStatus::Failed => "✗", _ => "○" }, c.label, c.result.clone().unwrap_or_default())).collect::<Vec<_>>().join("\n");
            let summary_msg = Message::assistant(thread_id, summary.clone());
            let _ = ravenbot_db::queries::MessageQueries::insert(state.db.pool(), &summary_msg).await;
            Ok(serde_json::json!({ "thread_id": thread_id, "checklist": checklist, "blackboard": blackboard.data, "summary": summary }))
        }
        Err(err) => {
            let err_str = err.to_string();
            tracing::warn!("Chatroom execution error: {}", err_str);
            let error_summary = format!("⚠️ **Model Error:** {}\n\n{}{}", err_str, error_hint(&err_str), "\n\nYou can also run the office's bots fully offline via local Ollama models.");
            let error_msg = Message::assistant(thread_id, error_summary.clone());
            let _ = ravenbot_db::queries::MessageQueries::insert(state.db.pool(), &error_msg).await;
            Ok(serde_json::json!({ "thread_id": thread_id, "checklist": [], "blackboard": {}, "summary": error_summary }))
        }
    }
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
    checklist: Vec<ChecklistItem>,
    blackboard_data: std::collections::HashMap<String, String>,
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
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .setup(|app| {
            // Get app data directory
            let app_dir = app.path().app_data_dir().expect("failed to get app data dir");
            std::fs::create_dir_all(&app_dir).ok();
            
            let db_path = app_dir.join("ravenbot.db");
            
            // Initialize database + runtime inside Tokio context so Runtime::new can spawn plugin seeding
            let rt = tokio::runtime::Runtime::new().unwrap();
            let (db, runtime) = rt.block_on(async {
                let db = Database::new(&db_path).await.expect("failed to initialize database");
                let runtime = Arc::new(ravenbot_runtime::Runtime::new(db.clone()));
                (db, runtime)
            });
            
            // Store state
            let routine_manager = Arc::new(ravenbot_scheduler::routine::RoutineManager::new(db.pool().clone()));
            let scheduler = Arc::new(ravenbot_scheduler::Scheduler::new(
                routine_manager,
                ravenbot_scheduler::SchedulerConfig::default(),
            ));
            app.manage(AppState { db: db.clone(), runtime: runtime.clone(), scheduler: scheduler.clone() });

            // Install the routine executor: due routines create a thread and really run
            let exec_app = app.handle().clone();
            let exec_db = db.clone();
            let exec_runtime = runtime.clone();
            tauri::async_runtime::block_on(scheduler.set_executor(Arc::new(move |routine| {
                let app = exec_app.clone();
                let db = exec_db.clone();
                let runtime = exec_runtime.clone();
                Box::pin(async move {
                    execute_routine_instruction(&db, &runtime, &app, &routine).await
                })
            })));

            // Start the scheduler tick loop
            tauri::async_runtime::spawn(async move {
                scheduler.start().await;
            });

            tracing::info!("RAVENBOT initialized, database at: {:?}", db_path);
            
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            create_bot,
            list_bots,
            get_bot,
            update_bot,
            delete_bot,
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
            pause_all,
            resume_all,
            get_status,
            set_api_key,
            check_api_key,
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
            import_openapi_plugin,
            get_plugin_connect_link,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running ravenbot");
}
