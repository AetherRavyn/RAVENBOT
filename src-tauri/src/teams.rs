//! Native Microsoft Teams connector over the Bot Framework.
//!
//! Teams delivers activities to the bot's HTTPS messaging endpoint. Every
//! request carries a Bot Framework JWT: this module validates its RS256
//! signature against the published OpenID keys, checks the audience and issuer,
//! and requires the token's `serviceurl` claim to match the activity. A tenant
//! allowlist is mandatory because a multi-tenant bot can otherwise be messaged
//! by any tenant that installs it.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use axum::http::{HeaderMap, StatusCode};
use jsonwebtoken::jwk::{Jwk, JwkSet};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use ravenbot_core::{Attachment, Message, Run, Thread};
use ravenbot_db::queries::{
    AppSettingsQueries, ApprovalQueries, BotQueries, MessageQueries, QuestionQueries, RunQueries,
    TeamsLink, TeamsQueries, ThreadQueries,
};
use ravenbot_db::Database;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use uuid::Uuid;

pub const ENABLED_KEY: &str = "teams_enabled";
pub const APP_ID_KEY: &str = "teams_app_id";
pub const APP_PASSWORD_KEY: &str = "teams_app_password";
pub const TENANTS_KEY: &str = "teams_tenants";
pub const DEFAULT_BOT_KEY: &str = "teams_default_bot";
const TOKEN_URL_ENV: &str = "RAVENBOT_TEAMS_TOKEN_URL";
const OPENID_URL_ENV: &str = "RAVENBOT_TEAMS_OPENID_URL";
const ISSUER_ENV: &str = "RAVENBOT_TEAMS_ISSUER";
const DEFAULT_TOKEN_URL: &str =
    "https://login.microsoftonline.com/botframework.com/oauth2/v2.0/token";
const DEFAULT_OPENID_URL: &str =
    "https://login.botframework.com/v1/.well-known/openidconfiguration";
