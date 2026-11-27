//! Native Discord connector over the Gateway WebSocket.
//!
//! The bot opens an outbound gateway connection, so a local-first install needs
//! no public interactions endpoint. A one-time code pairs a channel or DM with
//! a real RAVENBOT thread. Only a direct message or an explicit bot mention can
//! start a run, and the bot token is stored through the keychain path with the
//! owner-only database fallback used by the other connectors.

use std::sync::Arc;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use ravenbot_core::{Attachment, Message, Run, Thread};
use ravenbot_db::queries::{
    AppSettingsQueries, ApprovalQueries, BotQueries, DiscordLink, DiscordQueries, MessageQueries,
    QuestionQueries, RunQueries, ThreadQueries,
};
use ravenbot_db::Database;
use ravenbot_runtime::Runtime;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};
use uuid::Uuid;

pub const ENABLED_KEY: &str = "discord_enabled";
pub const TOKEN_KEY: &str = "discord_token";
pub const DEFAULT_BOT_KEY: &str = "discord_default_bot";
const API_BASE_ENV: &str = "RAVENBOT_DISCORD_API_BASE";
const DEFAULT_API_BASE: &str = "https://discord.com/api/v10";
/// GUILDS, GUILD_MESSAGES, DIRECT_MESSAGES and MESSAGE_CONTENT. The last one is
/// privileged and must be enabled in the Discord developer portal.
const INTENTS: u64 = 1 | 512 | 4096 | 32768;

