//! Native Telegram connector.
//!
//! Long polling, not a public webhook: a local-first install should not need a
//! public HTTPS endpoint to receive a chat message. A one-time code pairs a
//! Telegram chat with a real RAVENBOT thread; only paired chats are allowed to
//! start runs. The bot token is stored through the keychain path with the
//! owner-only database fallback used by the other connectors.

use std::sync::Arc;
use std::time::Duration;

use ravenbot_core::{Attachment, Message, Run, Thread};
use ravenbot_db::queries::{
    AppSettingsQueries, ApprovalQueries, BotQueries, MessageQueries, QuestionQueries, RunQueries,
    TelegramLink, TelegramQueries, ThreadQueries,
};
use ravenbot_db::Database;
use ravenbot_runtime::Runtime;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const ENABLED_KEY: &str = "telegram_enabled";
pub const TOKEN_KEY: &str = "telegram_token";
pub const DEFAULT_BOT_KEY: &str = "telegram_default_bot";
pub const OFFSET_KEY: &str = "telegram_offset";
const API_BASE_ENV: &str = "RAVENBOT_TELEGRAM_API_BASE";
const DEFAULT_API_BASE: &str = "https://api.telegram.org";

#[derive(Debug, Clone, Serialize)]
pub struct TelegramStatus {
    pub enabled: bool,
    pub token_present: bool,
    pub default_bot: Option<String>,
    pub links: Vec<TelegramLink>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TelegramPairing {
    pub code: String,
    pub bot_id: Uuid,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

pub fn validate_token(raw: &str) -> Result<String, String> {
    let token = raw.trim();
    if token.len() < 20 || token.len() > 200 {
        return Err("A Telegram bot token is between 20 and 200 characters.".to_string());
    }
    if token
        .chars()
        .any(|ch| ch.is_whitespace() || ch.is_control())
    {
        return Err("The Telegram bot token contains invalid characters.".to_string());
    }
    let Some((id, secret)) = token.split_once(':') else {
        return Err("The Telegram bot token must look like <bot-id>:<secret>.".to_string());
    };
    if id.is_empty() || !id.chars().all(|ch| ch.is_ascii_digit()) || secret.len() < 20 {
        return Err("The Telegram bot token must look like <bot-id>:<secret>.".to_string());
    }
    Ok(token.to_string())
}

async fn token_marker(stored: String) -> Option<String> {
    if stored == ravenbot_db::secrets::KEYCHAIN_MARKER {
        return ravenbot_db::secrets::get_telegram_token().await;
    }
    let stored = stored.trim().to_string();
    (!stored.is_empty()).then_some(stored)
}

pub async fn read_token(db: &Database) -> Option<String> {
    let stored = AppSettingsQueries::get(db.pool(), TOKEN_KEY)
        .await
        .ok()
        .flatten()?;
    token_marker(stored).await
}

pub async fn set_token(db: &Database, raw: &str) -> Result<(), String> {
    let token = validate_token(raw)?;
    let stored = if ravenbot_db::secrets::set_telegram_token(&token).await {
        ravenbot_db::secrets::KEYCHAIN_MARKER.to_string()
    } else {
        token
    };
    AppSettingsQueries::set(db.pool(), TOKEN_KEY, &stored)
        .await
        .map_err(|error| error.to_string())?;
    // A different bot has a different update offset.
    AppSettingsQueries::set(db.pool(), OFFSET_KEY, "0")
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub async fn clear_token(db: &Database) -> Result<(), String> {
    ravenbot_db::secrets::delete_telegram_token().await;
    AppSettingsQueries::set(db.pool(), TOKEN_KEY, "")
        .await
        .map_err(|error| error.to_string())?;
    AppSettingsQueries::set(db.pool(), OFFSET_KEY, "0")
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub async fn enabled(db: &Database) -> bool {
    AppSettingsQueries::get(db.pool(), ENABLED_KEY)
        .await
        .ok()
        .flatten()
        .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
}

pub async fn status(db: &Database) -> Result<TelegramStatus, String> {
    let token_present = read_token(db).await.is_some();
    let default_bot = AppSettingsQueries::get(db.pool(), DEFAULT_BOT_KEY)
        .await
        .ok()
        .flatten()
        .filter(|value| !value.trim().is_empty());
    let links = TelegramQueries::list_links(db.pool())
        .await
        .map_err(|error| error.to_string())?;
    Ok(TelegramStatus {
        enabled: enabled(db).await,
        token_present,
        default_bot,
        links,
    })
}

pub async fn create_pairing(
    db: &Database,
    bot_id: Uuid,
    ttl: Duration,
) -> Result<TelegramPairing, String> {
    BotQueries::get(db.pool(), bot_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "That agent does not exist.".to_string())?;
    let code = format!("{:06}", Uuid::new_v4().as_u128() % 1_000_000);
    let ttl = ttl.as_secs().clamp(60, 600);
    TelegramQueries::create_pairing(db.pool(), &hash_code(&code), bot_id, ttl as i64)
        .await
        .map_err(|error| error.to_string())?;
    Ok(TelegramPairing {
        code,
        bot_id,
        expires_at: chrono::Utc::now() + chrono::Duration::seconds(ttl as i64),
    })
}

fn hash_code(code: &str) -> String {
    let digest = Sha256::digest(code.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn clean_title(value: &str) -> String {
    let title: String = value
        .trim()
        .chars()
        .filter(|ch| !ch.is_control())
        .take(80)
        .collect();
    if title.is_empty() {
        "Telegram chat".to_string()
    } else {
        title
    }
}

/// Return the bot name when a code was valid, `None` for an invalid/expired
/// code. The link and thread are created in one call so a paired chat is
/// immediately routable.
async fn pair_chat(
    db: &Database,
    code: &str,
    chat_id: &str,
    title: &str,
) -> Result<Option<String>, String> {
    let Some(bot_id) = TelegramQueries::consume_pairing(db.pool(), &hash_code(code))
        .await
        .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    let bot = BotQueries::get(db.pool(), bot_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "The paired agent no longer exists.".to_string())?;
    let title = clean_title(title);
    let thread = Thread::new(bot_id, format!("Telegram: {title}"));
    ThreadQueries::create(db.pool(), &thread)
        .await
        .map_err(|error| error.to_string())?;
    TelegramQueries::upsert_link(db.pool(), chat_id, Some(&title), bot_id, thread.id)
        .await
        .map_err(|error| error.to_string())?;
    Ok(Some(bot.name))
}

fn api_base() -> String {
    std::env::var(API_BASE_ENV)
        .ok()
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_API_BASE.to_string())
}

pub(crate) async fn run_loop(db: Database, runtime: Arc<Runtime>, events: crate::HostEvents) {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(45))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            tracing::error!(%error, "Cannot create the Telegram HTTP client");
            return;
        }
    };
    let base = api_base();
    loop {
        if !enabled(&db).await {
            tokio::time::sleep(Duration::from_secs(3)).await;
            continue;
        }
        let Some(token) = read_token(&db).await else {
            tokio::time::sleep(Duration::from_secs(3)).await;
            continue;
        };
        match poll_once(&client, &db, &runtime, &events, &base, &token).await {
            Ok(_) => {}
            Err(error) => {
                // Never log the token: the request URL contains it.
                tracing::warn!(%error, "Telegram polling failed");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

/// One long-poll iteration. `base` is injectable for tests.
pub(crate) async fn poll_once(
    client: &reqwest::Client,
    db: &Database,
    runtime: &Arc<Runtime>,
    events: &crate::HostEvents,
    base: &str,
    token: &str,
) -> Result<usize, String> {
    let offset = AppSettingsQueries::get(db.pool(), OFFSET_KEY)
        .await
        .ok()
        .flatten()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0);
    let url = format!("{base}/bot{token}/getUpdates");
    let response = client
        .post(&url)
        .json(&json!({
            "offset": offset,
            "timeout": 25,
            "allowed_updates": ["message", "callback_query"],
        }))
        .send()
        .await
        .map_err(|_| "Telegram getUpdates request failed".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "Telegram getUpdates returned HTTP {}",
            response.status().as_u16()
        ));
    }
    let body: Value = response
        .json()
        .await
        .map_err(|_| "Telegram returned a non-JSON response".to_string())?;
    if body.get("ok").and_then(|value| value.as_bool()) != Some(true) {
        return Err("Telegram rejected the getUpdates request".to_string());
    }
    let updates = body
        .get("result")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    for update in &updates {
        if let Some(message) = update.get("message") {
            if let Err(error) =
                handle_message(client, db, runtime, events, base, token, message).await
            {
                tracing::warn!(%error, "Telegram update handling failed");
            }
        }
        if let Some(callback) = update.get("callback_query") {
            if let Err(error) = handle_callback(client, db, runtime, base, token, callback).await {
                tracing::warn!(%error, "Telegram callback handling failed");
            }
        }
        if let Some(update_id) = update.get("update_id").and_then(|value| value.as_i64()) {
            let next = (update_id + 1).to_string();
            let _ = AppSettingsQueries::set(db.pool(), OFFSET_KEY, &next).await;
        }
    }
    Ok(updates.len())
}

async fn send_approval_card(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    chat_id: &str,
    card: &crate::chat_commands::PendingCard,
) -> Result<(), String> {
    use crate::chat_commands::PendingCard;
    let (text, keyboard) = match card {
        PendingCard::Approval {
            id,
            title,
            detail,
            risk,
        } => (
            format!("Approval required: {title}\n{detail}\nRisk: {risk}"),
            serde_json::json!([[
                { "text": "Approve", "callback_data": format!("approve:{id}") },
                { "text": "Deny", "callback_data": format!("deny:{id}") }
            ]]),
        ),
        PendingCard::Question {
            id,
            header,
            question,
            options,
        } => {
            let rows: Vec<Value> = options
                .iter()
                .enumerate()
                .map(|(index, option)| {
                    serde_json::json!({
                        "text": option,
                        "callback_data": format!("ans:{id}:{index}")
                    })
                })
                .collect();
            (
                format!("{header}\n{question}\n\nReply `answer {id} <text>` if you need a custom answer."),
                Value::Array(rows.into_iter().map(|button| Value::Array(vec![button])).collect()),
            )
        }
    };
    if keyboard.as_array().is_some_and(|rows| rows.is_empty()) {
        return send_text(client, base, token, chat_id, &text).await;
    }
    let response = client
        .post(format!("{base}/bot{token}/sendMessage"))
        .json(&serde_json::json!({
            "chat_id": chat_id,
            "text": text,
            "disable_web_page_preview": true,
            "reply_markup": { "inline_keyboard": keyboard }
        }))
        .send()
        .await
        .map_err(|_| "Telegram approval card request failed".to_string())?;
    if !response.status().is_success() {
        return Err("Telegram rejected the approval card.".to_string());
    }
    Ok(())
}

async fn handle_callback(
    client: &reqwest::Client,
    db: &Database,
    runtime: &Arc<Runtime>,
    base: &str,
    token: &str,
    callback: &Value,
) -> Result<(), String> {
    let callback_id = callback
        .get("id")
        .and_then(|value| value.as_str())
        .ok_or_else(|| "Telegram callback has no id.".to_string())?;
    let data = callback
        .get("data")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let chat_id = callback
        .pointer("/message/chat/id")
        .and_then(|value| value.as_i64())
        .map(|value| value.to_string())
        .ok_or_else(|| "Telegram callback has no chat id.".to_string())?;
    let message_id = callback
        .pointer("/message/message_id")
        .and_then(|value| value.as_i64());
    let Some(link) = TelegramQueries::link_for_chat(db.pool(), &chat_id)
        .await
        .map_err(|error| error.to_string())?
    else {
        return Ok(());
    };
    let thread_id = Uuid::parse_str(&link.thread_id).unwrap_or_default();
    let result = if let Some((action, id, index)) = crate::chat_commands::parse_card_action(data) {
        match action.as_str() {
            "approve" => {
                crate::chat_commands::decide_approval(db, runtime, thread_id, id, true).await
            }
            "deny" => {
                crate::chat_commands::decide_approval(db, runtime, thread_id, id, false).await
            }
            "ans" => {
                let index = index.unwrap_or(usize::MAX);
                crate::chat_commands::answer_option(db, thread_id, id, index).await
            }
            _ => Err("Unknown approval action.".to_string()),
        }
    } else {
        Err("That approval action is malformed.".to_string())
    };
    let text = match result {
        Ok(message) => message,
        Err(error) => error,
    };
    let _ = client
        .post(format!("{base}/bot{token}/answerCallbackQuery"))
        .json(&serde_json::json!({ "callback_query_id": callback_id, "text": text }))
        .send()
        .await;
    if let Some(message_id) = message_id {
        let _ = client
            .post(format!("{base}/bot{token}/editMessageReplyMarkup"))
            .json(&serde_json::json!({
                "chat_id": chat_id,
                "message_id": message_id,
                "reply_markup": { "inline_keyboard": [] }
            }))
            .send()
            .await;
    }
    Ok(())
}

async fn telegram_attachments(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    message: &Value,
) -> Vec<Attachment> {
    use crate::chat_attachments::MAX_ATTACHMENTS;
    let mut candidates: Vec<(String, String, Option<String>, Option<u64>)> = Vec::new();
    if let Some(photo) = message
        .get("photo")
        .and_then(|value| value.as_array())
        .and_then(|photos| photos.last())
    {
        if let Some(file_id) = photo.get("file_id").and_then(|value| value.as_str()) {
            candidates.push((
                file_id.to_string(),
                "photo.jpg".to_string(),
                Some("image/jpeg".to_string()),
                photo.get("file_size").and_then(|value| value.as_u64()),
            ));
        }
    }
    if let Some(document) = message.get("document") {
        if let Some(file_id) = document.get("file_id").and_then(|value| value.as_str()) {
            candidates.push((
                file_id.to_string(),
                document
                    .get("file_name")
                    .and_then(|value| value.as_str())
                    .unwrap_or("document")
                    .to_string(),
                document
                    .get("mime_type")
                    .and_then(|value| value.as_str())
                    .map(str::to_string),
                document.get("file_size").and_then(|value| value.as_u64()),
            ));
        }
    }
    for kind in ["voice", "audio", "video"] {
        if let Some(media) = message.get(kind) {
            if let Some(file_id) = media.get("file_id").and_then(|value| value.as_str()) {
                candidates.push((
                    file_id.to_string(),
                    media
                        .get("file_name")
                        .and_then(|value| value.as_str())
                        .unwrap_or(kind)
                        .to_string(),
                    media
                        .get("mime_type")
                        .and_then(|value| value.as_str())
                        .map(str::to_string),
                    media.get("file_size").and_then(|value| value.as_u64()),
                ));
            }
        }
    }
    let mut attachments = Vec::new();
    for (file_id, name, mime, size) in candidates.into_iter().take(MAX_ATTACHMENTS) {
        let Ok(response) = client
            .get(format!("{base}/bot{token}/getFile"))
            .query(&[("file_id", &file_id)])
            .send()
            .await
        else {
            continue;
        };
        let Ok(body) = response.json::<Value>().await else {
            continue;
        };
        let Some(path) = body
            .pointer("/result/file_path")
            .and_then(|value| value.as_str())
        else {
            continue;
        };
        if let Ok(attachment) = crate::chat_attachments::download(
            client,
            &format!("{base}/file/bot{token}/{path}"),
            None,
            &name,
            mime.as_deref(),
            size,
        )
        .await
        {
            attachments.push(attachment);
        }
    }
    attachments
}

async fn handle_message(
    client: &reqwest::Client,
    db: &Database,
    runtime: &Arc<Runtime>,
    events: &crate::HostEvents,
    base: &str,
    token: &str,
    message: &Value,
) -> Result<(), String> {
    if message
        .get("from")
        .and_then(|from| from.get("is_bot"))
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        return Ok(());
    }
    let chat = message.get("chat").cloned().unwrap_or(Value::Null);
    let chat_id = chat
        .get("id")
        .and_then(|value| value.as_i64())
        .map(|value| value.to_string())
        .ok_or_else(|| "Telegram message has no chat id.".to_string())?;
    let title = chat
        .get("title")
        .or_else(|| chat.get("username"))
        .or_else(|| chat.get("first_name"))
        .and_then(|value| value.as_str())
        .unwrap_or("Telegram chat");
    let from = message.get("from").cloned().unwrap_or(Value::Null);
    let actor_id = from
        .get("id")
        .and_then(|value| value.as_i64())
        .map(|value| value.to_string())
        .unwrap_or_else(|| chat_id.clone());
    let actor_name = from
        .get("username")
        .or_else(|| from.get("first_name"))
        .and_then(|value| value.as_str())
        .unwrap_or("Telegram user")
        .to_string();
    if !crate::chat_throttle::allow("telegram", &chat_id) {
        return Ok(());
    }
    let attachments = telegram_attachments(client, base, token, message).await;
    let text = message
        .get("text")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if text.is_empty() && attachments.is_empty() {
        return Ok(());
    }
    let text = if text.is_empty() {
        "(shared files)".to_string()
    } else {
        text
    };
    let command = text
        .split_whitespace()
        .next()
        .unwrap_or("")
        .split('@')
        .next()
        .unwrap_or("");

    match command {
        "/start" => {
            let code: String = text
                .split_whitespace()
                .nth(1)
                .unwrap_or("")
                .chars()
                .filter(|ch| ch.is_ascii_digit())
                .collect();
            if code.len() != 6 {
                if let Some(link) = TelegramQueries::link_for_chat(db.pool(), &chat_id)
                    .await
                    .map_err(|error| error.to_string())?
                {
                    let bot = BotQueries::get(
                        db.pool(),
                        Uuid::parse_str(&link.bot_id).unwrap_or_default(),
                    )
                    .await
                    .ok()
                    .flatten()
                    .map(|bot| bot.name)
                    .unwrap_or_else(|| "an agent".to_string());
                    send_text(
                        client,
                        base,
                        token,
                        &chat_id,
                        &format!(
                            "This chat is paired with {bot}. Send /new for a new conversation, /stop to cancel a run, or /status."
                        ),
                    )
                    .await?;
                } else {
                    send_text(
                        client,
                        base,
                        token,
                        &chat_id,
                        "Pair this chat with RAVENBOT: send /start <code> using a code from Settings → Chat apps.",
                    )
                    .await?;
                }
                return Ok(());
            }
            match pair_chat(db, &code, &chat_id, title).await? {
                Some(name) => {
                    send_text(
                        client,
                        base,
                        token,
                        &chat_id,
                        &format!(
                            "Paired with {name}. Send a message, /new for a new conversation, /stop to cancel, or /status."
                        ),
                    )
                    .await?;
                }
                None => {
                    send_text(
                        client,
                        base,
                        token,
                        &chat_id,
                        "That pairing code is invalid or expired. Generate a new one in RAVENBOT.",
                    )
                    .await?;
                }
            }
            Ok(())
        }
        "/stop" => {
            let Some(link) = TelegramQueries::link_for_chat(db.pool(), &chat_id)
                .await
                .map_err(|error| error.to_string())?
            else {
                send_text(
                    client,
                    base,
                    token,
                    &chat_id,
                    "This chat is not paired yet. Send /start <code> first.",
                )
                .await?;
                return Ok(());
            };
            let thread_id = Uuid::parse_str(&link.thread_id).unwrap_or_default();
            let active: Vec<(String,)> = sqlx::query_as(
                "SELECT id FROM runs WHERE thread_id = ? AND state NOT IN ('completed','failed','cancelled')",
            )
            .bind(thread_id.to_string())
            .fetch_all(db.pool())
            .await
            .map_err(|error| error.to_string())?;
            for (id,) in &active {
                if let Ok(id) = Uuid::parse_str(id) {
                    runtime.request_cancel(id);
                }
            }
            send_text(
                client,
                base,
                token,
                &chat_id,
                if active.is_empty() {
                    "Nothing is running."
                } else {
                    "Stopping the active run."
                },
            )
            .await?;
            Ok(())
        }
        "/new" => {
            let Some(link) = TelegramQueries::link_for_chat(db.pool(), &chat_id)
                .await
                .map_err(|error| error.to_string())?
            else {
                send_text(
                    client,
                    base,
                    token,
                    &chat_id,
                    "This chat is not paired yet. Send /start <code> first.",
                )
                .await?;
                return Ok(());
            };
            let bot_id = Uuid::parse_str(&link.bot_id).unwrap_or_default();
            let bot = BotQueries::get(db.pool(), bot_id)
                .await
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "The paired agent no longer exists.".to_string())?;
            let title = clean_title(title);
            let thread = Thread::new(bot_id, format!("Telegram: {title}"));
            ThreadQueries::create(db.pool(), &thread)
                .await
                .map_err(|error| error.to_string())?;
            TelegramQueries::upsert_link(db.pool(), &chat_id, Some(&title), bot_id, thread.id)
                .await
                .map_err(|error| error.to_string())?;
            send_text(
                client,
                base,
                token,
                &chat_id,
                &format!("Started a new conversation with {}.", bot.name),
            )
            .await?;
            Ok(())
        }
        "/status" => {
            let Some(link) = TelegramQueries::link_for_chat(db.pool(), &chat_id)
                .await
                .map_err(|error| error.to_string())?
            else {
                send_text(
                    client,
                    base,
                    token,
                    &chat_id,
                    "This chat is not paired yet. Send /start <code> first.",
                )
                .await?;
                return Ok(());
            };
            let thread_id = Uuid::parse_str(&link.thread_id).unwrap_or_default();
            let active: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM runs WHERE thread_id = ? AND state NOT IN ('completed','failed','cancelled')",
            )
            .bind(thread_id.to_string())
            .fetch_one(db.pool())
            .await
            .map_err(|error| error.to_string())?;
            let bot = BotQueries::get(db.pool(), Uuid::parse_str(&link.bot_id).unwrap_or_default())
                .await
                .ok()
                .flatten()
                .map(|bot| bot.name)
                .unwrap_or_else(|| "an agent".to_string());
            send_text(
                client,
                base,
                token,
                &chat_id,
                &format!(
                    "Paired with {bot}. {} active run(s). Send /new for a new conversation.",
                    active
                ),
            )
            .await?;
            Ok(())
        }
        "/unlink" => {
            let removed = TelegramQueries::unlink(db.pool(), &chat_id)
                .await
                .map_err(|error| error.to_string())?;
            send_text(
                client,
                base,
                token,
                &chat_id,
                if removed {
                    "This chat is no longer paired."
                } else {
                    "This chat was not paired."
                },
            )
            .await?;
            Ok(())
        }
        "approve" | "deny" | "answer" => {
            let Some(link) = TelegramQueries::link_for_chat(db.pool(), &chat_id)
                .await
                .map_err(|error| error.to_string())?
            else {
                send_text(
                    client,
                    base,
                    token,
                    &chat_id,
                    "Pair this chat before deciding approvals.",
                )
                .await?;
                return Ok(());
            };
            let thread_id = Uuid::parse_str(&link.thread_id).unwrap_or_default();
            let reply = match crate::chat_commands::handle(db, runtime, thread_id, &text).await {
                Some(Ok(message)) => message,
                Some(Err(error)) => error,
                None => "Unknown command.".to_string(),
            };
            send_text(client, base, token, &chat_id, &reply).await?;
            Ok(())
        }
        _ => {
            handle_text(
                client,
                db,
                runtime,
                events,
                base,
                token,
                &chat_id,
                &text,
                attachments,
                &actor_id,
                &actor_name,
            )
            .await
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_text(
    client: &reqwest::Client,
    db: &Database,
    runtime: &Arc<Runtime>,
    events: &crate::HostEvents,
    base: &str,
    token: &str,
    chat_id: &str,
    text: &str,
    attachments: Vec<Attachment>,
    actor_id: &str,
    actor_name: &str,
) -> Result<(), String> {
    let Some(link) = TelegramQueries::link_for_chat(db.pool(), chat_id)
        .await
        .map_err(|error| error.to_string())?
    else {
        send_text(
            client,
            base,
            token,
            chat_id,
            "This chat is not paired yet. Send /start <code> first.",
        )
        .await?;
        return Ok(());
    };
    let thread_id = Uuid::parse_str(&link.thread_id).unwrap_or_default();
    let bot_id = Uuid::parse_str(&link.bot_id).unwrap_or_default();
    let active: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM runs WHERE thread_id = ? AND state NOT IN ('completed','failed','cancelled')",
    )
    .bind(thread_id.to_string())
    .fetch_one(db.pool())
    .await
    .map_err(|error| error.to_string())?;
    if active > 0 {
        send_text(
            client,
            base,
            token,
            chat_id,
            "Still working on the previous message. Send /stop to cancel it.",
        )
        .await?;
        return Ok(());
    }
    let content = text.chars().take(20_000).collect::<String>();
    let mut user = Message::user(thread_id, content);
    user.sender_name = Some(actor_name.to_string());
    user.attachments = attachments;
    MessageQueries::insert(db.pool(), &user)
        .await
        .map_err(|error| error.to_string())?;
    let mut run = Run::new(bot_id, thread_id);
    RunQueries::insert(db.pool(), &run)
        .await
        .map_err(|error| error.to_string())?;
    let _ = crate::chat_commands::attribute_run(db, run.id, actor_id, actor_name).await;
    let _ = TelegramQueries::touch(db.pool(), chat_id).await;
    let runtime = runtime.clone();
    let db = db.clone();
    let events = events.clone();
    let client = client.clone();
    let base = base.to_string();
    let token = token.to_string();
    let chat_id = chat_id.to_string();
    tokio::spawn(async move {
        runtime.set_thread_emitter(thread_id, events.stream_emitter());
        runtime.review_approvals_for_run(run.id, true);
        // A Telegram user cannot see an Allow/Deny card. If the run parks,
        // tell the chat where to resolve it once, then let the run continue
        // waiting for the desktop/remote decision.
        let monitor = {
            let db = db.clone();
            let client = client.clone();
            let base = base.clone();
            let token = token.clone();
            let chat_id = chat_id.clone();
            let run_id = run.id;
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    if let Ok(Some(current)) = RunQueries::get(db.pool(), run_id).await {
                        if matches!(
                            current.state,
                            ravenbot_core::RunState::Completed
                                | ravenbot_core::RunState::Failed
                                | ravenbot_core::RunState::Cancelled
                        ) {
                            break;
                        }
                    }
                    let approvals = ApprovalQueries::list_pending_for_thread(db.pool(), thread_id)
                        .await
                        .unwrap_or_default();
                    let questions = QuestionQueries::list_pending_for_thread(db.pool(), thread_id)
                        .await
                        .unwrap_or_default();
                    if !approvals.is_empty() || !questions.is_empty() {
                        if let Some(card) = crate::chat_commands::pending_card(&db, thread_id).await
                        {
                            let _ =
                                send_approval_card(&client, &base, &token, &chat_id, &card).await;
                        } else {
                            let _ = send_text(
                                &client,
                                &base,
                                &token,
                                &chat_id,
                                "This run needs a decision. Open RAVENBOT or the paired remote page.",
                            )
                            .await;
                        }
                        break;
                    }
                }
            })
        };
        let result = runtime.execute_run(&mut run).await;
        monitor.abort();
        runtime.review_approvals_for_run(run.id, false);
        runtime.set_thread_emitter(thread_id, None);
        let (reply, attachments) = match result {
            Ok(()) => crate::chat_outbound::last_assistant_reply(&db, thread_id)
                .await
                .unwrap_or_else(|| {
                    (
                        "The run finished without an answer.".to_string(),
                        Vec::new(),
                    )
                }),
            Err(error) => (format!("The run failed: {error}"), Vec::new()),
        };
        if let Err(error) = send_text(&client, &base, &token, &chat_id, &reply).await {
            tracing::warn!(%error, "Telegram reply failed");
        }
        for attachment in attachments {
            if let Ok(file) = crate::chat_outbound::OutboundFile::from_attachment(&attachment) {
                if let Err(error) =
                    send_outbound_file(&client, &base, &token, &chat_id, &file).await
                {
                    tracing::warn!(%error, "Telegram attachment reply failed");
                }
            }
        }
    });
    Ok(())
}

