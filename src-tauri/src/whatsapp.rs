//! Native WhatsApp Cloud API connector.
//!
//! Meta delivers inbound messages over a signed HTTPS webhook, so this module
//! also runs a small loopback listener for `/whatsapp`. A reverse proxy or
//! tunnel can expose that path publicly. Inbound payloads are verified with the
//! app secret before anything is routed, and only paired phone numbers can
//! start runs.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use hmac::{Hmac, Mac};
use ravenbot_core::{Attachment, Message, Run, Thread};
use ravenbot_db::queries::{
    AppSettingsQueries, ApprovalQueries, BotQueries, MessageQueries, QuestionQueries, RunQueries,
    ThreadQueries, WhatsAppLink, WhatsAppQueries,
};
use ravenbot_db::Database;
use ravenbot_runtime::Runtime;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use uuid::Uuid;

pub const ENABLED_KEY: &str = "whatsapp_enabled";
pub const TOKEN_KEY: &str = "whatsapp_token";
pub const APP_SECRET_KEY: &str = "whatsapp_app_secret";
pub const VERIFY_TOKEN_KEY: &str = "whatsapp_verify_token";
pub const PHONE_ID_KEY: &str = "whatsapp_phone_number_id";
pub const DEFAULT_BOT_KEY: &str = "whatsapp_default_bot";
const API_BASE_ENV: &str = "RAVENBOT_WHATSAPP_API_BASE";
const DEFAULT_API_BASE: &str = "https://graph.facebook.com/v21.0";

#[derive(Debug, Clone, Serialize)]
pub struct WhatsAppStatus {
    pub enabled: bool,
    pub token_present: bool,
    pub app_secret_present: bool,
    pub verify_token_present: bool,
    pub phone_number_id: Option<String>,
    pub default_bot: Option<String>,
    pub links: Vec<WhatsAppLink>,
    pub local_webhook_url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WhatsAppPairing {
    pub code: String,
    pub bot_id: Uuid,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone)]
struct WhatsAppConfig {
    token: String,
    phone_number_id: String,
    api_base: String,
}

pub fn validate_token(raw: &str) -> Result<String, String> {
    let token = raw.trim();
    if token.len() < 20 || token.len() > 2_048 {
        return Err("A WhatsApp access token is between 20 and 2048 characters.".to_string());
    }
    if token
        .chars()
        .any(|ch| ch.is_whitespace() || ch.is_control())
    {
        return Err("The WhatsApp access token contains invalid characters.".to_string());
    }
    Ok(token.to_string())
}

pub fn validate_app_secret(raw: &str) -> Result<String, String> {
    let secret = raw.trim();
    if secret.len() < 16 || secret.len() > 128 {
        return Err("The WhatsApp app secret is between 16 and 128 characters.".to_string());
    }
    if secret
        .chars()
        .any(|ch| ch.is_whitespace() || ch.is_control())
    {
        return Err("The WhatsApp app secret contains invalid characters.".to_string());
    }
    Ok(secret.to_string())
}

pub fn validate_verify_token(raw: &str) -> Result<String, String> {
    let token = raw.trim();
    if token.len() < 8 || token.len() > 128 {
        return Err("The WhatsApp verify token is between 8 and 128 characters.".to_string());
    }
    if token
        .chars()
        .any(|ch| ch.is_whitespace() || ch.is_control())
    {
        return Err("The WhatsApp verify token contains invalid characters.".to_string());
    }
    Ok(token.to_string())
}