#[derive(Debug, Clone, Serialize)]
pub struct DiscordStatus {
    pub enabled: bool,
    pub token_present: bool,
    pub default_bot: Option<String>,
    pub links: Vec<DiscordLink>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiscordPairing {
    pub code: String,
    pub bot_id: Uuid,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// Gateway session state survives a reconnect so Discord can resume and replay
/// dispatches sent while the socket was down.
#[derive(Default)]
struct GatewayState {
    session_id: Option<String>,
    resume_url: Option<String>,
    sequence: Option<i64>,
    bot_id: Option<String>,
}

pub fn validate_token(raw: &str) -> Result<String, String> {
    let token = raw.trim();
    if token.len() < 50 || token.len() > 200 {
        return Err("A Discord bot token is between 50 and 200 characters.".to_string());
    }
    if token
        .chars()
        .any(|ch| ch.is_whitespace() || ch.is_control())
    {
        return Err("The Discord bot token contains invalid characters.".to_string());
    }
    if token.matches('.').count() < 2
        || !token
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err("The Discord bot token must have three dot-separated parts.".to_string());
    }
    Ok(token.to_string())
}

async fn token_marker(stored: String) -> Option<String> {
    if stored == ravenbot_db::secrets::KEYCHAIN_MARKER {
        return ravenbot_db::secrets::get_discord_token().await;
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
    let stored = if ravenbot_db::secrets::set_discord_token(&token).await {
        ravenbot_db::secrets::KEYCHAIN_MARKER.to_string()
    } else {
        token
    };
    AppSettingsQueries::set(db.pool(), TOKEN_KEY, &stored)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub async fn clear_token(db: &Database) -> Result<(), String> {
    ravenbot_db::secrets::delete_discord_token().await;
    AppSettingsQueries::set(db.pool(), TOKEN_KEY, "")
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

pub async fn status(db: &Database) -> Result<DiscordStatus, String> {
    let links = DiscordQueries::list_links(db.pool())
        .await
        .map_err(|error| error.to_string())?;
    let default_bot = AppSettingsQueries::get(db.pool(), DEFAULT_BOT_KEY)
        .await
        .ok()
        .flatten()
        .filter(|value| !value.trim().is_empty());
    Ok(DiscordStatus {
        enabled: enabled(db).await,
        token_present: read_token(db).await.is_some(),
        default_bot,
        links,
    })
}

pub async fn create_pairing(
    db: &Database,
    bot_id: Uuid,
    ttl: Duration,
) -> Result<DiscordPairing, String> {
    BotQueries::get(db.pool(), bot_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "That agent does not exist.".to_string())?;
    let code = format!("{:06}", Uuid::new_v4().as_u128() % 1_000_000);
    let ttl = ttl.as_secs().clamp(60, 600);
    DiscordQueries::create_pairing(db.pool(), &hash_code(&code), bot_id, ttl as i64)
        .await
        .map_err(|error| error.to_string())?;
    Ok(DiscordPairing {
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
        "Discord chat".to_string()
    } else {
        title
    }
}

/// Remove only this bot's mention, including the legacy `<@!id>` form, so the
/// rest of the message is the command or prompt.
pub fn normalize_text(content: &str, bot_id: &str) -> String {
    content
        .replace(&format!("<@{bot_id}>"), " ")
        .replace(&format!("<@!{bot_id}>"), " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_addressed(content: &str, guild_id: Option<&str>, bot_id: &str) -> bool {
    guild_id.is_none()
        || content.contains(&format!("<@{bot_id}>"))
        || content.contains(&format!("<@!{bot_id}>"))
}

async fn pair_channel(
    db: &Database,
    code: &str,
    channel_id: &str,
    guild_id: Option<&str>,
    title: &str,
) -> Result<Option<String>, String> {
    let Some(bot_id) = DiscordQueries::consume_pairing(db.pool(), &hash_code(code))
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
    let thread = Thread::new(bot_id, title.clone());
    ThreadQueries::create(db.pool(), &thread)
        .await
        .map_err(|error| error.to_string())?;
    DiscordQueries::upsert_link(
        db.pool(),
        channel_id,
        guild_id,
        Some(&title),
        bot_id,
        thread.id,
    )
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
            tracing::error!(%error, "Cannot create the Discord HTTP client");
            return;
        }
    };
    let base = api_base();
    let mut state = GatewayState::default();
    loop {
        if !enabled(&db).await {
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        }
        let Some(token) = read_token(&db).await else {
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        };
        match session(&client, &db, &runtime, &events, &base, &token, &mut state).await {
            Ok(()) => {
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            Err(error) => {
                tracing::warn!(%error, "Discord gateway unavailable");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

async fn gateway_url(client: &reqwest::Client, base: &str, token: &str) -> Result<String, String> {
    let response = client
        .get(format!("{base}/gateway/bot"))
        .header("Authorization", format!("Bot {token}"))
        .send()
        .await
        .map_err(|_| "Discord gateway lookup failed".to_string())?;
    let body: Value = response
        .json()
        .await
        .map_err(|_| "Discord returned a non-JSON gateway response".to_string())?;
    let url = body
        .get("url")
        .and_then(|value| value.as_str())
        .ok_or_else(|| "Discord rejected the bot token".to_string())?;
    Ok(websocket_url(url))
}

fn websocket_url(raw: &str) -> String {
    let raw = raw.trim_end_matches('/');
    if raw.contains('?') {
        raw.to_string()
    } else {
        format!("{raw}/?v=10&encoding=json")
    }
}

async fn session(
    client: &reqwest::Client,
    db: &Database,
    runtime: &Arc<Runtime>,
    events: &crate::HostEvents,
    base: &str,
    token: &str,
    state: &mut GatewayState,
) -> Result<(), String> {
    let resuming =
        state.session_id.is_some() && state.sequence.is_some() && state.resume_url.is_some();
    let url = if resuming {
        websocket_url(state.resume_url.as_deref().unwrap_or_default())
    } else {
        gateway_url(client, base, token).await?
    };
    let (mut socket, _) = tokio::time::timeout(Duration::from_secs(20), connect_async(url))
        .await
        .map_err(|_| "Discord gateway connection timed out".to_string())?
        .map_err(|_| "Could not open the Discord gateway WebSocket".to_string())?;
    // A placeholder until the Hello frame supplies the real interval. It also
    // keeps the select shape fixed while the disable check runs every 5s.
    let mut heartbeat = tokio::time::interval(Duration::from_secs(3600));
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    heartbeat.tick().await;
    let mut connector_check = tokio::time::interval(Duration::from_secs(5));
    connector_check.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    connector_check.tick().await;
    let mut awaiting_ack = false;

    loop {
        let message = tokio::select! {
            _ = connector_check.tick() => {
                if !enabled(db).await {
                    break;
                }
                continue;
            }
            _ = heartbeat.tick() => {
                if awaiting_ack {
                    return Err("Discord did not acknowledge a heartbeat".to_string());
                }
                let payload = json!({ "op": 1, "d": state.sequence });
                socket
                    .send(WsMessage::Text(payload.to_string().into()))
                    .await
                    .map_err(|_| "Could not send a Discord heartbeat".to_string())?;
                awaiting_ack = true;
                continue;
            }
            message = socket.next() => message,
        };
        let Some(message) = message else {
            break;
        };
        let message = message.map_err(|_| "Discord gateway read failed".to_string())?;
        let WsMessage::Text(text) = message else {
            if matches!(message, WsMessage::Close(_)) {
                break;
            }
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(text.as_str()) else {
            continue;
        };
        if let Some(value) = value.get("s").and_then(|value| value.as_i64()) {
            state.sequence = Some(value);
        }
        match value.get("op").and_then(|value| value.as_i64()) {
            Some(10) => {
                let interval_ms = value
                    .pointer("/d/heartbeat_interval")
                    .and_then(|value| value.as_u64())
                    .unwrap_or(41_250)
                    .max(1_000);
                let mut timer = tokio::time::interval(Duration::from_millis(interval_ms));
                timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                // Tokio's first tick is immediate. Consume it so the first
                // heartbeat is one interval away.
                timer.tick().await;
                heartbeat = timer;
                let handshake = if resuming {
                    json!({
                        "op": 6,
                        "d": {
                            "token": token,
                            "session_id": state.session_id,
                            "seq": state.sequence
                        }
                    })
                } else {
                    json!({
                        "op": 2,
                        "d": {
                            "token": token,
                            "intents": INTENTS,
                            "properties": {
                                "os": "linux",
                                "browser": "ravenbot",
                                "device": "ravenbot"
                            }
                        }
                    })
                };
                socket
                    .send(WsMessage::Text(handshake.to_string().into()))
                    .await
                    .map_err(|_| "Could not identify with the Discord gateway".to_string())?;
            }
            Some(11) => awaiting_ack = false,
            Some(0) => match value.get("t").and_then(|value| value.as_str()) {
                Some("READY") => {
                    state.bot_id = value
                        .pointer("/d/user/id")
                        .and_then(|value| value.as_str())
                        .map(str::to_string);
                    state.session_id = value
                        .pointer("/d/session_id")
                        .and_then(|value| value.as_str())
                        .map(str::to_string);
                    state.resume_url = value
                        .pointer("/d/resume_gateway_url")
                        .and_then(|value| value.as_str())
                        .map(str::to_string);
                    tracing::info!("Discord gateway connected");
                }
                Some("RESUMED") => tracing::info!("Discord gateway resumed"),
                Some("INTERACTION_CREATE") => {
                    if let Some(data) = value.get("d") {
                        if let Err(error) =
                            handle_interaction(client, db, runtime, base, token, data).await
                        {
                            tracing::warn!(%error, "Discord interaction handling failed");
                        }
                    }
                }
                Some("MESSAGE_CREATE") => {
                    if let (Some(bot_id), Some(data)) = (state.bot_id.as_deref(), value.get("d")) {
                        if let Err(error) =
                            handle_message(client, db, runtime, events, base, token, bot_id, data)
                                .await
                        {
                            tracing::warn!(%error, "Discord message handling failed");
                        }
                    }
                }
                _ => {}
            },
            Some(7) => break,
            Some(9) => {
                // Discord says whether the existing session can still resume.
                if value.get("d").and_then(|value| value.as_bool()) != Some(true) {
                    state.session_id = None;
                    state.resume_url = None;
                    state.sequence = None;
                }
                break;
            }
            _ => {}
        }
    }
    Ok(())
}

async fn send_approval_card(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    channel_id: &str,
    card: &crate::chat_commands::PendingCard,
) -> Result<(), String> {
    use crate::chat_commands::PendingCard;
    let (text, buttons) = match card {
        PendingCard::Approval {
            id,
            title,
            detail,
            risk,
        } => (
            format!("Approval required: {title}\n{detail}\nRisk: {risk}"),
            vec![
                json!({ "type": 2, "style": 3, "label": "Approve", "custom_id": format!("approve:{id}") }),
                json!({ "type": 2, "style": 4, "label": "Deny", "custom_id": format!("deny:{id}") }),
            ],
        ),
        PendingCard::Question {
            id,
            header,
            question,
            options,
        } => {
            let buttons: Vec<Value> = options
                .iter()
                .enumerate()
                .map(|(index, option)| {
                    json!({ "type": 2, "style": 1, "label": option, "custom_id": format!("ans:{id}:{index}") })
                })
                .collect();
            (
                format!("{header}\n{question}\n\nUse `answer {id} <text>` for a custom answer."),
                buttons,
            )
        }
    };
    if buttons.is_empty() {
        return send_text(client, base, token, channel_id, &text).await;
    }
    let response = client
        .post(format!("{base}/channels/{channel_id}/messages"))
        .header("Authorization", format!("Bot {token}"))
        .json(&json!({
            "content": text,
            "components": [{ "type": 1, "components": buttons }]
        }))
        .send()
        .await
        .map_err(|_| "Discord approval card request failed".to_string())?;
    if !response.status().is_success() {
        return Err("Discord rejected the approval card.".to_string());
    }
    Ok(())
}

async fn handle_interaction(
    client: &reqwest::Client,
    db: &Database,
    runtime: &Arc<Runtime>,
    base: &str,
    token: &str,
    data: &Value,
) -> Result<(), String> {
    if data.get("type").and_then(|value| value.as_u64()) != Some(3) {
        return Ok(());
    }
    let Some(channel_id) = data.get("channel_id").and_then(|value| value.as_str()) else {
        return Ok(());
    };
    let Some(custom_id) = data
        .pointer("/data/custom_id")
        .and_then(|value| value.as_str())
    else {
        return Ok(());
    };
    let Some(link) = DiscordQueries::link_for_channel(db.pool(), channel_id)
        .await
        .map_err(|error| error.to_string())?
    else {
        return Ok(());
    };
    let thread_id = Uuid::parse_str(&link.thread_id).unwrap_or_default();
    let result =
        if let Some((action, id, index)) = crate::chat_commands::parse_card_action(custom_id) {
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
    if let (Some(interaction_id), Some(interaction_token)) = (
        data.get("id").and_then(|value| value.as_str()),
        data.get("token").and_then(|value| value.as_str()),
    ) {
        let _ = client
            .post(format!(
                "{base}/interactions/{interaction_id}/{interaction_token}/callback"
            ))
            .header("Authorization", format!("Bot {token}"))
            .json(&json!({
                "type": 4,
                "data": { "content": text, "flags": 64 }
            }))
            .send()
            .await;
    }
    Ok(())
}

async fn discord_attachments(client: &reqwest::Client, data: &Value) -> Vec<Attachment> {
    use crate::chat_attachments::MAX_ATTACHMENTS;
    let mut attachments = Vec::new();
    let Some(items) = data.get("attachments").and_then(|value| value.as_array()) else {
        return attachments;
    };
    for item in items.iter().take(MAX_ATTACHMENTS) {
        let Some(url) = item.get("url").and_then(|value| value.as_str()) else {
            continue;
        };
        let name = item
            .get("filename")
            .and_then(|value| value.as_str())
            .unwrap_or("attachment");
        let mime = item.get("content_type").and_then(|value| value.as_str());
        let size = item.get("size").and_then(|value| value.as_u64());
        if let Ok(attachment) =
            crate::chat_attachments::download(client, url, None, name, mime, size).await
        {
            attachments.push(attachment);
        }
    }
    attachments
}

#[allow(clippy::too_many_arguments)]
async fn handle_message(
    client: &reqwest::Client,
    db: &Database,
    runtime: &Arc<Runtime>,
    events: &crate::HostEvents,
    base: &str,
    token: &str,
    bot_id: &str,
    data: &Value,
) -> Result<(), String> {
    if data
        .pointer("/author/bot")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        return Ok(());
    }
    let content = data
        .get("content")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let Some(channel_id) = data.get("channel_id").and_then(|value| value.as_str()) else {
        return Ok(());
    };
    let guild_id = data.get("guild_id").and_then(|value| value.as_str());
    let attachments = discord_attachments(client, data).await;
    if !is_addressed(content, guild_id, bot_id) && attachments.is_empty() {
        return Ok(());
    }
    if let Some(message_id) = data.get("id").and_then(|value| value.as_str()) {
        let claimed = DiscordQueries::claim_event(db.pool(), channel_id, message_id)
            .await
            .map_err(|error| error.to_string())?;
        if !claimed {
            return Ok(());
        }
    }
    let text = normalize_text(content, bot_id);
    if text.is_empty() && attachments.is_empty() {
        return Ok(());
    }
    let text = if text.is_empty() {
        "(shared files)".to_string()
    } else {
        text
    };
    if !crate::chat_throttle::allow("discord", channel_id) {
        return Ok(());
    }
    let actor_id = data
        .pointer("/author/id")
        .and_then(|value| value.as_str())
        .unwrap_or("discord-user")
        .to_string();
    let actor_name = data
        .pointer("/author/global_name")
        .or_else(|| data.pointer("/author/username"))
        .and_then(|value| value.as_str())
        .unwrap_or(&actor_id)
        .to_string();
    let title = if guild_id.is_none() {
        data.pointer("/author/global_name")
            .or_else(|| data.pointer("/author/username"))
            .and_then(|value| value.as_str())
            .map(|name| format!("Discord DM {}", clean_title(name)))
            .unwrap_or_else(|| "Discord DM".to_string())
    } else {
        format!(
            "Discord channel {}",
            &channel_id[..channel_id.len().min(24)]
        )
    };
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
                    token,
                    channel_id,
                    "Send `pair <code>` with a code from RAVENBOT → Chat apps.",
                )
                .await?;
                return Ok(());
            }
            match pair_channel(db, &code, channel_id, guild_id, &title).await? {
                Some(name) => {
                    send_text(
                        client,
                        base,
                        token,
                        channel_id,
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
                        token,
                        channel_id,
                        "That pairing code is invalid or expired.",
                    )
                    .await?;
                }
            }
            Ok(())
        }
        "new" => {
            let Some(link) = DiscordQueries::link_for_channel(db.pool(), channel_id)
                .await
                .map_err(|error| error.to_string())?
            else {
                send_text(
                    client,
                    base,
                    token,
                    channel_id,
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
            let title = clean_title(&title);
            let thread = Thread::new(bot_id, title.clone());
            ThreadQueries::create(db.pool(), &thread)
                .await
                .map_err(|error| error.to_string())?;
            DiscordQueries::upsert_link(
                db.pool(),
                channel_id,
                guild_id,
                Some(&title),
                bot_id,
                thread.id,
            )
            .await
            .map_err(|error| error.to_string())?;
            send_text(
                client,
                base,
                token,
                channel_id,
                &format!("Started a new conversation with {}.", bot.name),
            )
            .await?;
            Ok(())
        }
        "stop" => {
            let Some(link) = DiscordQueries::link_for_channel(db.pool(), channel_id)
                .await
                .map_err(|error| error.to_string())?
            else {
                send_text(
                    client,
                    base,
                    token,
                    channel_id,
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
                token,
                channel_id,
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
            let Some(link) = DiscordQueries::link_for_channel(db.pool(), channel_id)
                .await
                .map_err(|error| error.to_string())?
            else {
                send_text(
                    client,
                    base,
                    token,
                    channel_id,
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
                token,
                channel_id,
                &format!("Paired with {bot}. {active} active run(s)."),
            )
            .await?;
            Ok(())
        }
        "unlink" => {
            let removed = DiscordQueries::unlink(db.pool(), channel_id)
                .await
                .map_err(|error| error.to_string())?;
            send_text(
                client,
                base,
                token,
                channel_id,
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
            let Some(link) = DiscordQueries::link_for_channel(db.pool(), channel_id)
                .await
                .map_err(|error| error.to_string())?
            else {
                send_text(
                    client,
                    base,
                    token,
                    channel_id,
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
            send_text(client, base, token, channel_id, &reply).await?;
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
                channel_id,
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
    channel_id: &str,
    text: &str,
    attachments: Vec<Attachment>,
    actor_id: &str,
    actor_name: &str,
) -> Result<(), String> {
    let Some(link) = DiscordQueries::link_for_channel(db.pool(), channel_id)
        .await
        .map_err(|error| error.to_string())?
    else {
        send_text(
            client,
            base,
            token,
            channel_id,
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
            token,
            channel_id,
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
    let _ = DiscordQueries::touch(db.pool(), channel_id).await;
    let runtime = runtime.clone();
    let db = db.clone();
    let events = events.clone();
    let client = client.clone();
    let base = base.to_string();
    let token = token.to_string();
    let channel_id = channel_id.to_string();
    tokio::spawn(async move {
        runtime.set_thread_emitter(thread_id, events.stream_emitter());
        runtime.review_approvals_for_run(run.id, true);
        let monitor = {
            let db = db.clone();
            let client = client.clone();
            let base = base.clone();
            let token = token.clone();
            let channel_id = channel_id.clone();
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
                            let _ = send_approval_card(&client, &base, &token, &channel_id, &card)
                                .await;
                        } else {
                            let _ = send_text(
                                &client,
                                &base,
                                &token,
                                &channel_id,
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
        if let Err(error) = send_text(&client, &base, &token, &channel_id, &reply).await {
            tracing::warn!(%error, "Discord reply failed");
        }
        for attachment in attachments {
            if let Ok(file) = crate::chat_outbound::OutboundFile::from_attachment(&attachment) {
                if let Err(error) =
                    send_outbound_file(&client, &base, &token, &channel_id, &file).await
                {
                    tracing::warn!(%error, "Discord attachment reply failed");
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
    channel_id: &str,
    file: &crate::chat_outbound::OutboundFile,
) -> Result<(), String> {
    let part = reqwest::multipart::Part::bytes(file.bytes.clone())
        .file_name(file.name.clone())
        .mime_str(&file.mime_type)
        .map_err(|_| "The attachment MIME type is invalid.".to_string())?;
    let payload = serde_json::json!({
        "content": "",
        "attachments": [{ "id": 0, "filename": file.name }]
    })
    .to_string();
    let form = reqwest::multipart::Form::new()
        .text("payload_json", payload)
        .part("files[0]", part);
    let response = client
        .post(format!("{base}/channels/{channel_id}/messages"))
        .header("Authorization", format!("Bot {token}"))
        .multipart(form)
        .send()
        .await
        .map_err(|_| "Discord attachment request failed".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "Discord returned HTTP {} for the attachment.",
            response.status().as_u16()
        ));
    }
    Ok(())
}

async fn send_text(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    channel_id: &str,
    text: &str,
) -> Result<(), String> {
    for chunk in split_message(text) {
        let payload = json!({
            "content": chunk,
            "allowed_mentions": { "parse": [] }
        });
        let response = client
            .post(format!("{base}/channels/{channel_id}/messages"))
            .header("Authorization", format!("Bot {token}"))
            .json(&payload)
            .send()
            .await
            .map_err(|_| "Discord message request failed".to_string())?;
        if !response.status().is_success() {
            return Err(format!(
                "Discord returned HTTP {}",
                response.status().as_u16()
            ));
        }
    }
    Ok(())
}

/// Send to a Discord channel that is already paired with RAVENBOT.
pub async fn send_linked_text(
    db: &Database,
    bot_id: Uuid,
    channel_id: &str,
    text: &str,
) -> Result<(), String> {
    if !enabled(db).await {
        return Err("Discord is not enabled in Settings → Chat apps.".to_string());
    }
    if DiscordQueries::link_for_channel(db.pool(), channel_id)
        .await
        .map_err(|error| format!("Could not read the Discord link: {error}"))?
        .filter(|link| link.bot_id == bot_id.to_string())
        .is_none()
    {
        return Err("That Discord channel is not paired with this agent.".to_string());
    }
    let token = read_token(db)
        .await
        .ok_or_else(|| "Discord has no bot token. Add one in Settings → Chat apps.".to_string())?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|error| format!("Could not prepare the Discord request: {error}"))?;
    send_text(&client, &api_base(), &token, channel_id, text).await
}

/// Discord's hard limit is 2000 characters. Leave room for formatting and
/// multi-byte characters.
pub fn split_message(text: &str) -> Vec<String> {
    let text = text.trim();
    if text.is_empty() {
        return vec!["(no response)".to_string()];
    }
    let mut chunks = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        current.push(ch);
        if current.chars().count() >= 1900 {
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

    #[tokio::test]
    async fn proactive_send_fails_closed_when_the_connector_is_off() {
        let path = std::env::temp_dir().join(format!("ravenbot-discord-{}.db", Uuid::new_v4()));
        let db = Database::new(&path).await.expect("db");
        let error = send_linked_text(&db, Uuid::new_v4(), "123", "hello")
            .await
            .expect_err("disabled");
        assert!(error.contains("not enabled"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn discord_tokens_are_validated_without_echoing_the_secret() {
        let token = format!("{}.{}.{}", "A".repeat(24), "B".repeat(6), "C".repeat(38));
        assert!(validate_token(&token).is_ok());
        assert!(validate_token("not-a-token").is_err());
        assert!(validate_token(&"a".repeat(80)).is_err());
        assert!(validate_token(&format!(
            "{}.{}.{}",
            "A".repeat(30),
            "B".repeat(6),
            "C".repeat(38)
        ))
        .is_ok());
        assert!(validate_token(&format!(
            "{}.{}.{}",
            "Ä".repeat(30),
            "B".repeat(6),
            "C".repeat(38)
        ))
        .is_err());
    }

    #[test]
    fn a_bot_mention_is_removed_before_commands() {
        assert_eq!(normalize_text("<@999> pair 123456", "999"), "pair 123456");
        assert_eq!(normalize_text("<@!999> status", "999"), "status");
        assert_eq!(normalize_text("  new  ", "999"), "new");
    }

    #[test]
    fn only_direct_messages_and_mentions_are_addressed() {
        assert!(is_addressed("hello", None, "999"));
        assert!(is_addressed("<@999> status", Some("G1"), "999"));
        assert!(is_addressed("<@!999> status", Some("G1"), "999"));
        assert!(!is_addressed("hello team", Some("G1"), "999"));
    }

    #[test]
    fn discord_messages_split_below_the_limit() {
        let chunks = split_message(&"x".repeat(5000));
        assert_eq!(chunks.len(), 3);
        assert!(chunks.iter().all(|chunk| chunk.chars().count() <= 1900));
        assert_eq!(split_message("   "), vec!["(no response)"]);
    }

    #[test]
    fn pairing_codes_are_hashed() {
        assert_eq!(hash_code("123456").len(), 64);
    }

    #[tokio::test]
    async fn a_gateway_pair_event_pairs_a_channel_and_replies() {
        use ravenbot_core::Bot;
        use ravenbot_db::queries::BotQueries;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;
        use tokio_tungstenite::accept_async;

        let path = std::env::temp_dir().join(format!("ravenbot-discord-{}.db", Uuid::new_v4()));
        let db = Database::new(&path).await.expect("db");
        let bot = Bot::new("Discord Bot", "chat test");
        BotQueries::insert(db.pool(), &bot).await.expect("bot");
        let pairing = create_pairing(&db, bot.id, Duration::from_secs(300))
            .await
            .expect("pairing");
        let token = format!("{}.{}.{}", "A".repeat(24), "B".repeat(6), "C".repeat(38));

        let http = TcpListener::bind("127.0.0.1:0").await.expect("http");
        let http_address = http.local_addr().expect("http address");
        let ws = TcpListener::bind("127.0.0.1:0").await.expect("ws");
        let ws_address = ws.local_addr().expect("ws address");
        let ws_url = format!("ws://{}", ws_address);

        let http_task = tokio::spawn(async move {
            let mut paths = Vec::new();
            for _ in 0..2 {
                let (mut socket, _) = http.accept().await.expect("http accept");
                let mut buffer = [0u8; 4096];
                let read = socket.read(&mut buffer).await.expect("http read");
                let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                paths.push(request);
                let body = if paths.len() == 1 {
                    format!("{{\"url\":\"{ws_url}\"}}")
                } else {
                    "{\"id\":\"m2\"}".to_string()
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = socket.write_all(response.as_bytes()).await;
            }
            assert!(paths[0].contains("/gateway/bot"));
            assert!(paths[1].contains("/channels/D123/messages"));
            assert!(paths[1].contains("Bot "));
        });

        let code = pairing.code.clone();
        let ws_task = tokio::spawn(async move {
            let (stream, _) = ws.accept().await.expect("ws accept");
            let mut socket = accept_async(stream).await.expect("ws handshake");
            socket
                .send(WsMessage::Text(
                    "{\"op\":10,\"d\":{\"heartbeat_interval\":45000}}".into(),
                ))
                .await
                .expect("hello");
            let identify = socket
                .next()
                .await
                .expect("identify frame")
                .expect("identify");
            let identify: Value =
                serde_json::from_str(identify.to_text().expect("text")).expect("identify json");
            assert_eq!(identify.get("op").and_then(|value| value.as_u64()), Some(2));
            socket
                .send(WsMessage::Text(
                    "{\"op\":0,\"s\":1,\"t\":\"READY\",\"d\":{\"user\":{\"id\":\"999\",\"username\":\"bot\"}}}"
                        .into(),
                ))
                .await
                .expect("ready");
            let event = json!({
                "op": 0,
                "s": 2,
                "t": "MESSAGE_CREATE",
                "d": {
                    "id": "m1",
                    "channel_id": "D123",
                    "content": format!("<@999> pair {code}"),
                    "author": { "id": "7", "bot": false, "username": "tester" }
                }
            });
            socket
                .send(WsMessage::Text(event.to_string().into()))
                .await
                .expect("event");
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
            &token,
            &mut GatewayState::default(),
        )
        .await
        .expect("session");
        http_task.await.expect("http task");
        ws_task.await.expect("ws task");
        let link = DiscordQueries::link_for_channel(db.pool(), "D123")
            .await
            .expect("link lookup")
            .expect("paired channel");
        assert_eq!(link.bot_id, bot.id.to_string());
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn a_reconnect_resumes_the_gateway_session() {
        use tokio::net::TcpListener;
        use tokio_tungstenite::accept_async;

        let ws = TcpListener::bind("127.0.0.1:0").await.expect("ws");
        let ws_address = ws.local_addr().expect("ws address");
        let token = format!("{}.{}.{}", "A".repeat(24), "B".repeat(6), "C".repeat(38));
        let ws_task = tokio::spawn(async move {
            let (stream, _) = ws.accept().await.expect("ws accept");
            let mut socket = accept_async(stream).await.expect("ws handshake");
            socket
                .send(WsMessage::Text(
                    "{\"op\":10,\"d\":{\"heartbeat_interval\":45000}}".into(),
                ))
                .await
                .expect("hello");
            let resume = socket.next().await.expect("resume frame").expect("resume");
            let resume: Value =
                serde_json::from_str(resume.to_text().expect("text")).expect("resume json");
            assert_eq!(resume.get("op").and_then(|value| value.as_u64()), Some(6));
            assert_eq!(
                resume
                    .pointer("/d/session_id")
                    .and_then(|value| value.as_str()),
                Some("session-1")
            );
            assert_eq!(
                resume.pointer("/d/seq").and_then(|value| value.as_i64()),
                Some(5)
            );
            socket
                .send(WsMessage::Text(
                    "{\"op\":0,\"s\":7,\"t\":\"RESUMED\"}".into(),
                ))
                .await
                .expect("resumed");
            socket.send(WsMessage::Close(None)).await.expect("close");
        });

        let path = std::env::temp_dir().join(format!("ravenbot-discord-{}.db", Uuid::new_v4()));
        let db = Database::new(&path).await.expect("db");
        let runtime = Arc::new(Runtime::new(db.clone()));
        let mut state = GatewayState {
            session_id: Some("session-1".to_string()),
            resume_url: Some(format!("ws://{}", ws_address)),
            sequence: Some(5),
            bot_id: Some("999".to_string()),
        };
        session(
            &reqwest::Client::new(),
            &db,
            &runtime,
            &crate::HostEvents::Headless,
            "http://127.0.0.1:1",
            &token,
            &mut state,
        )
        .await
        .expect("resume session");
        ws_task.await.expect("ws task");
        assert_eq!(state.sequence, Some(7));
        assert_eq!(state.session_id.as_deref(), Some("session-1"));
        let _ = std::fs::remove_file(&path);
    }
}