async fn send_outbound_file(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    chat_id: &str,
    file: &crate::chat_outbound::OutboundFile,
) -> Result<(), String> {
    let method = if file.is_image {
        "sendPhoto"
    } else {
        "sendDocument"
    };
    let field = if file.is_image { "photo" } else { "document" };
    let part = reqwest::multipart::Part::bytes(file.bytes.clone())
        .file_name(file.name.clone())
        .mime_str(&file.mime_type)
        .map_err(|_| "The attachment MIME type is invalid.".to_string())?;
    let form = reqwest::multipart::Form::new()
        .text("chat_id", chat_id.to_string())
        .part(field, part);
    let response = client
        .post(format!("{base}/bot{token}/{method}"))
        .multipart(form)
        .send()
        .await
        .map_err(|_| "Telegram attachment request failed".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "Telegram returned HTTP {} for the attachment.",
            response.status().as_u16()
        ));
    }
    Ok(())
}

pub async fn send_text(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    chat_id: &str,
    text: &str,
) -> Result<(), String> {
    for chunk in split_message(text) {
        let url = format!("{base}/bot{token}/sendMessage");
        let response = client
            .post(&url)
            .json(&json!({
                "chat_id": chat_id,
                "text": chunk,
                "disable_web_page_preview": true,
            }))
            .send()
            .await
            .map_err(|_| "Telegram sendMessage request failed".to_string())?;
        if !response.status().is_success() {
            return Err(format!(
                "Telegram sendMessage returned HTTP {}",
                response.status().as_u16()
            ));
        }
    }
    Ok(())
}