const BOT_ISSUER: &str = "https://api.botframework.com";
const BOT_SCOPE: &str = "https://api.botframework.com/.default";
const KEYS_TTL: Duration = Duration::from_secs(3_600);
const KEY_REFRESH_MIN: Duration = Duration::from_secs(60);
const MAX_OUTBOUND_FILES: usize = 4;
const PENDING_FILE_TTL: Duration = Duration::from_secs(60 * 60);
const UPLOAD_CHUNK_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct TeamsStatus {
    pub enabled: bool,
    pub app_id: Option<String>,
    pub app_password_present: bool,
    pub tenants: Vec<String>,
    pub default_bot: Option<String>,
    pub links: Vec<TeamsLink>,
    pub local_webhook_url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TeamsPairing {
    pub code: String,
    pub bot_id: Uuid,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BotClaims {
    aud: Value,
    iss: String,
    exp: usize,
    #[serde(default)]
    nbf: Option<usize>,
    #[serde(default)]
    serviceurl: Option<String>,
}

struct CachedKeys {
    keys: Vec<Jwk>,
    fetched_at: Instant,
}

struct CachedToken {
    app_id: String,
    token: String,
    expires_at: Instant,
}

struct TeamsAuth {
    client: reqwest::Client,
    token_url: String,
    cache: Mutex<Option<CachedToken>>,
}

impl TeamsAuth {
    fn new(client: reqwest::Client, token_url: String) -> Self {
        Self {
            client,
            token_url,
            cache: Mutex::new(None),
        }
    }

    async fn token(&self, app_id: &str, password: &str) -> Result<String, String> {
        {
            let cache = self.cache.lock().await;
            if let Some(cached) = cache.as_ref() {
                if cached.app_id == app_id
                    && cached.expires_at > Instant::now() + Duration::from_secs(60)
                {
                    return Ok(cached.token.clone());
                }
            }
        }
        let response = self
            .client
            .post(&self.token_url)
            .form(&[
                ("grant_type", "client_credentials"),
                ("client_id", app_id),
                ("client_secret", password),
                ("scope", BOT_SCOPE),
            ])
            .send()
            .await
            .map_err(|_| "Teams token request failed".to_string())?;
        let body: Value = response
            .json()
            .await
            .map_err(|_| "Teams token response was not JSON".to_string())?;
        let token = body
            .get("access_token")
            .and_then(|value| value.as_str())
            .ok_or_else(|| "Teams rejected the app credentials".to_string())?
            .to_string();
        let expires_in = body
            .get("expires_in")
            .and_then(|value| {
                value
                    .as_u64()
                    .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
            })
            .unwrap_or(3_600)
            .max(60);
        {
            let mut cache = self.cache.lock().await;
            *cache = Some(CachedToken {
                app_id: app_id.to_string(),
                token: token.clone(),
                expires_at: Instant::now() + Duration::from_secs(expires_in),
            });
        }
        Ok(token)
    }
}

struct TeamsJwtValidator {
    client: reqwest::Client,
    openid_url: String,
    issuer: String,
    cache: Mutex<Option<CachedKeys>>,
}

impl TeamsJwtValidator {
    fn new(client: reqwest::Client, openid_url: String, issuer: String) -> Self {
        Self {
            client,
            openid_url,
            issuer,
            cache: Mutex::new(None),
        }
    }

    async fn keys(&self, refresh: bool) -> Result<Vec<Jwk>, String> {
        {
            let cache = self.cache.lock().await;
            if let Some(cached) = cache.as_ref() {
                let fresh_for = if refresh { KEY_REFRESH_MIN } else { KEYS_TTL };
                if cached.fetched_at.elapsed() < fresh_for {
                    return Ok(cached.keys.clone());
                }
            }
        }
        let config: Value = self
            .client
            .get(&self.openid_url)
            .send()
            .await
            .map_err(|_| "Teams OpenID metadata request failed".to_string())?
            .json()
            .await
            .map_err(|_| "Teams OpenID metadata was not JSON".to_string())?;
        let jwks_uri = config
            .get("jwks_uri")
            .and_then(|value| value.as_str())
            .ok_or_else(|| "Teams OpenID metadata has no jwks_uri".to_string())?;
        let set: JwkSet = self
            .client
            .get(jwks_uri)
            .send()
            .await
            .map_err(|_| "Teams signing-key request failed".to_string())?
            .json()
            .await
            .map_err(|_| "Teams signing keys were not JSON".to_string())?;
        let keys = set.keys;
        {
            let mut cache = self.cache.lock().await;
            *cache = Some(CachedKeys {
                keys: keys.clone(),
                fetched_at: Instant::now(),
            });
        }
        Ok(keys)
    }

    async fn validate(&self, token: &str, app_id: &str, service_url: &str) -> Result<(), String> {
        let header =
            decode_header(token).map_err(|_| "Teams token header is invalid".to_string())?;
        if header.alg != Algorithm::RS256 {
            return Err("Teams token is not RS256".to_string());
        }
        let kid = header
            .kid
            .ok_or_else(|| "Teams token has no key id".to_string())?;
        let mut keys = self.keys(false).await?;
        let mut jwk = keys
            .iter()
            .find(|key| key.common.key_id.as_deref() == Some(kid.as_str()));
        if jwk.is_none() {
            keys = self.keys(true).await?;
            jwk = keys
                .iter()
                .find(|key| key.common.key_id.as_deref() == Some(kid.as_str()));
        }
        let jwk = jwk.ok_or_else(|| "Teams token key is unknown".to_string())?;
        let key = DecodingKey::from_jwk(jwk)
            .map_err(|_| "Teams signing key could not be used".to_string())?;
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_audience(&[app_id]);
        validation.set_issuer(&[self.issuer.as_str()]);
        validation.validate_aud = true;
        validation.validate_exp = true;
        validation.validate_nbf = true;
        validation.leeway = 300;
        let claims = decode::<BotClaims>(token, &key, &validation)
            .map_err(|_| "Teams token failed validation".to_string())?
            .claims;
        let claimed = claims
            .serviceurl
            .ok_or_else(|| "Teams token has no service URL".to_string())?;
        if !claimed
            .trim_end_matches('/')
            .eq_ignore_ascii_case(service_url.trim_end_matches('/'))
        {
            return Err("Teams token service URL does not match the activity".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct PendingConsent {
    token: String,
    conversation_id: String,
    name: String,
    mime_type: String,
    size: u64,
    created_at: i64,
}

#[cfg(test)]
static TEST_OUTBOUND_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

fn consent_dir() -> PathBuf {
    #[cfg(test)]
    if let Some(dir) = TEST_OUTBOUND_DIR.get() {
        return dir.clone();
    }
    ravenbot_core::cache_dir().join("teams-outbound")
}

fn consent_paths(dir: &Path, token: &str) -> Option<(PathBuf, PathBuf)> {
    let id = Uuid::parse_str(token).ok()?;
    let token = id.to_string();
    Some((
        dir.join(format!("{token}.json")),
        dir.join(format!("{token}.bin")),
    ))
}

fn ensure_consent_dir(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir)
        .map_err(|error| format!("Could not create the Teams upload directory: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
    Ok(())
}

fn write_pending_in(
    dir: &Path,
    conversation_id: &str,
    file: &crate::chat_outbound::OutboundFile,
) -> Result<PendingConsent, String> {
    ensure_consent_dir(dir)?;
    cleanup_stale_in(dir);
    let token = Uuid::new_v4().to_string();
    let (meta_path, data_path) = consent_paths(dir, &token)
        .ok_or_else(|| "The Teams upload token is invalid.".to_string())?;
    std::fs::write(&data_path, &file.bytes)
        .map_err(|error| format!("Could not store the Teams upload: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&data_path, std::fs::Permissions::from_mode(0o600));
    }
    let pending = PendingConsent {
        token,
        conversation_id: conversation_id.to_string(),
        name: file.name.clone(),
        mime_type: file.mime_type.clone(),
        size: file.bytes.len() as u64,
        created_at: chrono::Utc::now().timestamp(),
    };
    let metadata = serde_json::to_vec(&pending)
        .map_err(|_| "Could not encode the Teams upload metadata.".to_string())?;
    if let Err(error) = std::fs::write(&meta_path, metadata) {
        let _ = std::fs::remove_file(&data_path);
        return Err(format!(
            "Could not store the Teams upload metadata: {error}"
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&meta_path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(pending)
}

fn read_pending_in(
    dir: &Path,
    token: &str,
    conversation_id: &str,
) -> Result<(PendingConsent, Vec<u8>), String> {
    let (meta_path, data_path) = consent_paths(dir, token)
        .ok_or_else(|| "That file upload request is no longer valid.".to_string())?;
    let metadata = std::fs::read(&meta_path)
        .map_err(|_| "That file upload request has expired.".to_string())?;
    let pending: PendingConsent = serde_json::from_slice(&metadata)
        .map_err(|_| "That file upload request is malformed.".to_string())?;
    if pending.conversation_id != conversation_id {
        return Err("That file upload request belongs to another conversation.".to_string());
    }
    let age = chrono::Utc::now()
        .timestamp()
        .saturating_sub(pending.created_at) as u64;
    if age > PENDING_FILE_TTL.as_secs() {
        remove_pending_in(dir, token);
        return Err("That file upload request has expired.".to_string());
    }
    let bytes = std::fs::read(&data_path)
        .map_err(|_| "That file upload request has expired.".to_string())?;
    if bytes.len() as u64 != pending.size {
        remove_pending_in(dir, token);
        return Err("The stored Teams upload was incomplete.".to_string());
    }
    Ok((pending, bytes))
}

fn remove_pending_in(dir: &Path, token: &str) {
    if let Some((meta_path, data_path)) = consent_paths(dir, token) {
        let _ = std::fs::remove_file(meta_path);
        let _ = std::fs::remove_file(data_path);
    }
}

fn cleanup_stale_in(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let now = chrono::Utc::now().timestamp();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let Ok(raw) = std::fs::read(&path) else {
            continue;
        };
        let Ok(pending) = serde_json::from_slice::<PendingConsent>(&raw) else {
            let _ = std::fs::remove_file(&path);
            let _ = std::fs::remove_file(path.with_extension("bin"));
            continue;
        };
        if now.saturating_sub(pending.created_at) as u64 > PENDING_FILE_TTL.as_secs() {
            remove_pending_in(dir, &pending.token);
        }
    }
}

async fn upload_consent_file(
    client: &reqwest::Client,
    upload_url: &str,
    bytes: &[u8],
) -> Result<(), String> {
    let total = bytes.len();
    if total == 0 {
        let response = client
            .put(upload_url)
            .header(reqwest::header::CONTENT_LENGTH, "0")
            .body(Vec::new())
            .send()
            .await
            .map_err(|_| "The Teams file upload request failed.".to_string())?;
        if !response.status().is_success() {
            return Err(format!(
                "Teams rejected the upload with HTTP {}.",
                response.status().as_u16()
            ));
        }
        return Ok(());
    }
    let mut start = 0;
    while start < total {
        let end = (start + UPLOAD_CHUNK_BYTES).min(total);
        let chunk = bytes[start..end].to_vec();
        let response = client
            .put(upload_url)
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .header(reqwest::header::CONTENT_LENGTH, chunk.len())
            .header(
                reqwest::header::CONTENT_RANGE,
                format!("bytes {}-{}/{}", start, end - 1, total),
            )
            .body(chunk)
            .send()
            .await
            .map_err(|_| "The Teams file upload request failed.".to_string())?;
        if !response.status().is_success() {
            return Err(format!(
                "Teams rejected the upload with HTTP {}.",
                response.status().as_u16()
            ));
        }
        start = end;
    }
    Ok(())
}

pub(crate) struct TeamsService {
    client: reqwest::Client,
    auth: TeamsAuth,
    jwt: TeamsJwtValidator,
}

impl TeamsService {
    pub(crate) fn new(
        client: reqwest::Client,
        token_url: Option<String>,
        openid_url: Option<String>,
    ) -> Self {
        let token_url = token_url
            .or_else(|| std::env::var(TOKEN_URL_ENV).ok())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_TOKEN_URL.to_string());
        let openid_url = openid_url
            .or_else(|| std::env::var(OPENID_URL_ENV).ok())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_OPENID_URL.to_string());
        let issuer = std::env::var(ISSUER_ENV)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| BOT_ISSUER.to_string());
        Self {
            client: client.clone(),
            auth: TeamsAuth::new(client.clone(), token_url),
            jwt: TeamsJwtValidator::new(client, openid_url, issuer),
        }
    }

    pub(crate) async fn send_card(
        &self,
        app_id: &str,
        app_password: &str,
        service_url: &str,
        conversation_id: &str,
        activity_id: &str,
        card: &Value,
    ) -> Result<(), String> {
        let token = self.auth.token(app_id, app_password).await?;
        let url = activity_url(service_url, conversation_id, activity_id)?;
        let response = self
            .client
            .post(url)
            .bearer_auth(&token)
            .json(&json!({
                "type": "message",
                "attachments": [{
                    "contentType": "application/vnd.microsoft.card.adaptive",
                    "content": card
                }]
            }))
            .send()
            .await
            .map_err(|_| "Teams card request failed".to_string())?;
        if !response.status().is_success() {
            return Err(format!(
                "Teams connector returned HTTP {}",
                response.status().as_u16()
            ));
        }
        Ok(())
    }

    pub(crate) async fn send_text(
        &self,
        app_id: &str,
        app_password: &str,
        service_url: &str,
        conversation_id: &str,
        activity_id: &str,
        text: &str,
    ) -> Result<(), String> {
        let token = self.auth.token(app_id, app_password).await?;
        for chunk in split_message(text) {
            let url = activity_url(service_url, conversation_id, activity_id)?;
            let response = self
                .client
                .post(url)
                .bearer_auth(&token)
                .json(&json!({
                    "type": "message",
                    "text": chunk,
                    "textFormat": "plain"
                }))
                .send()
                .await
                .map_err(|_| "Teams activity request failed".to_string())?;
            if !response.status().is_success() {
                return Err(format!(
                    "Teams connector returned HTTP {}",
                    response.status().as_u16()
                ));
            }
        }
        Ok(())
    }

    /// Ask the user to allow one outbound file. Teams calls back with a
    /// `fileConsent/invoke` activity once the card is accepted.
    pub(crate) async fn send_file_consent(
        &self,
        app_id: &str,
        app_password: &str,
        service_url: &str,
        conversation_id: &str,
        activity_id: &str,
        file: &crate::chat_outbound::OutboundFile,
    ) -> Result<(), String> {
        let bearer = self.auth.token(app_id, app_password).await?;
        let pending = write_pending_in(&consent_dir(), conversation_id, file)?;
        let url = activity_url(service_url, conversation_id, activity_id)?;
        let response = self
            .client
            .post(url)
            .bearer_auth(&bearer)
            .json(&json!({
                "type": "message",
                "attachments": [{
                    "contentType": "application/vnd.microsoft.teams.card.file.consent",
                    "name": pending.name,
                    "content": {
                        "description": "RAVENBOT wants to send you this file.",
                        "sizeInBytes": pending.size,
                        "acceptContext": { "token": pending.token },
                        "declineContext": { "token": pending.token }
                    }
                }]
            }))
            .send()
            .await
            .map_err(|_| "Teams file consent request failed".to_string())?;
        if !response.status().is_success() {
            remove_pending_in(&consent_dir(), &pending.token);
            return Err(format!(
                "Teams connector returned HTTP {} for the file consent card.",
                response.status().as_u16()
            ));
        }
        Ok(())
    }

    /// Send the file info card after a successful consent upload.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn send_file_info(
        &self,
        app_id: &str,
        app_password: &str,
        service_url: &str,
        conversation_id: &str,
        activity_id: &str,
        name: &str,
        content_url: &str,
        unique_id: &str,
        file_type: &str,
    ) -> Result<(), String> {
        let bearer = self.auth.token(app_id, app_password).await?;
        let url = activity_url(service_url, conversation_id, activity_id)?;
        let response = self
            .client
            .post(url)
            .bearer_auth(&bearer)
            .json(&json!({
                "type": "message",
                "attachments": [{
                    "contentType": "application/vnd.microsoft.teams.card.file.info",
                    "contentUrl": content_url,
                    "name": name,
                    "content": {
                        "uniqueId": unique_id,
                        "fileType": file_type
                    }
                }]
            }))
            .send()
            .await
            .map_err(|_| "Teams file info request failed".to_string())?;
        if !response.status().is_success() {
            return Err(format!(
                "Teams connector returned HTTP {} for the file info card.",
                response.status().as_u16()
            ));
        }
        Ok(())
    }
}

impl TeamsService {
    async fn validate(&self, token: &str, app_id: &str, service_url: &str) -> Result<(), String> {
        self.jwt.validate(token, app_id, service_url).await
    }
}

fn activity_url(
    service_url: &str,
    conversation_id: &str,
    activity_id: &str,
) -> Result<reqwest::Url, String> {
    let mut url =
        reqwest::Url::parse(service_url).map_err(|_| "Teams service URL is invalid".to_string())?;
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| "Teams service URL cannot be a base".to_string())?;
        segments.pop_if_empty();
        segments.extend([
            "v3",
            "conversations",
            conversation_id,
            "activities",
            activity_id,
        ]);
    }
    Ok(url)
}

pub fn validate_app_id(raw: &str) -> Result<String, String> {
    let value = raw.trim().to_ascii_lowercase();
    if !is_guid(&value) {
        return Err("The Teams app id must be a GUID.".to_string());
    }
    Ok(value)
}

pub fn validate_app_password(raw: &str) -> Result<String, String> {
    let value = raw.trim();
    if value.len() < 8 || value.len() > 512 {
        return Err("The Teams app password is between 8 and 512 characters.".to_string());
    }
    if value
        .chars()
        .any(|ch| ch.is_whitespace() || ch.is_control())
    {
        return Err("The Teams app password contains invalid characters.".to_string());
    }
    Ok(value.to_string())
}

pub fn validate_tenants(raw: &str) -> Result<String, String> {
    let mut tenants = Vec::new();
    for value in raw.split(|ch: char| ch == ',' || ch.is_whitespace()) {
        let value = value.trim().to_ascii_lowercase();
        if value.is_empty() {
            continue;
        }
        if !is_guid(&value) {
            return Err(format!("The Teams tenant id is not a GUID: {value}"));
        }
        if !tenants.contains(&value) {
            tenants.push(value);
        }
    }
    if tenants.is_empty() {
        return Err("Add at least one Teams tenant id.".to_string());
    }
    Ok(tenants.join(","))
}

fn is_guid(value: &str) -> bool {
    value.len() == 36
        && value.chars().enumerate().all(|(index, ch)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                ch == '-'
            } else {
                ch.is_ascii_hexdigit()
            }
        })
}

async fn token_marker(stored: String) -> Option<String> {
    if stored == ravenbot_db::secrets::KEYCHAIN_MARKER {
        return ravenbot_db::secrets::get_teams_app_password().await;
    }
    let stored = stored.trim().to_string();
    (!stored.is_empty()).then_some(stored)
}

pub async fn read_app_password(db: &Database) -> Option<String> {
    let stored = AppSettingsQueries::get(db.pool(), APP_PASSWORD_KEY)
        .await
        .ok()
        .flatten()?;
    token_marker(stored).await
}

pub async fn set_app_password(db: &Database, raw: &str) -> Result<(), String> {
    let password = validate_app_password(raw)?;
    let stored = if ravenbot_db::secrets::set_teams_app_password(&password).await {
        ravenbot_db::secrets::KEYCHAIN_MARKER.to_string()
    } else {
        password
    };
    AppSettingsQueries::set(db.pool(), APP_PASSWORD_KEY, &stored)
        .await
        .map_err(|error| error.to_string())
}

pub async fn clear_app_password(db: &Database) -> Result<(), String> {
    ravenbot_db::secrets::delete_teams_app_password().await;
    AppSettingsQueries::set(db.pool(), APP_PASSWORD_KEY, "")
        .await
        .map_err(|error| error.to_string())
}

pub async fn read_app_id(db: &Database) -> Option<String> {
    AppSettingsQueries::get(db.pool(), APP_ID_KEY)
        .await
        .ok()
        .flatten()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
}

pub async fn set_app_id(db: &Database, raw: &str) -> Result<(), String> {
    let app_id = validate_app_id(raw)?;
    AppSettingsQueries::set(db.pool(), APP_ID_KEY, &app_id)
        .await
        .map_err(|error| error.to_string())
}

pub async fn clear_app_id(db: &Database) -> Result<(), String> {
    AppSettingsQueries::set(db.pool(), APP_ID_KEY, "")
        .await
        .map_err(|error| error.to_string())
}

pub async fn read_tenants(db: &Database) -> Vec<String> {
    AppSettingsQueries::get(db.pool(), TENANTS_KEY)
        .await
        .ok()
        .flatten()
        .map(|value| {
            value
                .split(',')
                .map(|tenant| tenant.trim().to_ascii_lowercase())
                .filter(|tenant| !tenant.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

pub async fn set_tenants(db: &Database, raw: &str) -> Result<(), String> {
    let tenants = validate_tenants(raw)?;
    AppSettingsQueries::set(db.pool(), TENANTS_KEY, &tenants)
        .await
        .map_err(|error| error.to_string())
}

pub async fn clear_tenants(db: &Database) -> Result<(), String> {
    AppSettingsQueries::set(db.pool(), TENANTS_KEY, "")
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

pub async fn status(db: &Database) -> Result<TeamsStatus, String> {
    let links = TeamsQueries::list_links(db.pool())
        .await
        .map_err(|error| error.to_string())?;
    let default_bot = AppSettingsQueries::get(db.pool(), DEFAULT_BOT_KEY)
        .await
        .ok()
        .flatten()
        .filter(|value| !value.trim().is_empty());
    Ok(TeamsStatus {
        enabled: enabled(db).await,
        app_id: read_app_id(db).await,
        app_password_present: read_app_password(db).await.is_some(),
        tenants: read_tenants(db).await,
        default_bot,
        links,
        local_webhook_url: crate::chat_webhook::local_teams_url(),
    })
}

pub async fn create_pairing(
    db: &Database,
    bot_id: Uuid,
    ttl: Duration,
) -> Result<TeamsPairing, String> {
    BotQueries::get(db.pool(), bot_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "That agent does not exist.".to_string())?;
    let code = format!("{:06}", Uuid::new_v4().as_u128() % 1_000_000);
    let ttl = ttl.as_secs().clamp(60, 600);
    TeamsQueries::create_pairing(db.pool(), &hash_code(&code), bot_id, ttl as i64)
        .await
        .map_err(|error| error.to_string())?;
    Ok(TeamsPairing {
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
        "Teams chat".to_string()
    } else {
        title
    }
}

pub fn strip_at_mentions(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut output = String::new();
    let mut index = 0;
    while index < text.len() {
        if lower[index..].starts_with("<at>") {
            if let Some(end) = lower[index..].find("</at>") {
                index += end + "</at>".len();
                continue;
            }
        }
        let ch = text[index..].chars().next().unwrap_or(' ');
        output.push(ch);
        index += ch.len_utf8();
    }
    output.trim().to_string()
}

fn is_addressed(
    conversation_type: &str,
    recipient_id: Option<&str>,
    text: &str,
    entities: Option<&Vec<Value>>,
) -> bool {
    if conversation_type == "personal" {
        return true;
    }
    if let (Some(recipient), Some(entities)) = (recipient_id, entities) {
        if entities.iter().any(|entity| {
            entity.get("type").and_then(|value| value.as_str()) == Some("mention")
                && entity
                    .pointer("/mentioned/id")
                    .and_then(|value| value.as_str())
                    == Some(recipient)
        }) {
            return true;
        }
    }
    text.to_ascii_lowercase().contains("<at>")
}

fn conversation_title(activity: &Value) -> String {
    activity
        .pointer("/conversation/name")
        .or_else(|| activity.pointer("/channelData/channel/name"))
        .and_then(|value| value.as_str())
        .map(|name| format!("Teams: {}", clean_title(name)))
        .unwrap_or_else(|| "Teams chat".to_string())
}

async fn pair_conversation(
    db: &Database,
    code: &str,
    context: &ActivityContext,
) -> Result<Option<String>, String> {
    let Some(bot_id) = TeamsQueries::consume_pairing(db.pool(), &hash_code(code))
        .await
        .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    let bot = BotQueries::get(db.pool(), bot_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "The paired agent no longer exists.".to_string())?;
    let thread = Thread::new(bot_id, context.title.clone());
    ThreadQueries::create(db.pool(), &thread)
        .await
        .map_err(|error| error.to_string())?;
    TeamsQueries::upsert_link(
        db.pool(),
        &context.conversation_id,
        &context.tenant_id,
        Some(&context.conversation_type),
        &context.service_url,
        Some(&context.title),
        bot_id,
        thread.id,
    )
    .await
    .map_err(|error| error.to_string())?;
    Ok(Some(bot.name))
}

#[derive(Clone)]
struct ActivityContext {
    conversation_id: String,
    tenant_id: String,
    conversation_type: String,
    service_url: String,
    activity_id: String,
    title: String,
}

pub(crate) async fn receive(
    state: &crate::chat_webhook::ChatWebhookState,
    headers: &HeaderMap,
    body: &[u8],
) -> Result<(StatusCode, String), String> {
    if !enabled(&state.db).await {
        return Ok((StatusCode::OK, "ignored".to_string()));
    }
    let Some(app_id) = read_app_id(&state.db).await else {
        return Ok((
            StatusCode::SERVICE_UNAVAILABLE,
            "Teams app id is not configured".to_string(),
        ));
    };
    let tenants = read_tenants(&state.db).await;
    let update: Value = match serde_json::from_slice(body) {
        Ok(value) => value,
        Err(_) => return Ok((StatusCode::BAD_REQUEST, "invalid JSON".to_string())),
    };
    let activity_type = update
        .get("type")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    if !matches!(activity_type, "message" | "invoke") {
        return Ok((StatusCode::OK, "ignored".to_string()));
    }
    if update.get("channelId").and_then(|value| value.as_str()) != Some("msteams") {
        return Ok((StatusCode::OK, "ignored".to_string()));
    }
    let Some(service_url) = update.get("serviceUrl").and_then(|value| value.as_str()) else {
        return Ok((StatusCode::BAD_REQUEST, "missing serviceUrl".to_string()));
    };
    let Some(conversation_id) = update
        .pointer("/conversation/id")
        .and_then(|value| value.as_str())
    else {
        return Ok((
            StatusCode::BAD_REQUEST,
            "missing conversation id".to_string(),
        ));
    };
    let token = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let Some(token) = token else {
        return Ok((StatusCode::UNAUTHORIZED, "missing bearer token".to_string()));
    };
    if state
        .teams
        .validate(token, &app_id, service_url)
        .await
        .is_err()
    {
        return Ok((StatusCode::UNAUTHORIZED, "invalid token".to_string()));
    }
    let tenant_id = update
        .pointer("/conversation/tenantId")
        .or_else(|| update.pointer("/channelData/tenant/id"))
        .and_then(|value| value.as_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    if tenant_id.is_empty() || !tenants.iter().any(|tenant| tenant == &tenant_id) {
        return Ok((StatusCode::FORBIDDEN, "tenant is not allowed".to_string()));
    }
    let Some(activity_id) = update.get("id").and_then(|value| value.as_str()) else {
        return Ok((StatusCode::BAD_REQUEST, "missing activity id".to_string()));
    };
    let Some(app_password) = read_app_password(&state.db).await else {
        return Ok((
            StatusCode::SERVICE_UNAVAILABLE,
            "Teams app password is not configured".to_string(),
        ));
    };
    if activity_type == "invoke" {
        if update.get("name").and_then(|value| value.as_str()) != Some("fileConsent/invoke") {
            return Ok((StatusCode::OK, "ignored".to_string()));
        }
        let value = update.get("value").cloned().unwrap_or(Value::Null);
        let state = state.clone();
        let app_id = app_id.clone();
        let service_url = service_url.trim_end_matches('/').to_string();
        let conversation_id = conversation_id.to_string();
        let activity_id = activity_id.to_string();
        tokio::spawn(async move {
            if let Err(error) = handle_file_consent(
                &state,
                &app_id,
                &app_password,
                &service_url,
                &conversation_id,
                &activity_id,
                &value,
            )
            .await
            {
                tracing::warn!(%error, "Teams file consent handling failed");
            }
        });
        return Ok((StatusCode::OK, "{}".to_string()));
    }
    if !TeamsQueries::claim_event(state.db.pool(), conversation_id, activity_id)
        .await
        .map_err(|error| error.to_string())?
    {
        return Ok((StatusCode::OK, "duplicate".to_string()));
    }
    let conversation_type = update
        .pointer("/conversation/conversationType")
        .and_then(|value| value.as_str())
        .unwrap_or("channel")
        .to_string();
    let context = ActivityContext {
        conversation_id: conversation_id.to_string(),
        tenant_id,
        conversation_type,
        service_url: service_url.trim_end_matches('/').to_string(),
        activity_id: activity_id.to_string(),
        title: conversation_title(&update),
    };
    let state = state.clone();
    tokio::spawn(async move {
        if let Err(error) = handle_activity(&state, &app_id, &app_password, &context, &update).await
        {
            tracing::warn!(%error, "Teams activity handling failed");
        }
    });
    Ok((StatusCode::OK, "ok".to_string()))
}

fn teams_card(card: &crate::chat_commands::PendingCard) -> Value {
    use crate::chat_commands::PendingCard;
    match card {
        PendingCard::Approval {
            id,
            title,
            detail,
            risk,
        } => json!({
            "$schema": "http://adaptivecards.io/schemas/adaptive-card.json",
            "type": "AdaptiveCard",
            "version": "1.4",
            "body": [
                { "type": "TextBlock", "size": "Medium", "weight": "Bolder", "text": title },
                { "type": "TextBlock", "text": detail, "wrap": true },
                { "type": "TextBlock", "text": format!("Risk: {risk}"), "wrap": true, "isSubtle": true }
            ],
            "actions": [
                { "type": "Action.Submit", "title": "Approve", "data": { "action": "approve", "id": id.to_string() } },
                { "type": "Action.Submit", "title": "Deny", "data": { "action": "deny", "id": id.to_string() } }
            ]
        }),
        PendingCard::Question {
            id,
            header,
            question,
            options,
        } => {
            let actions: Vec<Value> = options
                .iter()
                .enumerate()
                .map(|(index, option)| {
                    json!({
                        "type": "Action.Submit",
                        "title": option,
                        "data": { "action": "answer", "id": id.to_string(), "index": index }
                    })
                })
                .collect();
            json!({
                "$schema": "http://adaptivecards.io/schemas/adaptive-card.json",
                "type": "AdaptiveCard",
                "version": "1.4",
                "body": [
                    { "type": "TextBlock", "size": "Medium", "weight": "Bolder", "text": header },
                    { "type": "TextBlock", "text": question, "wrap": true },
                    { "type": "TextBlock", "text": format!("Use `answer {id} <text>` for a custom answer."), "wrap": true, "isSubtle": true }
                ],
                "actions": actions
            })
        }
    }
}

async fn teams_attachments(
    state: &crate::chat_webhook::ChatWebhookState,
    app_id: &str,
    app_password: &str,
    activity: &Value,
) -> Vec<Attachment> {
    use crate::chat_attachments::MAX_ATTACHMENTS;
    let Some(items) = activity
        .get("attachments")
        .and_then(|value| value.as_array())
    else {
        return Vec::new();
    };
    let mut attachments = Vec::new();
    let mut token: Option<String> = None;
    for item in items.iter().take(MAX_ATTACHMENTS) {
        let name = item
            .get("name")
            .and_then(|value| value.as_str())
            .unwrap_or("attachment");
        let mime = item.get("contentType").and_then(|value| value.as_str());
        if let Some(content) = item.get("content").and_then(|value| value.as_str()) {
            if let Some(mime) = mime {
                if let Ok(attachment) = crate::chat_attachments::from_base64(name, mime, content) {
                    attachments.push(attachment);
                }
            }
            continue;
        }
        let Some(url) = item.get("contentUrl").and_then(|value| value.as_str()) else {
            continue;
        };
        if token.is_none() {
            token = state.teams.auth.token(app_id, app_password).await.ok();
        }
        let Some(token) = token.as_deref() else {
            break;
        };
        if let Ok(attachment) =
            crate::chat_attachments::download(&state.client, url, Some(token), name, mime, None)
                .await
        {
            attachments.push(attachment);
        }
    }
    attachments
}

#[allow(clippy::too_many_arguments)]
async fn handle_file_consent(
    state: &crate::chat_webhook::ChatWebhookState,
    app_id: &str,
    app_password: &str,
    service_url: &str,
    conversation_id: &str,
    activity_id: &str,
    value: &Value,
) -> Result<(), String> {
    if value.get("type").and_then(|value| value.as_str()) != Some("fileUpload") {
        return Ok(());
    }
    let action = value
        .get("action")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let token = value
        .pointer("/context/token")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    if token.is_empty() {
        return Ok(());
    }
    let dir = consent_dir();
    cleanup_stale_in(&dir);
    match action {
        "accept" => {
            let (pending, bytes) = match read_pending_in(&dir, token, conversation_id) {
                Ok(found) => found,
                Err(error) => {
                    state
                        .teams
                        .send_text(
                            app_id,
                            app_password,
                            service_url,
                            conversation_id,
                            activity_id,
                            &error,
                        )
                        .await?;
                    return Ok(());
                }
            };
            let Some(upload) = value.get("uploadInfo").filter(|value| value.is_object()) else {
                remove_pending_in(&dir, token);
                state
                    .teams
                    .send_text(
                        app_id,
                        app_password,
                        service_url,
                        conversation_id,
                        activity_id,
                        "Teams did not provide an upload location for that file.",
                    )
                    .await?;
                return Ok(());
            };
            let Some(upload_url) = upload.get("uploadUrl").and_then(|value| value.as_str()) else {
                remove_pending_in(&dir, token);
                state
                    .teams
                    .send_text(
                        app_id,
                        app_password,
                        service_url,
                        conversation_id,
                        activity_id,
                        "Teams did not provide an upload location for that file.",
                    )
                    .await?;
                return Ok(());
            };
            let Some(content_url) = upload.get("contentUrl").and_then(|value| value.as_str())
            else {
                remove_pending_in(&dir, token);
                state
                    .teams
                    .send_text(
                        app_id,
                        app_password,
                        service_url,
                        conversation_id,
                        activity_id,
                        "Teams did not provide a file link after the upload.",
                    )
                    .await?;
                return Ok(());
            };
            if let Err(error) = upload_consent_file(&state.client, upload_url, &bytes).await {
                remove_pending_in(&dir, token);
                state
                    .teams
                    .send_text(
                        app_id,
                        app_password,
                        service_url,
                        conversation_id,
                        activity_id,
                        &format!("The file upload failed: {error}"),
                    )
                    .await?;
                return Ok(());
            }
            let name = upload
                .get("name")
                .and_then(|value| value.as_str())
                .unwrap_or(&pending.name);
            let unique_id = upload
                .get("uniqueId")
                .and_then(|value| value.as_str())
                .unwrap_or(token);
            let file_type = upload
                .get("fileType")
                .and_then(|value| value.as_str())
                .or_else(|| pending.mime_type.split('/').next_back())
                .unwrap_or("file");
            remove_pending_in(&dir, token);
            state
                .teams
                .send_file_info(
                    app_id,
                    app_password,
                    service_url,
                    conversation_id,
                    activity_id,
                    name,
                    content_url,
                    unique_id,
                    file_type,
                )
                .await?;
            Ok(())
        }
        "decline" => {
            remove_pending_in(&dir, token);
            state
                .teams
                .send_text(
                    app_id,
                    app_password,
                    service_url,
                    conversation_id,
                    activity_id,
                    "The file was not sent.",
                )
                .await?;
            Ok(())
        }
        _ => Ok(()),
    }
}

async fn handle_activity(
    state: &crate::chat_webhook::ChatWebhookState,
    app_id: &str,
    app_password: &str,
    context: &ActivityContext,
    activity: &Value,
) -> Result<(), String> {
    if activity
        .pointer("/from/role")
        .and_then(|value| value.as_str())
        == Some("bot")
    {
        return Ok(());
    }
    if let Some(value) = activity.get("value").filter(|value| value.is_object()) {
        let action = value
            .get("action")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        if matches!(action, "approve" | "deny" | "answer") {
            let Some(link) =
                TeamsQueries::link_for_conversation(state.db.pool(), &context.conversation_id)
                    .await
                    .map_err(|error| error.to_string())?
            else {
                return Ok(());
            };
            let thread_id = Uuid::parse_str(&link.thread_id).unwrap_or_default();
            let result = if action == "answer" {
                let id = value
                    .get("id")
                    .and_then(|value| value.as_str())
                    .and_then(|value| Uuid::parse_str(value).ok());
                let index = value
                    .get("index")
                    .and_then(|value| value.as_u64())
                    .map(|value| value as usize);
                match (id, index) {
                    (Some(id), Some(index)) => {
                        crate::chat_commands::answer_option(&state.db, thread_id, id, index).await
                    }
                    _ => Err("That answer action is malformed.".to_string()),
                }
            } else {
                let id = value
                    .get("id")
                    .and_then(|value| value.as_str())
                    .and_then(|value| Uuid::parse_str(value).ok());
                match id {
                    Some(id) => {
                        crate::chat_commands::decide_approval(
                            &state.db,
                            &state.runtime,
                            thread_id,
                            id,
                            action == "approve",
                        )
                        .await
                    }
                    None => Err("That approval action is malformed.".to_string()),
                }
            };
            let text = match result {
                Ok(message) => message,
                Err(error) => error,
            };
            state
                .teams
                .send_text(
                    app_id,
                    app_password,
                    &context.service_url,
                    &context.conversation_id,
                    &context.activity_id,
                    &text,
                )
                .await?;
            return Ok(());
        }
    }
    let raw_text = activity
        .get("text")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let actor_id = activity
        .pointer("/from/id")
        .and_then(|value| value.as_str())
        .unwrap_or("teams-user")
        .to_string();
    let actor_name = activity
        .pointer("/from/name")
        .and_then(|value| value.as_str())
        .unwrap_or(&actor_id)
        .to_string();
    let attachments = teams_attachments(state, app_id, app_password, activity).await;
    let recipient_id = activity
        .pointer("/recipient/id")
        .and_then(|value| value.as_str());
    let entities = activity.get("entities").and_then(|value| value.as_array());
    if !is_addressed(&context.conversation_type, recipient_id, raw_text, entities)
        && attachments.is_empty()
    {
        return Ok(());
    }
    let text = strip_at_mentions(raw_text);
    if text.is_empty() && attachments.is_empty() {
        return Ok(());
    }
    let text = if text.is_empty() {
        "(shared files)".to_string()
    } else {
        text
    };
    if !crate::chat_throttle::allow("teams", &context.conversation_id) {
        return Ok(());
    }
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
                state
                    .teams
                    .send_text(
                        app_id,
                        app_password,
                        &context.service_url,
                        &context.conversation_id,
                        &context.activity_id,
                        "Send `pair <code>` with a code from RAVENBOT → Chat apps.",
                    )
                    .await?;
                return Ok(());
            }
            match pair_conversation(&state.db, &code, context).await? {
                Some(name) => {
                    state
                        .teams
                        .send_text(
                            app_id,
                            app_password,
                            &context.service_url,
                            &context.conversation_id,
                            &context.activity_id,
                            &format!(
                                "Paired with {name}. Send a message, `new`, `stop`, `status` or `unlink`."
                            ),
                        )
                        .await?;
                }
                None => {
                    state
                        .teams
                        .send_text(
                            app_id,
                            app_password,
                            &context.service_url,
                            &context.conversation_id,
                            &context.activity_id,
                            "That pairing code is invalid or expired.",
                        )
                        .await?;
                }
            }
            Ok(())
        }
        "new" => {
            let Some(link) =
                TeamsQueries::link_for_conversation(state.db.pool(), &context.conversation_id)
                    .await
                    .map_err(|error| error.to_string())?
            else {
                state
                    .teams
                    .send_text(
                        app_id,
                        app_password,
                        &context.service_url,
                        &context.conversation_id,
                        &context.activity_id,
                        "Pair first with `pair <code>`.",
                    )
                    .await?;
                return Ok(());
            };
            let bot_id = Uuid::parse_str(&link.bot_id).unwrap_or_default();
            let bot = BotQueries::get(state.db.pool(), bot_id)
                .await
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "The paired agent no longer exists.".to_string())?;
            let thread = Thread::new(bot_id, context.title.clone());
            ThreadQueries::create(state.db.pool(), &thread)
                .await
                .map_err(|error| error.to_string())?;
            TeamsQueries::upsert_link(
                state.db.pool(),
                &context.conversation_id,
                &context.tenant_id,
                Some(&context.conversation_type),
                &context.service_url,
                Some(&context.title),
                bot_id,
                thread.id,
            )
            .await
            .map_err(|error| error.to_string())?;
            state
                .teams
                .send_text(
                    app_id,
                    app_password,
                    &context.service_url,
                    &context.conversation_id,
                    &context.activity_id,
                    &format!("Started a new conversation with {}.", bot.name),
                )
                .await?;
            Ok(())
        }
        "stop" => {
            let Some(link) =
                TeamsQueries::link_for_conversation(state.db.pool(), &context.conversation_id)
                    .await
                    .map_err(|error| error.to_string())?
            else {
                state
                    .teams
                    .send_text(
                        app_id,
                        app_password,
                        &context.service_url,
                        &context.conversation_id,
                        &context.activity_id,
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
            .fetch_all(state.db.pool())
            .await
            .map_err(|error| error.to_string())?;
            for (id,) in &active {
                if let Ok(id) = Uuid::parse_str(id) {
                    state.runtime.request_cancel(id);
                }
            }
            state
                .teams
                .send_text(
                    app_id,
                    app_password,
                    &context.service_url,
                    &context.conversation_id,
                    &context.activity_id,
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
            let Some(link) =
                TeamsQueries::link_for_conversation(state.db.pool(), &context.conversation_id)
                    .await
                    .map_err(|error| error.to_string())?
            else {
                state
                    .teams
                    .send_text(
                        app_id,
                        app_password,
                        &context.service_url,
                        &context.conversation_id,
                        &context.activity_id,
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
            .fetch_one(state.db.pool())
            .await
            .map_err(|error| error.to_string())?;
            let bot = BotQueries::get(
                state.db.pool(),
                Uuid::parse_str(&link.bot_id).unwrap_or_default(),
            )
            .await
            .ok()
            .flatten()
            .map(|bot| bot.name)
            .unwrap_or_else(|| "an agent".to_string());
            state
                .teams
                .send_text(
                    app_id,
                    app_password,
                    &context.service_url,
                    &context.conversation_id,
                    &context.activity_id,
                    &format!("Paired with {bot}. {active} active run(s)."),
                )
                .await?;
            Ok(())
        }
        "unlink" => {
            let removed = TeamsQueries::unlink(state.db.pool(), &context.conversation_id)
                .await
                .map_err(|error| error.to_string())?;
            state
                .teams
                .send_text(
                    app_id,
                    app_password,
                    &context.service_url,
                    &context.conversation_id,
                    &context.activity_id,
                    if removed {
                        "This conversation is no longer paired."
                    } else {
                        "This conversation was not paired."
                    },
                )
                .await?;
            Ok(())
        }
        "approve" | "deny" | "answer" => {
            let Some(link) =
                TeamsQueries::link_for_conversation(state.db.pool(), &context.conversation_id)
                    .await
                    .map_err(|error| error.to_string())?
            else {
                state
                    .teams
                    .send_text(
                        app_id,
                        app_password,
                        &context.service_url,
                        &context.conversation_id,
                        &context.activity_id,
                        "Pair this conversation before deciding approvals.",
                    )
                    .await?;
                return Ok(());
            };
            let thread_id = Uuid::parse_str(&link.thread_id).unwrap_or_default();
            let reply =
                match crate::chat_commands::handle(&state.db, &state.runtime, thread_id, &text)
                    .await
                {
                    Some(Ok(message)) => message,
                    Some(Err(error)) => error,
                    None => "Unknown command.".to_string(),
                };
            state
                .teams
                .send_text(
                    app_id,
                    app_password,
                    &context.service_url,
                    &context.conversation_id,
                    &context.activity_id,
                    &reply,
                )
                .await?;
            Ok(())
        }
        _ => {
            handle_text(
                state,
                app_id,
                app_password,
                context,
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
    state: &crate::chat_webhook::ChatWebhookState,
    app_id: &str,
    app_password: &str,
    context: &ActivityContext,
    text: &str,
    attachments: Vec<Attachment>,
    actor_id: &str,
    actor_name: &str,
) -> Result<(), String> {
    let Some(link) = TeamsQueries::link_for_conversation(state.db.pool(), &context.conversation_id)
        .await
        .map_err(|error| error.to_string())?
    else {
        state
            .teams
            .send_text(
                app_id,
                app_password,
                &context.service_url,
                &context.conversation_id,
                &context.activity_id,
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
    .fetch_one(state.db.pool())
    .await
    .map_err(|error| error.to_string())?;
    if active > 0 {
        state
            .teams
            .send_text(
                app_id,
                app_password,
                &context.service_url,
                &context.conversation_id,
                &context.activity_id,
                "Still working on the previous message. Send `stop` to cancel it.",
            )
            .await?;
        return Ok(());
    }
    let content = text.chars().take(20_000).collect::<String>();
    let mut user = Message::user(thread_id, content);
    user.sender_name = Some(actor_name.to_string());
    user.attachments = attachments;
    MessageQueries::insert(state.db.pool(), &user)
        .await
        .map_err(|error| error.to_string())?;
    let mut run = Run::new(bot_id, thread_id);
    RunQueries::insert(state.db.pool(), &run)
        .await
        .map_err(|error| error.to_string())?;
    let _ = crate::chat_commands::attribute_run(&state.db, run.id, actor_id, actor_name).await;
    let _ = TeamsQueries::touch(state.db.pool(), &context.conversation_id).await;
    let state = state.clone();
    let app_id = app_id.to_string();
    let app_password = app_password.to_string();
    let context = context.clone();
    tokio::spawn(async move {
        let auth = state.teams.clone();
        state
            .runtime
            .set_thread_emitter(thread_id, state.events.stream_emitter());
        state.runtime.review_approvals_for_run(run.id, true);
        let monitor = {
            let db = state.db.clone();
            let auth = auth.clone();
            let app_id = app_id.clone();
            let app_password = app_password.clone();
            let context = context.clone();
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
                            let card = teams_card(&card);
                            let _ = auth
                                .send_card(
                                    &app_id,
                                    &app_password,
                                    &context.service_url,
                                    &context.conversation_id,
                                    &context.activity_id,
                                    &card,
                                )
                                .await;
                        } else {
                            let _ = auth
                                .send_text(
                                    &app_id,
                                    &app_password,
                                    &context.service_url,
                                    &context.conversation_id,
                                    &context.activity_id,
                                    "This run needs a decision. Open RAVENBOT or the paired remote page.",
                                )
                                .await;
                        }
                        break;
                    }
                }
            })
        };
        let result = state.runtime.execute_run(&mut run).await;
        monitor.abort();
        state.runtime.review_approvals_for_run(run.id, false);
        state.runtime.set_thread_emitter(thread_id, None);
        let (reply, attachments) = match result {
            Ok(()) => crate::chat_outbound::last_assistant_reply(&state.db, thread_id)
                .await
                .unwrap_or_else(|| {
                    (
                        "The run finished without an answer.".to_string(),
                        Vec::new(),
                    )
                }),
            Err(error) => (format!("The run failed: {error}"), Vec::new()),
        };
        if let Err(error) = auth
            .send_text(
                &app_id,
                &app_password,
                &context.service_url,
                &context.conversation_id,
                &context.activity_id,
                &reply,
            )
            .await
        {
            tracing::warn!(%error, "Teams reply failed");
        }
        for attachment in attachments.into_iter().take(MAX_OUTBOUND_FILES) {
            let Ok(file) = crate::chat_outbound::OutboundFile::from_attachment(&attachment) else {
                continue;
            };
            if let Err(error) = auth
                .send_file_consent(
                    &app_id,
                    &app_password,
                    &context.service_url,
                    &context.conversation_id,
                    &context.activity_id,
                    &file,
                )
                .await
            {
                tracing::warn!(%error, "Teams file consent card failed");
            }
        }
    });
    Ok(())
}

/// Teams messages are plain text; keep each activity below the practical limit.
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
    use axum::body::Bytes;
    use axum::extract::{OriginalUri, Path as AxumPath, State as AxumState};
    use axum::http::HeaderMap as AxumHeaders;
    use axum::routing::{get, post, put};
    use axum::{Json, Router};
    use jsonwebtoken::{encode, EncodingKey, Header};
    use ravenbot_runtime::Runtime;
    use std::sync::Arc as StdArc;

    const TEST_KEY: &[u8] = include_bytes!("../tests/fixtures/teams_test_key.pem");
    const TEST_JWKS: &str = include_str!("../tests/fixtures/teams_jwks.json");
    const APP_ID: &str = "11111111-1111-1111-1111-111111111111";
    const TENANT_ID: &str = "22222222-2222-2222-2222-222222222222";

    #[derive(Clone)]
    struct MockTeams {
        base: String,
        paths: StdArc<std::sync::Mutex<Vec<String>>>,
        jwks: String,
    }

    async fn mock_openid(AxumState(state): AxumState<MockTeams>) -> Json<Value> {
        Json(json!({ "jwks_uri": format!("{}/jwks", state.base) }))
    }

    async fn mock_jwks(AxumState(state): AxumState<MockTeams>) -> Json<Value> {
        let value: Value = serde_json::from_str(&state.jwks).expect("jwks");
        Json(value)
    }

    async fn mock_token(
        AxumState(state): AxumState<MockTeams>,
        OriginalUri(uri): OriginalUri,
    ) -> Json<Value> {
        state.paths.lock().unwrap().push(uri.path().to_string());
        Json(json!({ "access_token": "connector-token", "expires_in": 3600 }))
    }

    async fn mock_activity(
        AxumState(state): AxumState<MockTeams>,
        AxumPath((_conversation, _activity)): AxumPath<(String, String)>,
        OriginalUri(uri): OriginalUri,
    ) -> Json<Value> {
        state.paths.lock().unwrap().push(uri.path().to_string());
        Json(json!({ "id": "reply-1" }))
    }

    async fn mock_server() -> (
        String,
        StdArc<std::sync::Mutex<Vec<String>>>,
        tokio::task::JoinHandle<()>,
    ) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let base = format!("http://{}", listener.local_addr().expect("address"));
        let paths = StdArc::new(std::sync::Mutex::new(Vec::new()));
        let state = MockTeams {
            base: base.clone(),
            paths: paths.clone(),
            jwks: TEST_JWKS.to_string(),
        };
        let app = Router::new()
            .route("/openid", get(mock_openid))
            .route("/jwks", get(mock_jwks))
            .route("/token", post(mock_token))
            .route(
                "/v3/conversations/{conversation}/activities/{activity}",
                post(mock_activity),
            )
            .with_state(state);
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (base, paths, task)
    }

    fn sign(app_id: &str, service_url: &str) -> String {
        let now = chrono::Utc::now().timestamp() as usize;
        let claims = json!({
            "aud": app_id,
            "iss": BOT_ISSUER,
            "exp": now + 3600,
            "nbf": now.saturating_sub(60),
            "serviceurl": service_url,
        });
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some("test-key-1".to_string());
        encode(
            &header,
            &claims,
            &EncodingKey::from_rsa_pem(TEST_KEY).expect("test key"),
        )
        .expect("token")
    }

    #[test]
    fn teams_ids_and_tenants_are_validated() {
        assert_eq!(validate_app_id(APP_ID).expect("app"), APP_ID);
        assert!(validate_app_id("not-a-guid").is_err());
        assert_eq!(
            validate_tenants(&format!("{TENANT_ID}, {TENANT_ID}")).expect("tenants"),
            TENANT_ID
        );
        assert!(validate_tenants("").is_err());
        assert!(validate_tenants("nope").is_err());
        assert!(validate_app_password(&"a".repeat(32)).is_ok());
        assert!(validate_app_password("short").is_err());
    }

    #[test]
    fn teams_mentions_and_message_splitting_are_handled() {
        assert_eq!(
            strip_at_mentions("<at>RAVENBOT</at> pair 123456"),
            "pair 123456"
        );
        assert_eq!(strip_at_mentions("  new  "), "new");
        assert!(is_addressed("personal", None, "hello", None));
        assert!(!is_addressed("channel", Some("28:bot"), "hello", None));
        assert!(is_addressed(
            "channel",
            Some("28:bot"),
            "<at>Bot</at> status",
            None
        ));
        let chunks = split_message(&"x".repeat(9000));
        assert_eq!(chunks.len(), 3);
        assert!(chunks.iter().all(|chunk| chunk.chars().count() <= 3500));
    }

    #[tokio::test]
    async fn bot_framework_tokens_are_checked_against_keys_audience_and_service_url() {
        let (base, _paths, task) = mock_server().await;
        let validator = TeamsJwtValidator::new(
            reqwest::Client::new(),
            format!("{base}/openid"),
            BOT_ISSUER.to_string(),
        );
        let token = sign(APP_ID, &base);
        validator
            .validate(&token, APP_ID, &base)
            .await
            .expect("valid token");
        assert!(validator
            .validate(&token, "33333333-3333-3333-3333-333333333333", &base)
            .await
            .is_err());
        assert!(validator
            .validate(&token, APP_ID, "https://evil.example")
            .await
            .is_err());
        assert!(validator
            .validate(&format!("{token}x"), APP_ID, &base)
            .await
            .is_err());
        task.abort();
    }

    #[tokio::test]
    async fn connector_tokens_are_cached_between_replies() {
        let (base, paths, task) = mock_server().await;
        let auth = TeamsAuth::new(reqwest::Client::new(), format!("{base}/token"));
        let first = auth
            .token(APP_ID, "teams-app-password-0123456789")
            .await
            .expect("first token");
        let second = auth
            .token(APP_ID, "teams-app-password-0123456789")
            .await
            .expect("cached token");
        assert_eq!(first, second);
        assert_eq!(
            paths
                .lock()
                .unwrap()
                .iter()
                .filter(|path| path.as_str() == "/token")
                .count(),
            1
        );
        task.abort();
    }

    #[tokio::test]
    async fn the_teams_webhook_pairs_a_conversation_and_replies() {
        use ravenbot_core::Bot;
        use ravenbot_db::queries::BotQueries;

        let path = std::env::temp_dir().join(format!("ravenbot-teams-{}.db", Uuid::new_v4()));
        let db = Database::new(&path).await.expect("db");
        let bot = Bot::new("Teams Bot", "chat test");
        BotQueries::insert(db.pool(), &bot).await.expect("bot");
        let pairing = create_pairing(&db, bot.id, Duration::from_secs(300))
            .await
            .expect("pairing");
        let password = "teams-app-password-0123456789";
        set_app_id(&db, APP_ID).await.expect("app id");
        set_app_password(&db, password).await.expect("password");
        set_tenants(&db, TENANT_ID).await.expect("tenants");
        AppSettingsQueries::set(db.pool(), ENABLED_KEY, "1")
            .await
            .expect("enabled");

        let (base, paths, task) = mock_server().await;
        let runtime = StdArc::new(Runtime::new(db.clone()));
        let server = crate::chat_webhook::ChatWebhookServer::start(
            db.clone(),
            runtime,
            crate::HostEvents::Headless,
            crate::chat_webhook::ChatWebhookConfig {
                bind: "127.0.0.1".into(),
                port: 0,
                whatsapp_api_base: None,
                teams_token_url: Some(format!("{base}/token")),
                teams_openid_url: Some(format!("{base}/openid")),
            },
        )
        .await
        .expect("webhook server");
        let token = sign(APP_ID, &base);
        let activity = json!({
            "type": "message",
            "id": "activity-1",
            "channelId": "msteams",
            "serviceUrl": base,
            "from": { "id": "user-1", "name": "Ada", "role": "user" },
            "recipient": { "id": "28:bot", "name": "RAVENBOT" },
            "conversation": {
                "id": "conv-1",
                "conversationType": "personal",
                "tenantId": TENANT_ID,
                "name": "Ada"
            },
            "text": format!("pair {}", pairing.code)
        });
        let response = reqwest::Client::new()
            .post(format!("http://{}/teams", server.addr()))
            .header("authorization", format!("Bearer {token}"))
            .json(&activity)
            .send()
            .await
            .expect("webhook");
        assert_eq!(response.status(), 200);

        let deadline = tokio::time::Instant::now() + Duration::from_secs(4);
        loop {
            if TeamsQueries::link_for_conversation(db.pool(), "conv-1")
                .await
                .expect("lookup")
                .is_some()
            {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                panic!("Teams webhook did not pair the conversation");
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        loop {
            if paths
                .lock()
                .unwrap()
                .iter()
                .any(|path| path.contains("/v3/conversations/conv-1/activities/activity-1"))
            {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                panic!("Teams webhook did not send a reply");
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(paths.lock().unwrap().iter().any(|path| path == "/token"));
        drop(server);
        task.abort();
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn pending_team_files_are_scoped_and_removed() {
        let dir = std::env::temp_dir().join(format!("ravenbot-teams-pending-{}", Uuid::new_v4()));
        let file = crate::chat_outbound::OutboundFile {
            name: "notes.txt".to_string(),
            mime_type: "text/plain".to_string(),
            bytes: b"hello".to_vec(),
            is_image: false,
        };
        let pending = write_pending_in(&dir, "conv-1", &file).expect("pending");
        let (stored, bytes) = read_pending_in(&dir, &pending.token, "conv-1").expect("stored file");
        assert_eq!(stored.name, "notes.txt");
        assert_eq!(bytes, b"hello");
        assert!(read_pending_in(&dir, &pending.token, "conv-2").is_err());
        assert!(read_pending_in(&dir, "../escape", "conv-1").is_err());
        remove_pending_in(&dir, &pending.token);
        assert!(read_pending_in(&dir, &pending.token, "conv-1").is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    type UploadLog = StdArc<std::sync::Mutex<Vec<(String, Vec<u8>)>>>;

    #[derive(Clone)]
    struct MockFileServer {
        base: String,
        activities: StdArc<std::sync::Mutex<Vec<Value>>>,
        uploads: UploadLog,
        jwks: String,
    }

    async fn mock_file_openid(AxumState(state): AxumState<MockFileServer>) -> Json<Value> {
        Json(json!({ "jwks_uri": format!("{}/jwks", state.base) }))
    }

    async fn mock_file_jwks(AxumState(state): AxumState<MockFileServer>) -> Json<Value> {
        let value: Value = serde_json::from_str(&state.jwks).expect("jwks");
        Json(value)
    }

    async fn mock_file_token() -> Json<Value> {
        Json(json!({ "access_token": "connector-token", "expires_in": 3600 }))
    }

    async fn mock_file_activity(
        AxumState(state): AxumState<MockFileServer>,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        state.activities.lock().unwrap().push(body);
        Json(json!({ "id": "reply-1" }))
    }

    async fn mock_file_upload(
        AxumState(state): AxumState<MockFileServer>,
        headers: AxumHeaders,
        body: Bytes,
    ) -> StatusCode {
        let range = headers
            .get("content-range")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .to_string();
        state.uploads.lock().unwrap().push((range, body.to_vec()));
        StatusCode::OK
    }

    async fn mock_file_server() -> (
        String,
        StdArc<std::sync::Mutex<Vec<Value>>>,
        UploadLog,
        tokio::task::JoinHandle<()>,
    ) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let base = format!("http://{}", listener.local_addr().expect("address"));
        let activities = StdArc::new(std::sync::Mutex::new(Vec::new()));
        let uploads = StdArc::new(std::sync::Mutex::new(Vec::new()));
        let state = MockFileServer {
            base: base.clone(),
            activities: activities.clone(),
            uploads: uploads.clone(),
            jwks: TEST_JWKS.to_string(),
        };
        let app = Router::new()
            .route("/openid", get(mock_file_openid))
            .route("/jwks", get(mock_file_jwks))
            .route("/token", post(mock_file_token))
            .route(
                "/v3/conversations/{conversation}/activities/{activity}",
                post(mock_file_activity),
            )
            .route("/upload", put(mock_file_upload))
            .with_state(state);
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (base, activities, uploads, task)
    }

    #[tokio::test]
    async fn teams_file_consent_cards_upload_after_acceptance() {
        let path = std::env::temp_dir().join(format!("ravenbot-teams-files-{}.db", Uuid::new_v4()));
        let db = Database::new(&path).await.expect("db");
        let password = "teams-app-password-0123456789";
        set_app_id(&db, APP_ID).await.expect("app id");
        set_app_password(&db, password).await.expect("password");
        set_tenants(&db, TENANT_ID).await.expect("tenants");
        AppSettingsQueries::set(db.pool(), ENABLED_KEY, "1")
            .await
            .expect("enabled");

        let outbound_dir =
            std::env::temp_dir().join(format!("ravenbot-teams-outbound-{}", Uuid::new_v4()));
        let _ = TEST_OUTBOUND_DIR.set(outbound_dir.clone());
        let outbound_dir = consent_dir();

        let (base, activities, uploads, task) = mock_file_server().await;
        let service = TeamsService::new(
            reqwest::Client::new(),
            Some(format!("{base}/token")),
            Some(format!("{base}/openid")),
        );
        let file = crate::chat_outbound::OutboundFile {
            name: "notes.txt".to_string(),
            mime_type: "text/plain".to_string(),
            bytes: b"hello".to_vec(),
            is_image: false,
        };
        service
            .send_file_consent(APP_ID, password, &base, "conv-1", "activity-1", &file)
            .await
            .expect("consent card");

        let token = wait_for_consent_token(&activities).await;
        let runtime = StdArc::new(Runtime::new(db.clone()));
        let server = crate::chat_webhook::ChatWebhookServer::start(
            db.clone(),
            runtime,
            crate::HostEvents::Headless,
            crate::chat_webhook::ChatWebhookConfig {
                bind: "127.0.0.1".into(),
                port: 0,
                whatsapp_api_base: None,
                teams_token_url: Some(format!("{base}/token")),
                teams_openid_url: Some(format!("{base}/openid")),
            },
        )
        .await
        .expect("webhook server");

        let signed = sign(APP_ID, &base);
        let invoke = json!({
            "type": "invoke",
            "id": "invoke-1",
            "channelId": "msteams",
            "name": "fileConsent/invoke",
            "serviceUrl": base,
            "from": { "id": "user-1", "name": "Ada", "role": "user" },
            "recipient": { "id": "28:bot", "name": "RAVENBOT" },
            "conversation": {
                "id": "conv-1",
                "conversationType": "personal",
                "tenantId": TENANT_ID,
                "name": "Ada"
            },
            "value": {
                "type": "fileUpload",
                "action": "accept",
                "context": { "token": token },
                "uploadInfo": {
                    "contentUrl": format!("{base}/content/notes.txt"),
                    "name": "notes.txt",
                    "uploadUrl": format!("{base}/upload"),
                    "uniqueId": "unique-1",
                    "fileType": "txt"
                }
            }
        });
        let response = reqwest::Client::new()
            .post(format!("http://{}/teams", server.addr()))
            .header("authorization", format!("Bearer {signed}"))
            .json(&invoke)
            .send()
            .await
            .expect("invoke");
        assert_eq!(response.status(), 200);

        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            let done = activities.lock().unwrap().iter().any(|body| {
                body.pointer("/attachments/0/contentType")
                    .and_then(|value| value.as_str())
                    == Some("application/vnd.microsoft.teams.card.file.info")
            });
            if done {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                panic!("Teams file info card was not sent");
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let uploads = uploads.lock().unwrap();
        assert_eq!(uploads.len(), 1);
        assert_eq!(uploads[0].0, "bytes 0-4/5");
        assert_eq!(uploads[0].1, b"hello");
        assert!(!outbound_dir.join(format!("{token}.json")).exists());
        drop(server);
        task.abort();
        let _ = std::fs::remove_dir_all(outbound_dir);
        let _ = std::fs::remove_file(&path);
    }

    async fn wait_for_consent_token(activities: &StdArc<std::sync::Mutex<Vec<Value>>>) -> String {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            let token = activities.lock().unwrap().iter().find_map(|body| {
                if body
                    .pointer("/attachments/0/contentType")
                    .and_then(|value| value.as_str())
                    != Some("application/vnd.microsoft.teams.card.file.consent")
                {
                    return None;
                }
                body.pointer("/attachments/0/content/acceptContext/token")
                    .and_then(|value| value.as_str())
                    .map(str::to_string)
            });
            if let Some(token) = token {
                return token;
            }
            if tokio::time::Instant::now() >= deadline {
                panic!("Teams consent card was not sent");
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}