pub fn validate_phone_number_id(raw: &str) -> Result<String, String> {
    let id = raw.trim();
    if id.len() < 5 || id.len() > 32 || !id.chars().all(|ch| ch.is_ascii_digit()) {
        return Err("The WhatsApp phone number id must be 5 to 32 digits.".to_string());
    }
    Ok(id.to_string())
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

async fn read_secret(
    db: &Database,
    key: &str,
    keychain: impl std::future::Future<Output = Option<String>>,
) -> Option<String> {
    let stored = AppSettingsQueries::get(db.pool(), key).await.ok().flatten();
    token_marker(stored, keychain).await
}

async fn store_secret(
    db: &Database,
    key: &str,
    value: &str,
    set: impl std::future::Future<Output = bool>,
) -> Result<(), String> {
    let stored = if set.await {
        ravenbot_db::secrets::KEYCHAIN_MARKER.to_string()
    } else {
        value.to_string()
    };
    AppSettingsQueries::set(db.pool(), key, &stored)
        .await
        .map_err(|error| error.to_string())
}

async fn clear_secret(
    db: &Database,
    key: &str,
    remove: impl std::future::Future<Output = ()>,
) -> Result<(), String> {
    remove.await;
    AppSettingsQueries::set(db.pool(), key, "")
        .await
        .map_err(|error| error.to_string())
}

pub async fn read_token(db: &Database) -> Option<String> {
    read_secret(db, TOKEN_KEY, ravenbot_db::secrets::get_whatsapp_token()).await
}

pub async fn set_token(db: &Database, raw: &str) -> Result<(), String> {
    let token = validate_token(raw)?;
    store_secret(
        db,
        TOKEN_KEY,
        &token,
        ravenbot_db::secrets::set_whatsapp_token(&token),
    )
    .await
}

pub async fn clear_token(db: &Database) -> Result<(), String> {
    clear_secret(db, TOKEN_KEY, ravenbot_db::secrets::delete_whatsapp_token()).await
}

pub async fn read_app_secret(db: &Database) -> Option<String> {
    read_secret(
        db,
        APP_SECRET_KEY,
        ravenbot_db::secrets::get_whatsapp_app_secret(),
    )
    .await
}

pub async fn set_app_secret(db: &Database, raw: &str) -> Result<(), String> {
    let secret = validate_app_secret(raw)?;
    store_secret(
        db,
        APP_SECRET_KEY,
        &secret,
        ravenbot_db::secrets::set_whatsapp_app_secret(&secret),
    )
    .await
}

pub async fn clear_app_secret(db: &Database) -> Result<(), String> {
    clear_secret(
        db,
        APP_SECRET_KEY,
        ravenbot_db::secrets::delete_whatsapp_app_secret(),
    )
    .await
}

pub async fn read_verify_token(db: &Database) -> Option<String> {
    read_secret(
        db,
        VERIFY_TOKEN_KEY,
        ravenbot_db::secrets::get_whatsapp_verify_token(),
    )
    .await
}

pub async fn set_verify_token(db: &Database, raw: &str) -> Result<(), String> {
    let token = validate_verify_token(raw)?;
    store_secret(
        db,
        VERIFY_TOKEN_KEY,
        &token,
        ravenbot_db::secrets::set_whatsapp_verify_token(&token),
    )
    .await
}

pub async fn clear_verify_token(db: &Database) -> Result<(), String> {
    clear_secret(
        db,
        VERIFY_TOKEN_KEY,
        ravenbot_db::secrets::delete_whatsapp_verify_token(),
    )
    .await
}

pub async fn read_phone_number_id(db: &Database) -> Option<String> {
    AppSettingsQueries::get(db.pool(), PHONE_ID_KEY)
        .await
        .ok()
        .flatten()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub async fn set_phone_number_id(db: &Database, raw: &str) -> Result<(), String> {
    let id = validate_phone_number_id(raw)?;
    AppSettingsQueries::set(db.pool(), PHONE_ID_KEY, &id)
        .await
        .map_err(|error| error.to_string())
}

pub async fn clear_phone_number_id(db: &Database) -> Result<(), String> {
    AppSettingsQueries::set(db.pool(), PHONE_ID_KEY, "")
        .await
        .map_err(|error| error.to_string())
}

pub async fn enabled(db: &Database) -> bool {
    AppSettingsQueries::get(db.pool(), ENABLED_KEY)
        .await
        .ok()
        .flatten()
        .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
}

pub(crate) fn api_base() -> String {
    std::env::var(API_BASE_ENV)
        .ok()
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_API_BASE.to_string())
}

pub async fn status(db: &Database) -> Result<WhatsAppStatus, String> {
    let links = WhatsAppQueries::list_links(db.pool())
        .await
        .map_err(|error| error.to_string())?;
    let default_bot = AppSettingsQueries::get(db.pool(), DEFAULT_BOT_KEY)
        .await
        .ok()
        .flatten()
        .filter(|value| !value.trim().is_empty());
    Ok(WhatsAppStatus {
        enabled: enabled(db).await,
        token_present: read_token(db).await.is_some(),
        app_secret_present: read_app_secret(db).await.is_some(),
        verify_token_present: read_verify_token(db).await.is_some(),
        phone_number_id: read_phone_number_id(db).await,
        default_bot,
        links,
        local_webhook_url: crate::chat_webhook::local_url(),
    })
}

pub async fn create_pairing(
    db: &Database,
    bot_id: Uuid,
    ttl: Duration,
) -> Result<WhatsAppPairing, String> {
    BotQueries::get(db.pool(), bot_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "That agent does not exist.".to_string())?;
    let code = format!("{:06}", Uuid::new_v4().as_u128() % 1_000_000);
    let ttl = ttl.as_secs().clamp(60, 600);
    WhatsAppQueries::create_pairing(db.pool(), &hash_code(&code), bot_id, ttl as i64)
        .await
        .map_err(|error| error.to_string())?;
    Ok(WhatsAppPairing {
        code,
        bot_id,
        expires_at: chrono::Utc::now() + chrono::Duration::seconds(ttl as i64),
    })
}

fn hash_code(code: &str) -> String {
    let digest = Sha256::digest(code.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn clean_name(value: &str) -> String {
    let name: String = value
        .trim()
        .chars()
        .filter(|ch| !ch.is_control())
        .take(80)
        .collect();
    if name.is_empty() {
        "WhatsApp chat".to_string()
    } else {
        name
    }
}

pub(crate) fn constant_time_eq(left: &str, right: &str) -> bool {
    left.as_bytes().ct_eq(right.as_bytes()).into()
}

/// Verify Meta's `X-Hub-Signature-256` header against the raw request body.
pub fn verify_signature(app_secret: &str, body: &[u8], header: &str) -> bool {
    let Some(signature) = header.strip_prefix("sha256=") else {
        return false;
    };
    let Some(signature) = hex_decode(signature) else {
        return false;
    };
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(app_secret.as_bytes()) else {
        return false;
    };
    mac.update(body);
    let expected = mac.finalize().into_bytes();
    signature.len() == expected.len() && signature.ct_eq(&expected).into()
}

fn hex_decode(value: &str) -> Option<Vec<u8>> {
    if !value.len().is_multiple_of(2) || !value.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).ok())
        .collect()
}

async fn load_config(db: &Database, base: &str) -> Option<WhatsAppConfig> {
    let token = read_token(db).await?;
    let phone_number_id = read_phone_number_id(db).await?;
    Some(WhatsAppConfig {
        token,
        phone_number_id,
        api_base: base.to_string(),
    })
}

async fn pair_number(
    db: &Database,
    code: &str,
    wa_id: &str,
    display_name: Option<&str>,
) -> Result<Option<String>, String> {
    let Some(bot_id) = WhatsAppQueries::consume_pairing(db.pool(), &hash_code(code))
        .await
        .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    let bot = BotQueries::get(db.pool(), bot_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "The paired agent no longer exists.".to_string())?;
    let title = clean_name(display_name.unwrap_or(wa_id));
    let thread = Thread::new(bot_id, format!("WhatsApp: {title}"));
    ThreadQueries::create(db.pool(), &thread)
        .await
        .map_err(|error| error.to_string())?;
    WhatsAppQueries::upsert_link(db.pool(), wa_id, Some(&title), bot_id, thread.id)
        .await
        .map_err(|error| error.to_string())?;
    Ok(Some(bot.name))
}

pub(crate) async fn process_update_with_base(
    client: &reqwest::Client,
    db: &Database,
    runtime: &Arc<Runtime>,
    events: &crate::HostEvents,
    update: &Value,
    base: &str,
) -> Result<usize, String> {
    if !enabled(db).await {
        return Ok(0);
    }
    if update.get("object").and_then(|value| value.as_str()) != Some("whatsapp_business_account") {
        return Ok(0);
    }
    let Some(config) = load_config(db, base).await else {
        return Err("WhatsApp is not fully configured.".to_string());
    };
    let mut handled = 0;
    let Some(entries) = update.get("entry").and_then(|value| value.as_array()) else {
        return Ok(0);
    };
    for entry in entries {
        let Some(changes) = entry.get("changes").and_then(|value| value.as_array()) else {
            continue;
        };
        for change in changes {
            let value = change.get("value").unwrap_or(&Value::Null);
            let names: HashMap<String, String> = value
                .get("contacts")
                .and_then(|value| value.as_array())
                .map(|contacts| {
                    contacts
                        .iter()
                        .filter_map(|contact| {
                            let wa_id = contact.get("wa_id")?.as_str()?;
                            let name = contact
                                .pointer("/profile/name")
                                .and_then(|value| value.as_str())?;
                            Some((wa_id.to_string(), clean_name(name)))
                        })
                        .collect()
                })
                .unwrap_or_default();
            let Some(messages) = value.get("messages").and_then(|value| value.as_array()) else {
                continue;
            };
            for message in messages {
                let message_type = message
                    .get("type")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                let is_text = message_type == "text";
                let is_media = matches!(message_type, "image" | "document" | "audio" | "video");
                let is_interactive = message_type == "interactive" || message_type == "button";
                if !is_text && !is_media && !is_interactive {
                    continue;
                }
                let Some(wa_id) = message.get("from").and_then(|value| value.as_str()) else {
                    continue;
                };
                if wa_id.len() < 6
                    || wa_id.len() > 20
                    || !wa_id.chars().all(|ch| ch.is_ascii_digit())
                {
                    continue;
                }
                let Some(message_id) = message.get("id").and_then(|value| value.as_str()) else {
                    continue;
                };
                let text = if is_text {
                    message
                        .pointer("/text/body")
                        .and_then(|value| value.as_str())
                        .unwrap_or("")
                        .to_string()
                } else if is_interactive {
                    let id = message
                        .pointer("/interactive/button_reply/id")
                        .or_else(|| message.pointer("/button/payload"))
                        .and_then(|value| value.as_str())
                        .unwrap_or("");
                    if id.starts_with("approve:") || id.starts_with("deny:") {
                        id.replacen(':', " ", 1)
                    } else if id.starts_with("ans:") {
                        id.to_string()
                    } else {
                        String::new()
                    }
                } else {
                    message
                        .pointer(&format!("/{message_type}/caption"))
                        .and_then(|value| value.as_str())
                        .unwrap_or("")
                        .to_string()
                };
                let attachments = if is_media {
                    whatsapp_attachments(client, &config, message, message_type).await
                } else {
                    Vec::new()
                };
                if text.trim().is_empty() && attachments.is_empty() {
                    continue;
                }
                let claimed = WhatsAppQueries::claim_event(db.pool(), wa_id, message_id)
                    .await
                    .map_err(|error| error.to_string())?;
                if !claimed {
                    continue;
                }
                if let Err(error) = handle_message(
                    client,
                    db,
                    runtime,
                    events,
                    &config,
                    wa_id,
                    names.get(wa_id).map(String::as_str),
                    &text,
                    attachments,
                )
                .await
                {
                    tracing::warn!(%error, "WhatsApp message handling failed");
                }
                handled += 1;
            }
        }
    }
    Ok(handled)
}

async fn send_approval_card(
    client: &reqwest::Client,
    config: &WhatsAppConfig,
    wa_id: &str,
    card: &crate::chat_commands::PendingCard,
) -> Result<(), String> {
    use crate::chat_commands::PendingCard;
    let (body, buttons) = match card {
        PendingCard::Approval {
            id,
            title,
            detail,
            risk,
        } => (
            format!("Approval required: {title}\n{detail}\nRisk: {risk}"),
            vec![
                json!({ "type": "reply", "reply": { "id": format!("approve:{id}"), "title": "Approve" } }),
                json!({ "type": "reply", "reply": { "id": format!("deny:{id}"), "title": "Deny" } }),
            ],
        ),
        PendingCard::Question {
            id,
            header,
            question,
            options,
        } => (
            format!("{header}\n{question}\n\nUse `answer {id} <text>` for a custom answer."),
            options
                .iter()
                .take(3)
                .enumerate()
                .map(|(index, option)| {
                    json!({ "type": "reply", "reply": { "id": format!("ans:{id}:{index}"), "title": option } })
                })
                .collect(),
        ),
    };
    if buttons.is_empty() {
        return send_text(client, config, wa_id, &body).await;
    }
    let response = client
        .post(format!(
            "{}/{}/messages",
            config.api_base, config.phone_number_id
        ))
        .bearer_auth(&config.token)
        .json(&json!({
            "messaging_product": "whatsapp",
            "recipient_type": "individual",
            "to": wa_id,
            "type": "interactive",
            "interactive": {
                "type": "button",
                "body": { "text": body },
                "action": { "buttons": buttons }
            }
        }))
        .send()
        .await
        .map_err(|_| "WhatsApp approval card request failed".to_string())?;
    if !response.status().is_success() {
        return Err("WhatsApp rejected the approval card.".to_string());
    }
    Ok(())
}

async fn whatsapp_attachments(
    client: &reqwest::Client,
    config: &WhatsAppConfig,
    message: &Value,
    message_type: &str,
) -> Vec<Attachment> {
    let Some(media_id) = message
        .pointer(&format!("/{message_type}/id"))
        .and_then(|value| value.as_str())
    else {
        return Vec::new();
    };
    let Ok(metadata) = client
        .get(format!("{}/{}", config.api_base, media_id))
        .bearer_auth(&config.token)
        .send()
        .await
    else {
        return Vec::new();
    };
    let Ok(metadata) = metadata.json::<Value>().await else {
        return Vec::new();
    };
    let Some(url) = metadata.get("url").and_then(|value| value.as_str()) else {
        return Vec::new();
    };
    let mime = metadata
        .get("mime_type")
        .and_then(|value| value.as_str())
        .or_else(|| {
            message
                .pointer(&format!("/{message_type}/mime_type"))
                .and_then(|value| value.as_str())
        });
    let name = message
        .pointer(&format!("/{message_type}/filename"))
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| match message_type {
            "image" => "image".to_string(),
            "audio" => "audio".to_string(),
            "video" => "video".to_string(),
            _ => "document".to_string(),
        });
    let size = metadata.get("file_size").and_then(|value| value.as_u64());
    crate::chat_attachments::download(client, url, Some(&config.token), &name, mime, size)
        .await
        .into_iter()
        .collect()
}