/// Send to a place that is already paired with RAVENBOT.
///
/// The agent tool path uses this wrapper instead of touching a token. It
/// re-checks the connector switch and the link row, so an id that is not paired
/// cannot become an outbound destination.
pub async fn send_linked_text(
    db: &Database,
    bot_id: Uuid,
    chat_id: &str,
    text: &str,
) -> Result<(), String> {
    if !enabled(db).await {
        return Err("Telegram is not enabled in Settings → Chat apps.".to_string());
    }
    if TelegramQueries::link_for_chat(db.pool(), chat_id)
        .await
        .map_err(|error| format!("Could not read the Telegram link: {error}"))?
        .filter(|link| link.bot_id == bot_id.to_string())
        .is_none()
    {
        return Err("That Telegram chat is not paired with this agent.".to_string());
    }
    let token = read_token(db)
        .await
        .ok_or_else(|| "Telegram has no bot token. Add one in Settings → Chat apps.".to_string())?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|error| format!("Could not prepare the Telegram request: {error}"))?;
    send_text(&client, &api_base(), &token, chat_id, text).await
}

/// Telegram's hard limit is 4096 UTF-16 code units. 3500 characters leaves
/// room for surrogate pairs and is still readable.
pub fn split_message(text: &str) -> Vec<String> {
    let text = text.trim();
    if text.is_empty() {
        return vec!["(no response)".to_string()];
    }
    let mut chunks = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        current.push(ch);
        if current.chars().count() >= 3500 {
            chunks.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bot_tokens_are_validated_without_echoing_the_secret() {
        assert!(validate_token("123456:abcdefghijklmnopqrstuvwxyz").is_ok());
        assert!(validate_token("not-a-token").is_err());
        assert!(validate_token("123: short").is_err());
        assert!(validate_token("abc:abcdefghijklmnopqrstuvwxyz").is_err());
    }

    #[test]
    fn long_messages_are_split_below_the_telegram_limit() {
        let text = "a".repeat(9000);
        let chunks = split_message(&text);
        assert_eq!(chunks.len(), 3);
        assert!(chunks.iter().all(|chunk| chunk.chars().count() <= 3500));
        assert_eq!(chunks.concat(), text);
        assert_eq!(split_message("   "), vec!["(no response)"]);
    }

    #[test]
    fn a_pairing_code_is_hashed() {
        let hash = hash_code("123456");
        assert_eq!(hash.len(), 64);
        assert_ne!(hash, "123456");
    }

    #[tokio::test]
    async fn proactive_send_fails_closed_when_the_connector_is_off() {
        let path = std::env::temp_dir().join(format!("ravenbot-telegram-{}.db", Uuid::new_v4()));
        let db = Database::new(&path).await.expect("db");
        let error = send_linked_text(&db, Uuid::new_v4(), "42", "hello")
            .await
            .expect_err("disabled");
        assert!(error.contains("not enabled"));
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn a_start_update_pairs_a_chat_and_sends_a_reply() {
        use ravenbot_core::Bot;
        use ravenbot_db::queries::BotQueries;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let path = std::env::temp_dir().join(format!("ravenbot-telegram-{}.db", Uuid::new_v4()));
        let db = Database::new(&path).await.expect("db");
        let bot = Bot::new("Telegram Bot", "chat test");
        BotQueries::insert(db.pool(), &bot).await.expect("bot");
        let pairing = create_pairing(&db, bot.id, Duration::from_secs(300))
            .await
            .expect("pairing");
        let token = "123456:abcdefghijklmnopqrstuvwxyz";

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let address = listener.local_addr().expect("address");
        let update = json!({
            "ok": true,
            "result": [{
                "update_id": 100,
                "message": {
                    "message_id": 1,
                    "date": 0,
                    "chat": { "id": 42, "title": "Test chat" },
                    "from": { "id": 7, "is_bot": false },
                    "text": format!("/start {}", pairing.code),
                }
            }]
        })
        .to_string();
        let server = tokio::spawn(async move {
            for step in 0..2 {
                let (mut socket, _) = listener.accept().await.expect("accept");
                let mut buffer = [0u8; 4096];
                let _ = socket.read(&mut buffer).await;
                let body = if step == 0 {
                    update.clone()
                } else {
                    "{\"ok\":true,\"result\":{}}".to_string()
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = socket.write_all(response.as_bytes()).await;
            }
        });

        let client = reqwest::Client::new();
        let runtime = Arc::new(Runtime::new(db.clone()));
        let base = format!("http://{}", address);
        let handled = poll_once(
            &client,
            &db,
            &runtime,
            &crate::HostEvents::Headless,
            &base,
            token,
        )
        .await
        .expect("poll");
        assert_eq!(handled, 1);
        server.await.expect("server");
        let link = TelegramQueries::link_for_chat(db.pool(), "42")
            .await
            .expect("link lookup")
            .expect("paired link");
        assert_eq!(link.bot_id, bot.id.to_string());
        assert_eq!(
            AppSettingsQueries::get(db.pool(), OFFSET_KEY)
                .await
                .expect("offset")
                .as_deref(),
            Some("101")
        );
        let _ = std::fs::remove_file(&path);
    }
}
