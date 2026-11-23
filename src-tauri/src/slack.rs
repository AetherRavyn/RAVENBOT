//! Native Slack connector over Socket Mode.
//!
//! Socket Mode keeps the local-first promise: Slack connects to us through the
//! app-level token's WebSocket, so no public request URL or open port is
//! required. Message events are routed to a paired channel's RAVENBOT thread
//! and replies go back through `chat.postMessage`.

use std::sync::Arc;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use ravenbot_core::{Attachment, Message, Run, Thread};
use ravenbot_db::queries::{
    AppSettingsQueries, ApprovalQueries, BotQueries, MessageQueries, QuestionQueries, RunQueries,
    SlackLink, SlackQueries, ThreadQueries,
};
use ravenbot_db::Database;
use ravenbot_runtime::Runtime;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};
use uuid::Uuid;

pub const ENABLED_KEY: &str = "slack_enabled";
pub const APP_TOKEN_KEY: &str = "slack_app_token";
pub const BOT_TOKEN_KEY: &str = "slack_bot_token";
pub const DEFAULT_BOT_KEY: &str = "slack_default_bot";
const API_BASE_ENV: &str = "RAVENBOT_SLACK_API_BASE";
const DEFAULT_API_BASE: &str = "https://slack.com/api";

#[derive(Debug, Clone, Serialize)]
pub struct SlackStatus {
    pub enabled: bool,
    pub app_token_present: bool,
    pub bot_token_present: bool,
    pub default_bot: Option<String>,
    pub links: Vec<SlackLink>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SlackPairing {
    pub code: String,
    pub bot_id: Uuid,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

pub fn validate_tokens(app_token: &str, bot_token: &str) -> Result<(String, String), String> {
    let app = app_token.trim();
    let bot = bot_token.trim();
    if !app.starts_with("xapp-") || app.len() < 20 || app.len() > 300 {
        return Err("The Slack app token must start with xapp-.".to_string());
    }
    if !(bot.starts_with("xoxb-") || bot.starts_with("xoxp-")) || bot.len() < 20 || bot.len() > 300
    {
        return Err("The Slack bot token must start with xoxb- or xoxp-.".to_string());
    }
    if app.chars().any(|ch| ch.is_whitespace() || ch.is_control())
        || bot.chars().any(|ch| ch.is_whitespace() || ch.is_control())
    {
        return Err("A Slack token contains invalid characters.".to_string());
    }
    Ok((app.to_string(), bot.to_string()))
}

async fn token_marker(
    stored: Option<String>,
    keychain: impl std::future::Future<Output = Option<String>>,
) -> Option<String> {
    let stored = stored?;
    if stored == ravenbot_db::secrets::KEYCHAIN_MARKER {
        return keychain.await;
    }
    let stored = stored.trim().to_string();
    (!stored.is_empty()).then_some(stored)
}

pub async fn app_token(db: &Database) -> Option<String> {
    let stored = AppSettingsQueries::get(db.pool(), APP_TOKEN_KEY)
        .await
        .ok()
        .flatten();
    token_marker(stored, ravenbot_db::secrets::get_slack_app_token()).await
}

pub async fn bot_token(db: &Database) -> Option<String> {
    let stored = AppSettingsQueries::get(db.pool(), BOT_TOKEN_KEY)
        .await
        .ok()
        .flatten();
    token_marker(stored, ravenbot_db::secrets::get_slack_bot_token()).await
}

pub async fn set_tokens(db: &Database, app: &str, bot: &str) -> Result<(), String> {
    let (app, bot) = validate_tokens(app, bot)?;
    let app_stored = if ravenbot_db::secrets::set_slack_app_token(&app).await {
        ravenbot_db::secrets::KEYCHAIN_MARKER.to_string()
    } else {
        app
    };
    let bot_stored = if ravenbot_db::secrets::set_slack_bot_token(&bot).await {
        ravenbot_db::secrets::KEYCHAIN_MARKER.to_string()
    } else {
        bot
    };
    AppSettingsQueries::set(db.pool(), APP_TOKEN_KEY, &app_stored)
        .await
        .map_err(|error| error.to_string())?;
    AppSettingsQueries::set(db.pool(), BOT_TOKEN_KEY, &bot_stored)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub async fn clear_tokens(db: &Database) -> Result<(), String> {
    ravenbot_db::secrets::delete_slack_app_token().await;
    ravenbot_db::secrets::delete_slack_bot_token().await;
    AppSettingsQueries::set(db.pool(), APP_TOKEN_KEY, "")
        .await
        .map_err(|error| error.to_string())?;
    AppSettingsQueries::set(db.pool(), BOT_TOKEN_KEY, "")
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

pub async fn status(db: &Database) -> Result<SlackStatus, String> {
    let links = SlackQueries::list_links(db.pool())
        .await
        .map_err(|error| error.to_string())?;
    let default_bot = AppSettingsQueries::get(db.pool(), DEFAULT_BOT_KEY)
        .await
        .ok()
        .flatten()
        .filter(|value| !value.trim().is_empty());
    Ok(SlackStatus {
        enabled: enabled(db).await,
        app_token_present: app_token(db).await.is_some(),
        bot_token_present: bot_token(db).await.is_some(),
        default_bot,
        links,
    })
}

pub async fn create_pairing(
    db: &Database,
    bot_id: Uuid,
    ttl: Duration,
) -> Result<SlackPairing, String> {
    BotQueries::get(db.pool(), bot_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "That agent does not exist.".to_string())?;
    let code = format!("{:06}", Uuid::new_v4().as_u128() % 1_000_000);
    let ttl = ttl.as_secs().clamp(60, 600);
    SlackQueries::create_pairing(db.pool(), &hash_code(&code), bot_id, ttl as i64)
        .await
        .map_err(|error| error.to_string())?;
    Ok(SlackPairing {
        code,
        bot_id,
        expires_at: chrono::Utc::now() + chrono::Duration::seconds(ttl as i64),
    })
}

fn hash_code(code: &str) -> String {
    let digest = Sha256::digest(code.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

async fn pair_channel(db: &Database, code: &str, channel: &str) -> Result<Option<String>, String> {
    let Some(bot_id) = SlackQueries::consume_pairing(db.pool(), &hash_code(code))
        .await
        .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    let bot = BotQueries::get(db.pool(), bot_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "The paired agent no longer exists.".to_string())?;
    let title = format!("Slack channel {}", &channel[..channel.len().min(24)]);
    let thread = Thread::new(bot_id, title.clone());
    ThreadQueries::create(db.pool(), &thread)
        .await
        .map_err(|error| error.to_string())?;
    SlackQueries::upsert_link(db.pool(), channel, Some(&title), bot_id, thread.id)
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
        .timeout(Duration::from_secs(30))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            tracing::error!(%error, "Cannot create the Slack HTTP client");
            return;
        }
    };
    let base = api_base();
    loop {
        if !enabled(&db).await {
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        }
        let (Some(app), Some(bot)) = (app_token(&db).await, bot_token(&db).await) else {
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        };
        match session(&client, &db, &runtime, &events, &base, &app, &bot).await {
            Ok(()) => {
                // Slack asked us to reconnect, or the socket closed cleanly.
                // A short pause avoids a tight loop if it closes immediately.
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            Err(error) => {
                tracing::warn!(%error, "Slack Socket Mode unavailable");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

async fn open_connection(
    client: &reqwest::Client,
    base: &str,
    app_token: &str,
) -> Result<String, String> {
    let response = client
        .post(format!("{base}/apps.connections.open"))
        .bearer_auth(app_token)
        .send()
        .await
        .map_err(|_| "Slack apps.connections.open request failed".to_string())?;
    let body: Value = response
        .json()
        .await
        .map_err(|_| "Slack returned a non-JSON connection response".to_string())?;
    if body.get("ok").and_then(|value| value.as_bool()) != Some(true) {
        return Err("Slack rejected the app-level token".to_string());
    }
    body.get("url")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .ok_or_else(|| "Slack returned no Socket Mode URL".to_string())
}

#[allow(clippy::too_many_arguments)]
async fn session(
    client: &reqwest::Client,
    db: &Database,
    runtime: &Arc<Runtime>,
    events: &crate::HostEvents,
    base: &str,
    app_token: &str,
    bot_token: &str,
) -> Result<(), String> {
    let url = open_connection(client, base, app_token).await?;
    let (mut socket, _) = tokio::time::timeout(Duration::from_secs(20), connect_async(url))
        .await
        .map_err(|_| "Slack Socket Mode connection timed out".to_string())?
        .map_err(|_| "Could not open the Slack Socket Mode WebSocket".to_string())?;
    loop {
        // Wake periodically so disabling the connector closes the socket
        // instead of leaving it live until Slack happens to disconnect.
        let message = match tokio::time::timeout(Duration::from_secs(5), socket.next()).await {
            Err(_) => {
                if !enabled(db).await {
                    break;
                }
                continue;
            }
            Ok(None) => break,
            Ok(Some(message)) => message.map_err(|_| "Slack WebSocket read failed".to_string())?,
        };
        let WsMessage::Text(text) = message else {
            if matches!(message, WsMessage::Close(_)) {
                break;
            }
            continue;
        };
        let value: Value = match serde_json::from_str(text.as_str()) {
            Ok(value) => value,
            Err(_) => continue,
        };
        match value.get("type").and_then(|value| value.as_str()) {
            Some("hello") => tracing::info!("Slack Socket Mode connected"),
            Some("disconnect") => break,
            Some("interactive") => {
                if let Some(envelope_id) = value.get("envelope_id").and_then(|v| v.as_str()) {
                    let ack = json!({ "envelope_id": envelope_id }).to_string();
                    let _ = socket.send(WsMessage::Text(ack.into())).await;
                }
                if let Some(payload) = value.get("payload") {
                    if let Err(error) =
                        handle_interactive(client, db, runtime, base, bot_token, payload).await
                    {
                        tracing::warn!(%error, "Slack interaction handling failed");
                    }
                }
            }
            Some("events_api") => {
                if let Some(envelope_id) = value.get("envelope_id").and_then(|v| v.as_str()) {
                    let ack = json!({ "envelope_id": envelope_id }).to_string();
                    let _ = socket.send(WsMessage::Text(ack.into())).await;
                }
                if let Some(event) = value.pointer("/payload/event") {
                    if let Err(error) =
                        handle_event(client, db, runtime, events, base, bot_token, event).await
                    {
                        tracing::warn!(%error, "Slack event handling failed");
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn is_addressed(raw_text: &str, channel_type: &str, event_type: Option<&str>) -> bool {
    channel_type == "im" || raw_text.contains("<@") || event_type == Some("app_mention")
}

fn normalize_text(text: &str) -> String {
    text.split_whitespace()
        .filter(|word| !word.starts_with("<@"))
        .collect::<Vec<_>>()
        .join(" ")
}

async fn send_approval_card(
    client: &reqwest::Client,
    base: &str,
    bot_token: &str,
    channel: &str,
    thread_ts: Option<&str>,
    card: &crate::chat_commands::PendingCard,
) -> Result<(), String> {
    use crate::chat_commands::PendingCard;
    let (text, elements) = match card {
        PendingCard::Approval {
            id,
            title,
            detail,
            risk,
        } => (
            format!("Approval required: {title}\n{detail}\nRisk: {risk}"),
            vec![
                json!({ "type": "button", "text": { "type": "plain_text", "text": "Approve" }, "style": "primary", "action_id": "approve", "value": format!("approve:{id}") }),
                json!({ "type": "button", "text": { "type": "plain_text", "text": "Deny" }, "style": "danger", "action_id": "deny", "value": format!("deny:{id}") }),
            ],
        ),
        PendingCard::Question {
            id,
            header,
            question,
            options,
        } => {
            let elements: Vec<Value> = options
                .iter()
                .enumerate()
                .map(|(index, option)| {
                    json!({ "type": "button", "text": { "type": "plain_text", "text": option }, "action_id": "answer", "value": format!("ans:{id}:{index}") })
                })
                .collect();
            (
                format!("{header}\n{question}\n\nUse `answer {id} <text>` for a custom answer."),
                elements,
            )
        }
    };
    if elements.is_empty() {
        return send_text(client, base, bot_token, channel, thread_ts, &text).await;
    }
    let blocks = json!([
        { "type": "section", "text": { "type": "mrkdwn", "text": text } },
        { "type": "actions", "elements": elements }
    ]);
    let mut payload = json!({ "channel": channel, "text": "Approval needed", "blocks": blocks });
    if let Some(thread_ts) = thread_ts {
        payload["thread_ts"] = json!(thread_ts);
    }
    let response = client
        .post(format!("{base}/chat.postMessage"))
        .bearer_auth(bot_token)
        .json(&payload)
        .send()
        .await
        .map_err(|_| "Slack approval card request failed".to_string())?;
    let body: Value = response
        .json()
        .await
        .map_err(|_| "Slack approval card response was not JSON".to_string())?;
    if body.get("ok").and_then(|value| value.as_bool()) != Some(true) {
        return Err("Slack rejected the approval card.".to_string());
    }
    Ok(())
}

async fn handle_interactive(
    client: &reqwest::Client,
    db: &Database,
    runtime: &Arc<Runtime>,
    base: &str,
    bot_token: &str,
    payload: &Value,
) -> Result<(), String> {
    if payload.get("type").and_then(|value| value.as_str()) != Some("block_actions") {
        return Ok(());
    }
    let Some(channel) = payload
        .pointer("/channel/id")
        .and_then(|value| value.as_str())
    else {
        return Ok(());
    };
    let Some(value) = payload
        .pointer("/actions/0/value")
        .and_then(|value| value.as_str())
    else {
        return Ok(());
    };
    let Some(link) = SlackQueries::link_for_channel(db.pool(), channel)
        .await
        .map_err(|error| error.to_string())?
    else {
        return Ok(());
    };
    let thread_id = Uuid::parse_str(&link.thread_id).unwrap_or_default();
    let result = if let Some((action, id, index)) = crate::chat_commands::parse_card_action(value) {
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
    let thread_ts = payload
        .pointer("/message/ts")
        .and_then(|value| value.as_str());
    send_text(client, base, bot_token, channel, thread_ts, &text).await
}

async fn slack_attachments(
    client: &reqwest::Client,
    bot_token: &str,
    event: &Value,
) -> Vec<Attachment> {
    use crate::chat_attachments::MAX_ATTACHMENTS;
    let mut attachments = Vec::new();
    let Some(files) = event.get("files").and_then(|value| value.as_array()) else {
        return attachments;
    };
    for file in files.iter().take(MAX_ATTACHMENTS) {
        let Some(url) = file
            .get("url_private_download")
            .or_else(|| file.get("url_private"))
            .and_then(|value| value.as_str())
        else {
            continue;
        };
        let name = file
            .get("name")
            .or_else(|| file.get("title"))
            .and_then(|value| value.as_str())
            .unwrap_or("attachment");
        let mime = file.get("mimetype").and_then(|value| value.as_str());
        let size = file.get("size").and_then(|value| value.as_u64());
        if let Ok(attachment) =
            crate::chat_attachments::download(client, url, Some(bot_token), name, mime, size).await
        {
            attachments.push(attachment);
        }
    }
    attachments
}

async fn handle_event(
    client: &reqwest::Client,
    db: &Database,
    runtime: &Arc<Runtime>,
    events: &crate::HostEvents,
    base: &str,
    bot_token: &str,
    event: &Value,
) -> Result<(), String> {
    if event
        .get("bot_id")
        .and_then(|value| value.as_str())
        .is_some()
        || event
            .get("subtype")
            .and_then(|value| value.as_str())
            .is_some_and(|subtype| subtype != "file_share")
        || event.get("user").and_then(|value| value.as_str()).is_none()
    {
        return Ok(());
    }
    let Some(channel) = event.get("channel").and_then(|value| value.as_str()) else {
        return Ok(());
    };
    let raw_text = event
        .get("text")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let channel_type = event
        .get("channel_type")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let event_type = event.get("type").and_then(|value| value.as_str());
    // A bot installed in a busy channel must not answer every message. Only a
    // direct message or an explicit mention starts a run.
    let attachments = slack_attachments(client, bot_token, event).await;
    if !is_addressed(raw_text, channel_type, event_type) && attachments.is_empty() {
        return Ok(());
    }
    if let Some(event_ts) = event
        .get("event_ts")
        .or_else(|| event.get("ts"))
        .and_then(|value| value.as_str())
    {
        let claimed = SlackQueries::claim_event(db.pool(), channel, event_ts)
            .await
            .map_err(|error| error.to_string())?;
        if !claimed {
            return Ok(());
        }
    }
    let actor_id = event
        .get("user")
        .and_then(|value| value.as_str())
        .unwrap_or("slack-user")
        .to_string();
    let actor_name = event
        .pointer("/user_profile/display_name")
        .or_else(|| event.pointer("/user_profile/real_name"))
        .and_then(|value| value.as_str())
        .unwrap_or(&actor_id)
        .to_string();
    let text = normalize_text(raw_text);
    if text.trim().is_empty() && attachments.is_empty() {
        return Ok(());
    }
    let text = if text.trim().is_empty() {
        "(shared files)".to_string()
    } else {
        text
    };
    if !crate::chat_throttle::allow("slack", channel) {
        return Ok(());
    }
    let thread_ts = event
        .get("thread_ts")
        .or_else(|| event.get("ts"))
        .and_then(|value| value.as_str());
    let command = text.split_whitespace().next().unwrap_or("");

    match command {
        "pair" => {
            let code = text
                .split_whitespace()
                .nth(1)
                .unwrap_or("")
                .chars()
                .filter(|ch| ch.is_ascii_digit())
                .collect::<String>();
            if code.len() != 6 {
                send_text(
                    client,
                    base,
                    bot_token,
                    channel,
                    thread_ts,
                    "Send `pair <code>` with a code from RAVENBOT → Chat apps.",
                )
                .await?;
                return Ok(());
            }
            match pair_channel(db, &code, channel).await? {
                Some(name) => {
                    send_text(
                        client,
                        base,
                        bot_token,
                        channel,
                        thread_ts,
                        &format!(
                            "Paired with {name}. Send a message, `new`, `stop`, `status` or `unlink`."
                        ),
                    )
                    .await?;
                }
                None => {
                    send_text(
                        client,
                        base,
                        bot_token,
                        channel,
                        thread_ts,
                        "That pairing code is invalid or expired.",
                    )
                    .await?;
                }
            }
            Ok(())
        }
        "new" => {
            let Some(link) = SlackQueries::link_for_channel(db.pool(), channel)
                .await
                .map_err(|error| error.to_string())?
            else {
                send_text(
                    client,
                    base,
                    bot_token,
                    channel,
                    thread_ts,
                    "Pair first with `pair <code>`.",
                )
                .await?;
                return Ok(());
            };
            let bot_id = Uuid::parse_str(&link.bot_id).unwrap_or_default();
            let bot = BotQueries::get(db.pool(), bot_id)
                .await
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "The paired agent no longer exists.".to_string())?;
            let title = format!("Slack channel {}", &channel[..channel.len().min(24)]);
            let thread = Thread::new(bot_id, title.clone());
            ThreadQueries::create(db.pool(), &thread)
                .await
                .map_err(|error| error.to_string())?;
            SlackQueries::upsert_link(db.pool(), channel, Some(&title), bot_id, thread.id)
                .await
                .map_err(|error| error.to_string())?;
            send_text(
                client,
                base,
                bot_token,
                channel,
                thread_ts,
                &format!("Started a new conversation with {}.", bot.name),
            )
            .await?;
            Ok(())
        }
        "stop" => {
            let Some(link) = SlackQueries::link_for_channel(db.pool(), channel)
                .await
                .map_err(|error| error.to_string())?
            else {
                send_text(
                    client,
                    base,
                    bot_token,
                    channel,
                    thread_ts,
                    "Pair first with `pair <code>`.",
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
                bot_token,
                channel,
                thread_ts,
                if active.is_empty() {
                    "Nothing is running."
                } else {
                    "Stopping the active run."
                },
            )
            .await?;
            Ok(())
        }
        "status" => {
            let Some(link) = SlackQueries::link_for_channel(db.pool(), channel)
                .await
                .map_err(|error| error.to_string())?
            else {
                send_text(
                    client,
                    base,
                    bot_token,
                    channel,
                    thread_ts,
                    "Pair first with `pair <code>`.",
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
                bot_token,
                channel,
                thread_ts,
                &format!("Paired with {bot}. {active} active run(s)."),
            )
            .await?;
            Ok(())
        }
        "unlink" => {
            let removed = SlackQueries::unlink(db.pool(), channel)
                .await
                .map_err(|error| error.to_string())?;
            send_text(
                client,
                base,
                bot_token,
                channel,
                thread_ts,
                if removed {
                    "This channel is no longer paired."
                } else {
                    "This channel was not paired."
                },
            )
            .await?;
            Ok(())
        }
        "approve" | "deny" | "answer" => {
            let Some(link) = SlackQueries::link_for_channel(db.pool(), channel)
                .await
                .map_err(|error| error.to_string())?
            else {
                send_text(
                    client,
                    base,
                    bot_token,
                    channel,
                    thread_ts,
                    "Pair this channel before deciding approvals.",
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
            send_text(client, base, bot_token, channel, thread_ts, &reply).await?;
            Ok(())
        }
        _ => {
            handle_text(
                client,
                db,
                runtime,
                events,
                base,
                bot_token,
                channel,
                thread_ts,
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
    bot_token: &str,
    channel: &str,
    thread_ts: Option<&str>,
    text: &str,
    attachments: Vec<Attachment>,
    actor_id: &str,
    actor_name: &str,
) -> Result<(), String> {
    let Some(link) = SlackQueries::link_for_channel(db.pool(), channel)
        .await
        .map_err(|error| error.to_string())?
    else {
        send_text(
            client,
            base,
            bot_token,
            channel,
            thread_ts,
            "Pair first with `pair <code>` using a code from RAVENBOT → Chat apps.",
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
            bot_token,
            channel,
            thread_ts,
            "Still working on the previous message. Send `stop` to cancel it.",
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
    let _ = SlackQueries::touch(db.pool(), channel).await;
    let runtime = runtime.clone();
    let db = db.clone();
    let events = events.clone();
    let client = client.clone();
    let base = base.to_string();
    let bot_token = bot_token.to_string();
    let channel = channel.to_string();
    let reply_ts = thread_ts.map(str::to_string);
    tokio::spawn(async move {
        runtime.set_thread_emitter(thread_id, events.stream_emitter());
        runtime.review_approvals_for_run(run.id, true);
        let monitor = {
            let db = db.clone();
            let client = client.clone();
            let base = base.clone();
            let bot_token = bot_token.clone();
            let channel = channel.clone();
            let reply_ts = reply_ts.clone();
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
                            let _ = send_approval_card(
                                &client,
                                &base,
                                &bot_token,
                                &channel,
                                reply_ts.as_deref(),
                                &card,
                            )
                            .await;
                        } else {
                            let _ = send_text(
                                &client,
                                &base,
                                &bot_token,
                                &channel,
                                reply_ts.as_deref(),
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
        if let Err(error) = send_text(
            &client,
            &base,
            &bot_token,
            &channel,
            reply_ts.as_deref(),
            &reply,
        )
        .await
        {
            tracing::warn!(%error, "Slack reply failed");
        }
        for attachment in attachments {
            if let Ok(file) = crate::chat_outbound::OutboundFile::from_attachment(&attachment) {
                if let Err(error) =
                    send_outbound_file(&client, &base, &bot_token, &channel, &file).await
                {
                    tracing::warn!(%error, "Slack attachment reply failed");
                }
            }
        }
    });
    Ok(())
}

async fn send_outbound_file(
    client: &reqwest::Client,
    base: &str,
    bot_token: &str,
    channel: &str,
    file: &crate::chat_outbound::OutboundFile,
) -> Result<(), String> {
    let response = client
        .post(format!("{base}/files.getUploadURLExternal"))
        .bearer_auth(bot_token)
        .form(&[
            ("filename", file.name.as_str()),
            ("length", &file.bytes.len().to_string()),
        ])
        .send()
        .await
        .map_err(|_| "Slack upload URL request failed".to_string())?;
    let body: Value = response
        .json()
        .await
        .map_err(|_| "Slack upload URL response was not JSON".to_string())?;
    if body.get("ok").and_then(|value| value.as_bool()) != Some(true) {
        return Err("Slack rejected the upload URL request.".to_string());
    }
    let Some(upload_url) = body.get("upload_url").and_then(|value| value.as_str()) else {
        return Err("Slack returned no upload URL.".to_string());
    };
    let Some(file_id) = body.get("file_id").and_then(|value| value.as_str()) else {
        return Err("Slack returned no file id.".to_string());
    };
    let upload = client
        .post(upload_url)
        .body(file.bytes.clone())
        .send()
        .await
        .map_err(|_| "Slack file upload failed".to_string())?;
    if !upload.status().is_success() {
        return Err("Slack rejected the file bytes.".to_string());
    }
    let files = serde_json::json!([{ "id": file_id, "title": file.name }]).to_string();
    let complete: Value = client
        .post(format!("{base}/files.completeUploadExternal"))
        .bearer_auth(bot_token)
        .form(&[("files", files), ("channel_id", channel.to_string())])
        .send()
        .await
        .map_err(|_| "Slack file completion request failed".to_string())?
        .json()
        .await
        .map_err(|_| "Slack file completion response was not JSON".to_string())?;
    if complete.get("ok").and_then(|value| value.as_bool()) != Some(true) {
        return Err("Slack rejected the completed file.".to_string());
    }
    Ok(())
}

async fn send_text(
    client: &reqwest::Client,
    base: &str,
    bot_token: &str,
    channel: &str,
    thread_ts: Option<&str>,
    text: &str,
) -> Result<(), String> {
    for chunk in split_message(text) {
        let mut payload = json!({ "channel": channel, "text": chunk });
        if let Some(thread_ts) = thread_ts {
            payload["thread_ts"] = json!(thread_ts);
        }
        let response = client
            .post(format!("{base}/chat.postMessage"))
            .bearer_auth(bot_token)
            .json(&payload)
            .send()
            .await
            .map_err(|_| "Slack chat.postMessage request failed".to_string())?;
        let body: Value = response
            .json()
            .await
            .map_err(|_| "Slack chat.postMessage returned non-JSON".to_string())?;
        if body.get("ok").and_then(|value| value.as_bool()) != Some(true) {
            return Err("Slack rejected the message".to_string());
        }
    }
    Ok(())
}

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
    fn slack_tokens_are_validated() {
        assert!(validate_tokens(
            "xapp-1-abcdefghijklmnopqrstuvwxyz",
            "xoxb-123-abcdefghijklmnopqrstuvwxyz"
        )
        .is_ok());
        assert!(validate_tokens("bad", "xoxb-123-abcdefghijklmnopqrstuvwxyz").is_err());
        assert!(validate_tokens("xapp-1-abcdefghijklmnopqrstuvwxyz", "bad").is_err());
    }

    #[test]
    fn only_direct_messages_and_mentions_are_addressed() {
        assert!(is_addressed("hello", "im", Some("message")));
        assert!(is_addressed(
            "<@U123> status",
            "channel",
            Some("app_mention")
        ));
        assert!(is_addressed("<@U123> status", "channel", Some("message")));
        assert!(!is_addressed("hello team", "channel", Some("message")));
    }

    #[test]
    fn a_slack_mention_prefix_is_removed_before_commands() {
        assert_eq!(normalize_text("<@U123> pair 123456"), "pair 123456");
        assert_eq!(normalize_text("  status  "), "status");
    }

    #[test]
    fn slack_messages_split_below_the_limit() {
        let chunks = split_message(&"x".repeat(8000));
        assert_eq!(chunks.len(), 3);
        assert!(chunks.iter().all(|chunk| chunk.chars().count() <= 3500));
    }

    #[test]
    fn pairing_codes_are_hashed() {
        assert_eq!(hash_code("123456").len(), 64);
    }

    #[tokio::test]
    async fn a_socket_mode_pair_event_pairs_a_channel_and_replies() {
        use ravenbot_core::Bot;
        use ravenbot_db::queries::BotQueries;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;
        use tokio_tungstenite::accept_async;

        let path = std::env::temp_dir().join(format!("ravenbot-slack-{}.db", Uuid::new_v4()));
        let db = Database::new(&path).await.expect("db");
        let bot = Bot::new("Slack Bot", "chat test");
        BotQueries::insert(db.pool(), &bot).await.expect("bot");
        let pairing = create_pairing(&db, bot.id, Duration::from_secs(300))
            .await
            .expect("pairing");
        let app = "xapp-1-abcdefghijklmnopqrstuvwxyz";
        let bot_token = "xoxb-123-abcdefghijklmnopqrstuvwxyz";

        // One HTTP endpoint serving apps.connections.open and chat.postMessage.
        let http = TcpListener::bind("127.0.0.1:0").await.expect("http");
        let http_address = http.local_addr().expect("http address");
        let ws = TcpListener::bind("127.0.0.1:0").await.expect("ws");
        let ws_address = ws.local_addr().expect("ws address");
        let ws_url = format!("ws://{}/socket", ws_address);

        let http_task = tokio::spawn(async move {
            for step in 0..2 {
                let (mut socket, _) = http.accept().await.expect("http accept");
                let mut buffer = [0u8; 4096];
                let _ = socket.read(&mut buffer).await;
                let body = if step == 0 {
                    format!("{{\"ok\":true,\"url\":\"{ws_url}\"}}")
                } else {
                    "{\"ok\":true}".to_string()
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = socket.write_all(response.as_bytes()).await;
            }
        });

        let code = pairing.code.clone();
        let ws_task = tokio::spawn(async move {
            let (stream, _) = ws.accept().await.expect("ws accept");
            let mut socket = accept_async(stream).await.expect("ws handshake");
            socket
                .send(WsMessage::Text("{\"type\":\"hello\"}".into()))
                .await
                .expect("hello");
            let event = json!({
                "type": "events_api",
                "envelope_id": "env-1",
                "payload": {
                    "event": {
                        "type": "message",
                        "user": "U123",
                        "channel": "C123",
                        "channel_type": "im",
                        "ts": "1700000000.000100",
                        "text": format!("pair {code}")
                    }
                }
            });
            socket
                .send(WsMessage::Text(event.to_string().into()))
                .await
                .expect("event");
            let ack = socket.next().await.expect("ack frame").expect("ack");
            assert!(ack.to_text().expect("text").contains("env-1"));
            socket.send(WsMessage::Close(None)).await.expect("close");
        });

        let client = reqwest::Client::new();
        let runtime = Arc::new(Runtime::new(db.clone()));
        session(
            &client,
            &db,
            &runtime,
            &crate::HostEvents::Headless,
            &format!("http://{}", http_address),
            app,
            bot_token,
        )
        .await
        .expect("session");
        http_task.await.expect("http task");
        ws_task.await.expect("ws task");
        let link = SlackQueries::link_for_channel(db.pool(), "C123")
            .await
            .expect("link lookup")
            .expect("paired channel");
        assert_eq!(link.bot_id, bot.id.to_string());
        let _ = std::fs::remove_file(&path);
    }
}