#[allow(clippy::too_many_arguments)]
async fn handle_message(
    client: &reqwest::Client,
    db: &Database,
    runtime: &Arc<Runtime>,
    events: &crate::HostEvents,
    config: &WhatsAppConfig,
    wa_id: &str,
    display_name: Option<&str>,
    text: &str,
    attachments: Vec<Attachment>,
) -> Result<(), String> {
    let text = text.trim();
    if text.is_empty() && attachments.is_empty() {
        return Ok(());
    }
    let text = if text.is_empty() {
        "(shared files)"
    } else {
        text
    };
    if !crate::chat_throttle::allow("whatsapp", wa_id) {
        return Ok(());
    }
    let command = text.split_whitespace().next().unwrap_or("");

    if command.starts_with("ans:") {
        let Some((_, question_id, index)) = crate::chat_commands::parse_card_action(command) else {
            send_text(client, config, wa_id, "That answer action is malformed.").await?;
            return Ok(());
        };
        let Some(link) = WhatsAppQueries::link_for_number(db.pool(), wa_id)
            .await
            .map_err(|error| error.to_string())?
        else {
            send_text(client, config, wa_id, "Pair this number before answering.").await?;
            return Ok(());
        };
        let thread_id = Uuid::parse_str(&link.thread_id).unwrap_or_default();
        let reply = match crate::chat_commands::answer_option(
            db,
            thread_id,
            question_id,
            index.unwrap_or(usize::MAX),
        )
        .await
        {
            Ok(message) => message,
            Err(error) => error,
        };
        send_text(client, config, wa_id, &reply).await?;
        return Ok(());
    }

    match command {
        "pair" | "/start" | "start" | "/pair" => {
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
                    config,
                    wa_id,
                    "Send `pair <code>` with a code from RAVENBOT → Chat apps.",
                )
                .await?;
                return Ok(());
            }
            match pair_number(db, &code, wa_id, display_name).await? {
                Some(name) => {
                    send_text(
                        client,
                        config,
                        wa_id,
                        &format!(
                            "Paired with {name}. Send a message, `new`, `stop`, `status` or `unlink`."
                        ),
                    )
                    .await?;
                }
                None => {
                    send_text(
                        client,
                        config,
                        wa_id,
                        "That pairing code is invalid or expired.",
                    )
                    .await?;
                }
            }
            Ok(())
        }
        "new" => {
            let Some(link) = WhatsAppQueries::link_for_number(db.pool(), wa_id)
                .await
                .map_err(|error| error.to_string())?
            else {
                send_text(client, config, wa_id, "Pair first with `pair <code>`.").await?;
                return Ok(());
            };
            let bot_id = Uuid::parse_str(&link.bot_id).unwrap_or_default();
            let bot = BotQueries::get(db.pool(), bot_id)
                .await
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "The paired agent no longer exists.".to_string())?;
            let title = clean_name(display_name.unwrap_or(wa_id));
            let thread = Thread::new(bot_id, format!("WhatsApp: {title}"));
            ThreadQueries::create(db.pool(), &thread)
                .await
                .map_err(|error| error.to_string())?;
            WhatsAppQueries::upsert_link(db.pool(), wa_id, Some(&title), bot_id, thread.id)
                .await
                .map_err(|error| error.to_string())?;
            send_text(
                client,
                config,
                wa_id,
                &format!("Started a new conversation with {}.", bot.name),
            )
            .await?;
            Ok(())
        }
        "stop" => {
            let Some(link) = WhatsAppQueries::link_for_number(db.pool(), wa_id)
                .await
                .map_err(|error| error.to_string())?
            else {
                send_text(client, config, wa_id, "Pair first with `pair <code>`.").await?;
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
                config,
                wa_id,
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
            let Some(link) = WhatsAppQueries::link_for_number(db.pool(), wa_id)
                .await
                .map_err(|error| error.to_string())?
            else {
                send_text(client, config, wa_id, "Pair first with `pair <code>`.").await?;
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
                config,
                wa_id,
                &format!("Paired with {bot}. {active} active run(s)."),
            )
            .await?;
            Ok(())
        }
        "unlink" => {
            let removed = WhatsAppQueries::unlink(db.pool(), wa_id)
                .await
                .map_err(|error| error.to_string())?;
            send_text(
                client,
                config,
                wa_id,
                if removed {
                    "This number is no longer paired."
                } else {
                    "This number was not paired."
                },
            )
            .await?;
            Ok(())
        }
        "approve" | "deny" | "answer" => {
            let Some(link) = WhatsAppQueries::link_for_number(db.pool(), wa_id)
                .await
                .map_err(|error| error.to_string())?
            else {
                send_text(
                    client,
                    config,
                    wa_id,
                    "Pair this number before deciding approvals.",
                )
                .await?;
                return Ok(());
            };
            let thread_id = Uuid::parse_str(&link.thread_id).unwrap_or_default();
            let reply = match crate::chat_commands::handle(db, runtime, thread_id, text).await {
                Some(Ok(message)) => message,
                Some(Err(error)) => error,
                None => "Unknown command.".to_string(),
            };
            send_text(client, config, wa_id, &reply).await?;
            Ok(())
        }
        _ => {
            handle_text(
                client,
                db,
                runtime,
                events,
                config,
                wa_id,
                text,
                attachments,
                display_name,
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
    config: &WhatsAppConfig,
    wa_id: &str,
    text: &str,
    attachments: Vec<Attachment>,
    display_name: Option<&str>,
) -> Result<(), String> {
    let Some(link) = WhatsAppQueries::link_for_number(db.pool(), wa_id)
        .await
        .map_err(|error| error.to_string())?
    else {
        send_text(
            client,
            config,
            wa_id,
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
            config,
            wa_id,
            "Still working on the previous message. Send `stop` to cancel it.",
        )
        .await?;
        return Ok(());
    }
    let content = text.chars().take(20_000).collect::<String>();
    let actor_name = display_name.unwrap_or(wa_id);
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
    let _ = crate::chat_commands::attribute_run(db, run.id, wa_id, actor_name).await;
    let _ = WhatsAppQueries::touch(db.pool(), wa_id).await;
    let runtime = runtime.clone();
    let db = db.clone();
    let events = events.clone();
    let client = client.clone();
    let config = config.clone();
    let wa_id = wa_id.to_string();
    tokio::spawn(async move {
        runtime.set_thread_emitter(thread_id, events.stream_emitter());
        runtime.review_approvals_for_run(run.id, true);
        let monitor = {
            let db = db.clone();
            let client = client.clone();
            let config = config.clone();
            let wa_id = wa_id.clone();
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
                            let _ = send_approval_card(&client, &config, &wa_id, &card).await;
                        } else {
                            let _ = send_text(
                                &client,
                                &config,
                                &wa_id,
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
        if let Err(error) = send_text(&client, &config, &wa_id, &reply).await {
            tracing::warn!(%error, "WhatsApp reply failed");
        }
        for attachment in attachments {
            if let Ok(file) = crate::chat_outbound::OutboundFile::from_attachment(&attachment) {
                if let Err(error) = send_outbound_file(&client, &config, &wa_id, &file).await {
                    tracing::warn!(%error, "WhatsApp attachment reply failed");
                }
            }
        }
    });
    Ok(())
}

async fn send_outbound_file(
    client: &reqwest::Client,
    config: &WhatsAppConfig,
    wa_id: &str,
    file: &crate::chat_outbound::OutboundFile,
) -> Result<(), String> {
    let part = reqwest::multipart::Part::bytes(file.bytes.clone())
        .file_name(file.name.clone())
        .mime_str(&file.mime_type)
        .map_err(|_| "The attachment MIME type is invalid.".to_string())?;
    let form = reqwest::multipart::Form::new()
        .text("messaging_product", "whatsapp")
        .text("type", file.mime_type.clone())
        .part("file", part);
    let upload: Value = client
        .post(format!(
            "{}/{}/media",
            config.api_base, config.phone_number_id
        ))
        .bearer_auth(&config.token)
        .multipart(form)
        .send()
        .await
        .map_err(|_| "WhatsApp media upload failed".to_string())?
        .json()
        .await
        .map_err(|_| "WhatsApp media upload response was not JSON".to_string())?;
    let Some(media_id) = upload.get("id").and_then(|value| value.as_str()) else {
        return Err("WhatsApp returned no media id.".to_string());
    };
    let payload = if file.is_image {
        json!({
            "messaging_product": "whatsapp",
            "recipient_type": "individual",
            "to": wa_id,
            "type": "image",
            "image": { "id": media_id }
        })
    } else {
        json!({
            "messaging_product": "whatsapp",
            "recipient_type": "individual",
            "to": wa_id,
            "type": "document",
            "document": { "id": media_id, "filename": file.name }
        })
    };
    let response = client
        .post(format!(
            "{}/{}/messages",
            config.api_base, config.phone_number_id
        ))
        .bearer_auth(&config.token)
        .json(&payload)
        .send()
        .await
        .map_err(|_| "WhatsApp attachment message failed".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "WhatsApp returned HTTP {} for the attachment.",
            response.status().as_u16()
        ));
    }
    Ok(())
}

async fn send_text(
    client: &reqwest::Client,
    config: &WhatsAppConfig,
    wa_id: &str,
    text: &str,
) -> Result<(), String> {
    for chunk in split_message(text) {
        let response = client
            .post(format!(
                "{}/{}/messages",
                config.api_base, config.phone_number_id
            ))
            .bearer_auth(&config.token)
            .json(&json!({
                "messaging_product": "whatsapp",
                "recipient_type": "individual",
                "to": wa_id,
                "type": "text",
                "text": { "preview_url": false, "body": chunk }
            }))
            .send()
            .await
            .map_err(|_| "WhatsApp message request failed".to_string())?;
        if !response.status().is_success() {
            return Err(format!(
                "WhatsApp returned HTTP {}",
                response.status().as_u16()
            ));
        }
    }
    Ok(())
}

/// Send to a WhatsApp number that is already paired with RAVENBOT.
///
/// A free-form message is still subject to Meta's 24-hour customer-service
/// window. The wrapper does not fake a template message: the provider error is
/// returned instead.
pub async fn send_linked_text(
    db: &Database,
    bot_id: Uuid,
    wa_id: &str,
    text: &str,
) -> Result<(), String> {
    if !enabled(db).await {
        return Err("WhatsApp is not enabled in Settings → Chat apps.".to_string());
    }
    if WhatsAppQueries::link_for_number(db.pool(), wa_id)
        .await
        .map_err(|error| format!("Could not read the WhatsApp link: {error}"))?
        .filter(|link| link.bot_id == bot_id.to_string())
        .is_none()
    {
        return Err("That WhatsApp number is not paired with this agent.".to_string());
    }
    let base = api_base();
    let config = load_config(db, &base).await.ok_or_else(|| {
        "WhatsApp has no token or phone number id. Configure it in Settings → Chat apps."
            .to_string()
    })?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|error| format!("Could not prepare the WhatsApp request: {error}"))?;
    send_text(&client, &config, wa_id, text).await
}

/// WhatsApp's text limit is 4096 characters. Leave room for formatting and
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

    #[tokio::test]
    async fn proactive_send_fails_closed_when_the_connector_is_off() {
        let path = std::env::temp_dir().join(format!("ravenbot-whatsapp-{}.db", Uuid::new_v4()));
        let db = Database::new(&path).await.expect("db");
        let error = send_linked_text(&db, Uuid::new_v4(), "15551234567", "hello")
            .await
            .expect_err("disabled");
        assert!(error.contains("not enabled"));
        let _ = std::fs::remove_file(&path);
    }

    fn signature(secret: &str, body: &[u8]) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("hmac");
        mac.update(body);
        let bytes = mac.finalize().into_bytes();
        format!(
            "sha256={}",
            bytes
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        )
    }

    #[test]
    fn whatsapp_credentials_are_validated() {
        assert!(validate_token(&"a".repeat(30)).is_ok());
        assert!(validate_token("short").is_err());
        assert!(validate_app_secret(&"a".repeat(32)).is_ok());
        assert!(validate_app_secret("short").is_err());
        assert!(validate_verify_token("verify-token").is_ok());
        assert!(validate_verify_token("short").is_err());
        assert!(validate_phone_number_id("1234567890").is_ok());
        assert!(validate_phone_number_id("12ab").is_err());
    }

    #[test]
    fn webhook_signatures_are_checked_in_constant_time() {
        let secret = "0123456789abcdef0123456789abcdef";
        let body = br#"{"object":"whatsapp_business_account"}"#;
        let header = signature(secret, body);
        assert!(verify_signature(secret, body, &header));
        assert!(!verify_signature(secret, body, "sha256=00"));
        assert!(!verify_signature("another-secret", body, &header));
        assert!(!verify_signature(secret, body, ""));
        // A non-ASCII header must be rejected, not panic while slicing.
        assert!(!verify_signature(secret, body, "sha256=éé"));
    }

    #[test]
    fn whatsapp_messages_split_below_the_limit() {
        let chunks = split_message(&"x".repeat(9000));
        assert_eq!(chunks.len(), 3);
        assert!(chunks.iter().all(|chunk| chunk.chars().count() <= 3500));
        assert_eq!(split_message("   "), vec!["(no response)"]);
    }

    #[tokio::test]
    async fn a_webhook_pair_event_pairs_a_number_and_replies() {
        use ravenbot_core::Bot;
        use ravenbot_db::queries::BotQueries;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let path = std::env::temp_dir().join(format!("ravenbot-whatsapp-{}.db", Uuid::new_v4()));
        let db = Database::new(&path).await.expect("db");
        let bot = Bot::new("WhatsApp Bot", "chat test");
        BotQueries::insert(db.pool(), &bot).await.expect("bot");
        let pairing = create_pairing(&db, bot.id, Duration::from_secs(300))
            .await
            .expect("pairing");
        let token = "EAAGabcdefghijklmnopqrstuvwxyz";
        set_token(&db, token).await.expect("token");
        set_phone_number_id(&db, "1234567890").await.expect("phone");
        AppSettingsQueries::set(db.pool(), ENABLED_KEY, "1")
            .await
            .expect("enabled");

        let http = TcpListener::bind("127.0.0.1:0").await.expect("http");
        let http_address = http.local_addr().expect("address");
        let http_task = tokio::spawn(async move {
            let (mut socket, _) = http.accept().await.expect("accept");
            let mut buffer = [0u8; 4096];
            let read = socket.read(&mut buffer).await.expect("read");
            let request = String::from_utf8_lossy(&buffer[..read]).to_string();
            assert!(request.contains("/1234567890/messages"));
            assert!(request.contains("Bearer "));
            let body = r#"{"messages":[{"id":"wamid.out"}]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = socket.write_all(response.as_bytes()).await;
        });

        let update = json!({
            "object": "whatsapp_business_account",
            "entry": [{
                "changes": [{
                    "value": {
                        "contacts": [{
                            "wa_id": "15551234567",
                            "profile": { "name": "Ada" }
                        }],
                        "messages": [{
                            "from": "15551234567",
                            "id": "wamid.1",
                            "type": "text",
                            "text": { "body": format!("pair {}", pairing.code) }
                        }]
                    }
                }]
            }]
        });
        let client = reqwest::Client::new();
        let runtime = Arc::new(Runtime::new(db.clone()));
        let handled = process_update_with_base(
            &client,
            &db,
            &runtime,
            &crate::HostEvents::Headless,
            &update,
            &format!("http://{}", http_address),
        )
        .await
        .expect("process");
        assert_eq!(handled, 1);
        http_task.await.expect("http task");
        let link = WhatsAppQueries::link_for_number(db.pool(), "15551234567")
            .await
            .expect("lookup")
            .expect("link");
        assert_eq!(link.bot_id, bot.id.to_string());
        assert_eq!(link.display_name.as_deref(), Some("Ada"));
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn the_webhook_server_checks_verification_and_signatures() {
        use ravenbot_core::Bot;
        use ravenbot_db::queries::BotQueries;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let path = std::env::temp_dir().join(format!("ravenbot-whatsapp-{}.db", Uuid::new_v4()));
        let db = Database::new(&path).await.expect("db");
        let bot = Bot::new("WhatsApp Bot", "chat test");
        BotQueries::insert(db.pool(), &bot).await.expect("bot");
        let pairing = create_pairing(&db, bot.id, Duration::from_secs(300))
            .await
            .expect("pairing");
        let secret = "0123456789abcdef0123456789abcdef";
        let verify = "verify-token-123";
        set_token(&db, "EAAGabcdefghijklmnopqrstuvwxyz")
            .await
            .expect("token");
        set_app_secret(&db, secret).await.expect("secret");
        set_verify_token(&db, verify).await.expect("verify");
        set_phone_number_id(&db, "1234567890").await.expect("phone");
        AppSettingsQueries::set(db.pool(), ENABLED_KEY, "1")
            .await
            .expect("enabled");

        let graph = TcpListener::bind("127.0.0.1:0").await.expect("graph");
        let graph_address = graph.local_addr().expect("address");
        let graph_task = tokio::spawn(async move {
            let (mut socket, _) = graph.accept().await.expect("accept");
            let mut buffer = [0u8; 4096];
            let read = socket.read(&mut buffer).await.expect("read");
            let request = String::from_utf8_lossy(&buffer[..read]).to_string();
            assert!(request.contains("/1234567890/messages"));
            let body = r#"{"messages":[{"id":"wamid.out"}]}"#;
            let response = format!(
                "HTTP/1.1 200 OK
content-type: application/json
content-length: {}

{}",
                body.len(),
                body
            );
            let _ = socket.write_all(response.as_bytes()).await;
        });

        let runtime = Arc::new(Runtime::new(db.clone()));
        let server = crate::chat_webhook::ChatWebhookServer::start(
            db.clone(),
            runtime,
            crate::HostEvents::Headless,
            crate::chat_webhook::ChatWebhookConfig {
                bind: "127.0.0.1".into(),
                port: 0,
                whatsapp_api_base: Some(format!("http://{}", graph_address)),
                teams_token_url: None,
                teams_openid_url: None,
            },
        )
        .await
        .expect("server");
        let base = format!("http://{}/whatsapp", server.addr());
        let client = reqwest::Client::new();

        let ok = client
            .get(format!(
                "{base}?hub.mode=subscribe&hub.verify_token={verify}&hub.challenge=abc123"
            ))
            .send()
            .await
            .expect("challenge");
        assert_eq!(ok.status(), 200);
        assert_eq!(ok.text().await.expect("text"), "abc123");

        let denied = client
            .get(format!(
                "{base}?hub.mode=subscribe&hub.verify_token=wrong&hub.challenge=abc123"
            ))
            .send()
            .await
            .expect("bad challenge");
        assert_eq!(denied.status(), 403);

        let update = json!({
            "object": "whatsapp_business_account",
            "entry": [{
                "changes": [{
                    "value": {
                        "contacts": [{
                            "wa_id": "15551234567",
                            "profile": { "name": "Ada" }
                        }],
                        "messages": [{
                            "from": "15551234567",
                            "id": "wamid.webhook",
                            "type": "text",
                            "text": { "body": format!("pair {}", pairing.code) }
                        }]
                    }
                }]
            }]
        })
        .to_string();
        let bad = client
            .post(&base)
            .header("x-hub-signature-256", "sha256=deadbeef")
            .body(update.clone())
            .send()
            .await
            .expect("bad signature");
        assert_eq!(bad.status(), 403);

        let signature = signature(secret, update.as_bytes());
        let accepted = client
            .post(&base)
            .header("x-hub-signature-256", signature)
            .header("content-type", "application/json")
            .body(update)
            .send()
            .await
            .expect("webhook");
        assert_eq!(accepted.status(), 200);

        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        loop {
            if WhatsAppQueries::link_for_number(db.pool(), "15551234567")
                .await
                .expect("lookup")
                .is_some()
            {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                panic!("webhook did not pair the number");
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        tokio::time::timeout(Duration::from_secs(3), graph_task)
            .await
            .expect("graph reply")
            .expect("graph task");
        drop(server);
        let _ = std::fs::remove_file(&path);
    }
}
