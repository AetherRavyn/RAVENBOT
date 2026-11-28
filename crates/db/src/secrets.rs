//! Protected storage for provider credentials.
//!
//! The OS keychain is preferred. When it is unavailable (a headless Linux box
//! without a Secret Service, for example), the value is encrypted with a
//! private 256-bit key under `config/sealed-secrets` and the database keeps a
//! marker. Whenever either store is writable, new secrets are never written as
//! plaintext, and startup migrates legacy rows. The encrypted fallback protects copied databases and backups;
//! it cannot protect a host where an attacker can read the user's config
//! directory, which is the boundary the OS keychain exists to improve on.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use keyring::Entry;
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::PathBuf;

pub const KEYCHAIN_MARKER: &str = "__RAVENBOT_KEYCHAIN__";
const SERVICE: &str = "com.ravenbot.app";

#[cfg(test)]
static FORCE_FALLBACK_FOR_TESTS: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[cfg(test)]
static TEST_VAULT_ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Keep tests off the real OS keychain, even when one is available.
#[cfg(test)]
pub(crate) fn force_fallback_for_tests() {
    FORCE_FALLBACK_FOR_TESTS.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// Keep sealed test entries under a temporary directory.
#[cfg(test)]
pub(crate) fn use_test_vault_root(root: PathBuf) {
    let _ = TEST_VAULT_ROOT.set(root);
}

fn disabled() -> bool {
    #[cfg(test)]
    if FORCE_FALLBACK_FOR_TESTS.load(std::sync::atomic::Ordering::SeqCst) {
        return true;
    }
    std::env::var("RAVENBOT_DISABLE_KEYCHAIN")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn namespace() -> String {
    std::env::var("RAVENBOT_KEYCHAIN_NAMESPACE")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_default()
}

fn namespaced(account: &str, namespace: &str) -> String {
    if namespace.is_empty() {
        account.to_string()
    } else {
        format!("{namespace}:{account}")
    }
}

fn entry(account: &str) -> Result<Entry, keyring::Error> {
    Entry::new(SERVICE, account)
}

/// Encrypted fallback for hosts without an OS keychain.
///
/// One random 256-bit key lives in `config/sealed-secrets/key` (0600); every
/// account is a separate XChaCha20-Poly1305 file named by the SHA-256 of its
/// account name. The directory is private, files are written through a temp
/// file and renamed, and the plaintext never enters the database. A copied
/// `ravenbot.db` therefore no longer yields provider, plugin, MCP or signing
/// secrets on a keychain-less machine.
///
/// This protects data at rest from a copied/backed-up DB. It does not pretend
/// to protect a host where an attacker can read the user's config directory;
/// that is the boundary the OS keychain exists to improve on.
pub struct SealedVault {
    root: PathBuf,
}

impl SealedVault {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn entries(&self) -> PathBuf {
        self.root.join("entries")
    }

    fn key_path(&self) -> PathBuf {
        self.root.join("key")
    }

    fn prepare(&self) -> Result<(), String> {
        std::fs::create_dir_all(self.entries())
            .map_err(|error| format!("Could not create the sealed-secret store: {error}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.root, std::fs::Permissions::from_mode(0o700));
            let _ =
                std::fs::set_permissions(self.entries(), std::fs::Permissions::from_mode(0o700));
        }
        Ok(())
    }

    fn read_key(&self) -> Result<[u8; 32], String> {
        let mut file = std::fs::File::open(self.key_path())
            .map_err(|error| format!("The sealed-secret key is unavailable: {error}"))?;
        let mut key = [0u8; 32];
        file.read_exact(&mut key)
            .map_err(|error| format!("The sealed-secret key is invalid: {error}"))?;
        Ok(key)
    }

    fn load_or_create_key(&self) -> Result<[u8; 32], String> {
        self.prepare()?;
        let path = self.key_path();
        let mut options = std::fs::OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&path) {
            Ok(mut file) => {
                let mut key = [0u8; 32];
                rand::rngs::OsRng.fill_bytes(&mut key);
                file.write_all(&key)
                    .and_then(|_| file.sync_all())
                    .map_err(|error| format!("Could not persist the sealed-secret key: {error}"))?;
                Ok(key)
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => self.read_key(),
            Err(error) => Err(format!("Could not create the sealed-secret key: {error}")),
        }
    }

    fn entry_path(&self, account: &str) -> PathBuf {
        let digest = Sha256::digest(account.as_bytes());
        let name: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
        self.entries().join(format!("{name}.bin"))
    }

    fn cipher(key: &[u8; 32]) -> Result<XChaCha20Poly1305, String> {
        XChaCha20Poly1305::new_from_slice(key)
            .map_err(|_| "The sealed-secret key has the wrong length.".to_string())
    }

    pub fn put(&self, account: &str, value: &str) -> Result<(), String> {
        let key = self.load_or_create_key()?;
        let cipher = Self::cipher(&key)?;
        let mut nonce = [0u8; 24];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        let ciphertext = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: value.as_bytes(),
                    aad: account.as_bytes(),
                },
            )
            .map_err(|_| "Could not encrypt the secret.".to_string())?;
        let mut payload = Vec::with_capacity(nonce.len() + ciphertext.len());
        payload.extend_from_slice(&nonce);
        payload.extend_from_slice(&ciphertext);

        let path = self.entry_path(account);
        let temp = self
            .entries()
            .join(format!(".{}.tmp", uuid::Uuid::new_v4().simple()));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let write_result = (|| -> Result<(), String> {
            let mut file = options
                .open(&temp)
                .map_err(|error| format!("Could not write the sealed secret: {error}"))?;
            file.write_all(&payload)
                .and_then(|_| file.sync_all())
                .map_err(|error| format!("Could not persist the sealed secret: {error}"))?;
            match std::fs::rename(&temp, &path) {
                Ok(()) => Ok(()),
                Err(_error) if path.exists() => {
                    // Windows does not replace an existing destination with
                    // rename; the Unix path above stays atomic.
                    std::fs::remove_file(&path).map_err(|remove_error| {
                        format!("Could not replace the sealed secret: {remove_error}")
                    })?;
                    std::fs::rename(&temp, &path).map_err(|rename_error| {
                        format!("Could not replace the sealed secret: {rename_error}")
                    })
                }
                Err(error) => Err(format!("Could not replace the sealed secret: {error}")),
            }?;
            Ok(())
        })();
        if write_result.is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        write_result
    }

    pub fn get(&self, account: &str) -> Option<String> {
        let key = self.read_key().ok()?;
        let cipher = Self::cipher(&key).ok()?;
        let payload = std::fs::read(self.entry_path(account)).ok()?;
        if payload.len() <= 24 {
            return None;
        }
        let (nonce, ciphertext) = payload.split_at(24);
        let plaintext = cipher
            .decrypt(
                XNonce::from_slice(nonce),
                Payload {
                    msg: ciphertext,
                    aad: account.as_bytes(),
                },
            )
            .ok()?;
        String::from_utf8(plaintext).ok()
    }

    pub fn delete(&self, account: &str) {
        let _ = std::fs::remove_file(self.entry_path(account));
    }

    /// The key path, exposed so a diagnostic can report whether the fallback
    /// exists without reading the key itself.
    pub fn key_path_for_diagnostics(&self) -> PathBuf {
        self.key_path()
    }
}

fn fallback_vault() -> SealedVault {
    #[cfg(test)]
    if let Some(root) = TEST_VAULT_ROOT.get() {
        return SealedVault::new(root.clone());
    }
    let root = std::env::var("RAVENBOT_SECRET_VAULT")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            ravenbot_core::raven_root()
                .join("config")
                .join("sealed-secrets")
        });
    SealedVault::new(root)
}

async fn set_secret(account: String, value: String) -> bool {
    let account = namespaced(&account, &namespace());
    if !disabled() {
        let keychain_account = account.clone();
        let keychain_value = value.clone();
        let stored = tokio::task::spawn_blocking(move || match entry(&keychain_account) {
            Ok(entry) => entry.set_password(&keychain_value).is_ok(),
            Err(_) => false,
        })
        .await
        .unwrap_or(false);
        if stored {
            return true;
        }
    }
    let vault = fallback_vault();
    tokio::task::spawn_blocking(move || vault.put(&account, &value).is_ok())
        .await
        .unwrap_or(false)
}

async fn get_secret(account: String) -> Option<String> {
    let account = namespaced(&account, &namespace());
    if !disabled() {
        let keychain_account = account.clone();
        if let Some(value) = tokio::task::spawn_blocking(move || match entry(&keychain_account) {
            Ok(entry) => entry.get_password().ok(),
            Err(_) => None,
        })
        .await
        .ok()
        .flatten()
        {
            return Some(value);
        }
    }
    let vault = fallback_vault();
    tokio::task::spawn_blocking(move || vault.get(&account))
        .await
        .ok()
        .flatten()
}

async fn delete_secret(account: String) {
    let account = namespaced(&account, &namespace());
    if !disabled() {
        let keychain_account = account.clone();
        let _ = tokio::task::spawn_blocking(move || {
            if let Ok(entry) = entry(&keychain_account) {
                let _ = entry.delete_credential();
            }
        })
        .await;
    }
    let vault = fallback_vault();
    let _ = tokio::task::spawn_blocking(move || vault.delete(&account)).await;
}

/// Move legacy plaintext secrets out of the database.
///
/// New writes never put plaintext in SQLite: when the OS keychain is absent,
/// `set_secret` seals the value in the encrypted vault and the row keeps a
/// marker. This pass covers databases written before that fallback existed.
/// It is best-effort and idempotent; a row is only replaced after the secret is
/// safely stored.
pub async fn migrate_plaintext_at_rest(pool: &sqlx::SqlitePool) -> Result<usize, String> {
    let mut migrated = 0usize;

    let provider_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT provider, api_key FROM provider_keys
         WHERE api_key != '' AND api_key != ?",
    )
    .bind(KEYCHAIN_MARKER)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    for (provider, value) in provider_rows {
        if set_secret(format!("provider:{provider}"), value).await {
            let _ = sqlx::query(
                "UPDATE provider_keys SET api_key = ?, updated_at = datetime('now') WHERE provider = ?",
            )
            .bind(KEYCHAIN_MARKER)
            .bind(&provider)
            .execute(pool)
            .await;
            migrated += 1;
        }
    }

    let vault_rows: Vec<(String, String)> =
        sqlx::query_as("SELECT id, secret FROM vault_items WHERE secret != '' AND secret != ?")
            .bind(KEYCHAIN_MARKER)
            .fetch_all(pool)
            .await
            .unwrap_or_default();
    for (id, value) in vault_rows {
        if set_secret(format!("vault:{id}"), value).await {
            let _ = sqlx::query("UPDATE vault_items SET secret = ?, updated_at = ? WHERE id = ?")
                .bind(KEYCHAIN_MARKER)
                .bind(chrono::Utc::now().to_rfc3339())
                .bind(&id)
                .execute(pool)
                .await;
            migrated += 1;
        }
    }

    let plugin_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT plugin_id, header_value FROM plugin_credentials
         WHERE header_value != '' AND header_value != ?",
    )
    .bind(KEYCHAIN_MARKER)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    for (plugin_id, value) in plugin_rows {
        if set_secret(format!("plugin:{plugin_id}"), value).await {
            let _ =
                sqlx::query("UPDATE plugin_credentials SET header_value = ? WHERE plugin_id = ?")
                    .bind(KEYCHAIN_MARKER)
                    .bind(&plugin_id)
                    .execute(pool)
                    .await;
            migrated += 1;
        }
    }

    let provider_header_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, headers_json FROM custom_providers
         WHERE headers_json IS NOT NULL AND headers_json != ''
           AND headers_json != ?",
    )
    .bind(KEYCHAIN_MARKER)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    for (id, value) in provider_header_rows {
        if set_secret(format!("custom-provider:{id}:headers"), value).await {
            let _ = sqlx::query(
                "UPDATE custom_providers SET headers_json = ?, updated_at = datetime('now') WHERE id = ?",
            )
            .bind(KEYCHAIN_MARKER)
            .bind(&id)
            .execute(pool)
            .await;
            migrated += 1;
        }
    }

    let mcp_rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT server_id, key, value FROM mcp_server_env
         WHERE value != '' AND value != ?",
    )
    .bind(KEYCHAIN_MARKER)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    for (server_id, key, value) in mcp_rows {
        if set_secret(format!("mcp:{server_id}:{key}"), value).await {
            let _ = sqlx::query(
                "UPDATE mcp_server_env SET value = ?, updated_at = datetime('now')
                 WHERE server_id = ? AND key = ?",
            )
            .bind(KEYCHAIN_MARKER)
            .bind(&server_id)
            .bind(&key)
            .execute(pool)
            .await;
            migrated += 1;
        }
    }

    let webhook_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, webhook_secret FROM routines
         WHERE webhook_secret IS NOT NULL AND webhook_secret != ''
           AND webhook_secret != ?",
    )
    .bind(KEYCHAIN_MARKER)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    for (id, value) in webhook_rows {
        if set_secret(format!("webhook:{id}"), value).await {
            let _ = sqlx::query("UPDATE routines SET webhook_secret = ? WHERE id = ?")
                .bind(KEYCHAIN_MARKER)
                .bind(&id)
                .execute(pool)
                .await;
            migrated += 1;
        }
    }

    let bundle_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, private_key FROM bundle_signing_key
         WHERE id = 1 AND private_key != ?",
    )
    .bind(KEYCHAIN_MARKER)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    for (id, value) in bundle_rows {
        if set_secret("bundle:signing-seed".to_string(), value).await {
            let _ = sqlx::query("UPDATE bundle_signing_key SET private_key = ? WHERE id = ?")
                .bind(KEYCHAIN_MARKER)
                .bind(&id)
                .execute(pool)
                .await;
            migrated += 1;
        }
    }

    let app_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT key, value FROM app_settings
         WHERE key IN ('attention_email_password', 'agent_email_password',
                       'memory_embedding_api_key',
                       'telegram_token', 'slack_app_token', 'slack_bot_token',
                       'discord_token', 'attention_webhook')
           AND value != '' AND value != ?",
    )
    .bind(KEYCHAIN_MARKER)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    for (key, value) in app_rows {
        let account = match key.as_str() {
            "attention_email_password" => "attention-email-password",
            "agent_email_password" => "agent-email-password",
            "memory_embedding_api_key" => "memory-embedding-key",
            "telegram_token" => "telegram:bot",
            "slack_app_token" => "slack:app",
            "slack_bot_token" => "slack:bot",
            "discord_token" => "discord:bot",
            "attention_webhook" => "attention-webhook",
            _ => continue,
        };
        if set_secret(account.to_string(), value).await {
            let _ = sqlx::query(
                "UPDATE app_settings SET value = ?, updated_at = datetime('now') WHERE key = ?",
            )
            .bind(KEYCHAIN_MARKER)
            .bind(&key)
            .execute(pool)
            .await;
            migrated += 1;
        }
    }

    Ok(migrated)
}

/// Store a provider key in the OS keychain or the encrypted local vault.
/// Returns false only when both are unavailable/writable.
pub async fn set_provider_key(provider: &str, value: &str) -> bool {
    set_secret(format!("provider:{provider}"), value.to_string()).await
}

/// MCP credentials are per server and per key; the account name keeps both.
pub async fn set_mcp_env(server_id: &str, key: &str, value: &str) -> bool {
    set_secret(format!("mcp:{server_id}:{key}"), value.to_string()).await
}

pub async fn get_mcp_env(server_id: &str, key: &str) -> Option<String> {
    get_secret(format!("mcp:{server_id}:{key}")).await
}

pub async fn delete_mcp_env(server_id: &str, key: &str) {
    delete_secret(format!("mcp:{server_id}:{key}")).await;
}

pub async fn set_plugin_credential(plugin_id: &str, value: &str) -> bool {
    set_secret(format!("plugin:{plugin_id}"), value.to_string()).await
}

pub async fn get_plugin_credential(plugin_id: &str) -> Option<String> {
    get_secret(format!("plugin:{plugin_id}")).await
}

pub async fn delete_plugin_credential(plugin_id: &str) {
    delete_secret(format!("plugin:{plugin_id}")).await;
}

pub async fn set_memory_embedding_key(value: &str) -> bool {
    set_secret("memory-embedding-key".to_string(), value.to_string()).await
}

pub async fn get_memory_embedding_key() -> Option<String> {
    get_secret("memory-embedding-key".to_string()).await
}

pub async fn delete_memory_embedding_key() {
    delete_secret("memory-embedding-key".to_string()).await;
}

pub async fn set_attention_email_password(value: &str) -> bool {
    set_secret("attention-email-password".to_string(), value.to_string()).await
}

pub async fn get_attention_email_password() -> Option<String> {
    get_secret("attention-email-password".to_string()).await
}

pub async fn delete_attention_email_password() {
    delete_secret("attention-email-password".to_string()).await;
}

/// The agent's own SMTP password. Separate from the fixed alert account so
/// the alert recipient never becomes the agent's identity.
pub async fn set_agent_email_password(value: &str) -> bool {
    set_secret("agent-email-password".to_string(), value.to_string()).await
}

pub async fn get_agent_email_password() -> Option<String> {
    get_secret("agent-email-password".to_string()).await
}

pub async fn delete_agent_email_password() {
    delete_secret("agent-email-password".to_string()).await;
}

pub async fn set_attention_webhook(value: &str) -> bool {
    set_secret("attention-webhook".to_string(), value.to_string()).await
}

pub async fn get_attention_webhook() -> Option<String> {
    get_secret("attention-webhook".to_string()).await
}

pub async fn delete_attention_webhook() {
    delete_secret("attention-webhook".to_string()).await;
}

pub async fn set_vault_secret(item_id: &str, value: &str) -> bool {
    set_secret(format!("vault:{item_id}"), value.to_string()).await
}

pub async fn get_vault_secret(item_id: &str) -> Option<String> {
    get_secret(format!("vault:{item_id}")).await
}

pub async fn delete_vault_secret(item_id: &str) {
    delete_secret(format!("vault:{item_id}")).await;
}

pub async fn set_webhook_secret(routine_id: &str, value: &str) -> bool {
    set_secret(format!("webhook:{routine_id}"), value.to_string()).await
}

pub async fn get_webhook_secret(routine_id: &str) -> Option<String> {
    get_secret(format!("webhook:{routine_id}")).await
}

pub async fn delete_webhook_secret(routine_id: &str) {
    delete_secret(format!("webhook:{routine_id}")).await;
}

pub async fn set_mcp_headers(server_id: &str, value: &str) -> bool {
    set_secret(format!("mcp-server:{server_id}:headers"), value.to_string()).await
}

pub async fn get_mcp_headers(server_id: &str) -> Option<String> {
    get_secret(format!("mcp-server:{server_id}:headers")).await
}

pub async fn delete_mcp_headers(server_id: &str) {
    delete_secret(format!("mcp-server:{server_id}:headers")).await;
}

/// HTTP headers for the enterprise OTLP export collector, as a JSON object.
///
/// Header values are credentials (a bearer token, an API key), so they follow
/// the same keychain-or-sealed-vault path as provider keys. The database only
/// keeps a marker that the entry exists.
pub async fn set_otel_export_headers(value: &str) -> bool {
    set_secret("otel-export:headers".to_string(), value.to_string()).await
}

pub async fn get_otel_export_headers() -> Option<String> {
    get_secret("otel-export:headers".to_string()).await
}

pub async fn delete_otel_export_headers() {
    delete_secret("otel-export:headers".to_string()).await;
}

pub async fn set_custom_provider_headers(provider_id: &str, value: &str) -> bool {
    set_secret(
        format!("custom-provider:{provider_id}:headers"),
        value.to_string(),
    )
    .await
}

pub async fn get_custom_provider_headers(provider_id: &str) -> Option<String> {
    get_secret(format!("custom-provider:{provider_id}:headers")).await
}

pub async fn delete_custom_provider_headers(provider_id: &str) {
    delete_secret(format!("custom-provider:{provider_id}:headers")).await;
}

pub async fn set_signing_seed(value: &str) -> bool {
    set_secret("bundle:signing-seed".to_string(), value.to_string()).await
}

pub async fn get_signing_seed() -> Option<String> {
    get_secret("bundle:signing-seed".to_string()).await
}

/// Read a provider key from the OS keychain.
pub async fn get_provider_key(provider: &str) -> Option<String> {
    get_secret(format!("provider:{provider}")).await
}

/// Delete a provider key from the OS keychain. A missing entry is success.
pub async fn delete_provider_key(provider: &str) {
    delete_secret(format!("provider:{provider}")).await;
}

pub async fn set_telegram_token(value: &str) -> bool {
    set_secret("telegram:bot".to_string(), value.to_string()).await
}

pub async fn get_telegram_token() -> Option<String> {
    get_secret("telegram:bot".to_string()).await
}

pub async fn delete_telegram_token() {
    delete_secret("telegram:bot".to_string()).await;
}

pub async fn set_slack_app_token(value: &str) -> bool {
    set_secret("slack:app".to_string(), value.to_string()).await
}

pub async fn get_slack_app_token() -> Option<String> {
    get_secret("slack:app".to_string()).await
}

pub async fn delete_slack_app_token() {
    delete_secret("slack:app".to_string()).await;
}

pub async fn set_slack_bot_token(value: &str) -> bool {
    set_secret("slack:bot".to_string(), value.to_string()).await
}

pub async fn get_slack_bot_token() -> Option<String> {
    get_secret("slack:bot".to_string()).await
}

pub async fn delete_slack_bot_token() {
    delete_secret("slack:bot".to_string()).await;
}

pub async fn set_discord_token(value: &str) -> bool {
    set_secret("discord:bot".to_string(), value.to_string()).await
}

pub async fn get_discord_token() -> Option<String> {
    get_secret("discord:bot".to_string()).await
}

pub async fn delete_discord_token() {
    delete_secret("discord:bot".to_string()).await;
}

pub async fn set_whatsapp_token(value: &str) -> bool {
    set_secret("whatsapp:token".to_string(), value.to_string()).await
}

pub async fn get_whatsapp_token() -> Option<String> {
    get_secret("whatsapp:token".to_string()).await
}

pub async fn delete_whatsapp_token() {
    delete_secret("whatsapp:token".to_string()).await;
}

pub async fn set_whatsapp_app_secret(value: &str) -> bool {
    set_secret("whatsapp:app_secret".to_string(), value.to_string()).await
}

pub async fn get_whatsapp_app_secret() -> Option<String> {
    get_secret("whatsapp:app_secret".to_string()).await
}

pub async fn delete_whatsapp_app_secret() {
    delete_secret("whatsapp:app_secret".to_string()).await;
}

pub async fn set_whatsapp_verify_token(value: &str) -> bool {
    set_secret("whatsapp:verify_token".to_string(), value.to_string()).await
}

pub async fn get_whatsapp_verify_token() -> Option<String> {
    get_secret("whatsapp:verify_token".to_string()).await
}

pub async fn delete_whatsapp_verify_token() {
    delete_secret("whatsapp:verify_token".to_string()).await;
}

pub async fn set_teams_app_password(value: &str) -> bool {
    set_secret("teams:app_password".to_string(), value.to_string()).await
}

pub async fn get_teams_app_password() -> Option<String> {
    get_secret("teams:app_password".to_string()).await
}

pub async fn delete_teams_app_password() {
    delete_secret("teams:app_password".to_string()).await;
}

#[cfg(test)]
mod account_namespace_tests {
    use super::namespaced;

    #[test]
    fn owner_keys_keep_the_legacy_name_and_accounts_are_prefixed() {
        assert_eq!(namespaced("provider:openai", ""), "provider:openai");
        assert_eq!(
            namespaced("provider:openai", "account-1"),
            "account-1:provider:openai"
        );
    }
}

#[cfg(test)]
mod sealed_vault_tests {
    use super::SealedVault;
    use uuid::Uuid;

    fn vault() -> (SealedVault, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!("ravenbot-vault-{}", Uuid::new_v4()));
        (SealedVault::new(root.clone()), root)
    }

    #[test]
    fn secrets_round_trip_and_delete() {
        let (vault, root) = vault();
        vault.put("provider:openai", "sk-test-secret").expect("put");
        assert_eq!(
            vault.get("provider:openai").as_deref(),
            Some("sk-test-secret")
        );
        vault.put("provider:groq", "gsk-other").expect("put other");
        assert_eq!(vault.get("provider:groq").as_deref(), Some("gsk-other"));
        vault.delete("provider:openai");
        assert!(vault.get("provider:openai").is_none());
        assert_eq!(vault.get("provider:groq").as_deref(), Some("gsk-other"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn the_vault_file_and_key_are_private_and_do_not_contain_plaintext() {
        let (vault, root) = vault();
        vault
            .put("plugin:example", "super-secret-token")
            .expect("put");
        let key = std::fs::read(root.join("key")).expect("key");
        assert_eq!(key.len(), 32);
        let entries: Vec<_> = std::fs::read_dir(root.join("entries"))
            .expect("entries")
            .flatten()
            .collect();
        assert_eq!(entries.len(), 1);
        let bytes = std::fs::read(entries[0].path()).expect("entry");
        assert!(
            !bytes
                .windows(b"super-secret-token".len())
                .any(|window| window == b"super-secret-token"),
            "the entry must be encrypted"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(root.join("key"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
            assert_eq!(
                std::fs::metadata(entries[0].path())
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_tampered_entry_is_unavailable_not_wrong_plaintext() {
        let (vault, root) = vault();
        vault.put("vault:item", "correct horse").expect("put");
        let entries: Vec<_> = std::fs::read_dir(root.join("entries"))
            .expect("entries")
            .flatten()
            .collect();
        let path = entries[0].path();
        let mut bytes = std::fs::read(&path).expect("read");
        let last = bytes.len() - 1;
        bytes[last] ^= 0x01;
        std::fs::write(&path, bytes).expect("tamper");
        assert!(vault.get("vault:item").is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_new_vault_instance_reuses_the_key() {
        let (vault, root) = vault();
        vault.put("mcp:github:token", "ghp-example").expect("put");
        let reopened = SealedVault::new(root.clone());
        assert_eq!(
            reopened.get("mcp:github:token").as_deref(),
            Some("ghp-example")
        );
        let _ = std::fs::remove_dir_all(root);
    }
}

#[cfg(test)]
mod secret_migration_tests {
    use super::*;

    struct EnvGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            std::env::set_var(key, value);
            Self { key, previous }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }

    #[tokio::test]
    async fn legacy_plaintext_provider_keys_move_to_the_encrypted_vault() {
        let root = std::env::temp_dir().join(format!(
            "ravenbot-secret-migration-{}",
            uuid::Uuid::new_v4()
        ));
        let _vault = EnvGuard::set("RAVENBOT_SECRET_VAULT", &root.display().to_string());
        let _keychain = EnvGuard::set("RAVENBOT_DISABLE_KEYCHAIN", "1");

        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        crate::migrations::run(&pool).await.expect("migrations");
        sqlx::query("INSERT INTO provider_keys (provider, api_key) VALUES ('openai', 'sk-legacy')")
            .execute(&pool)
            .await
            .expect("provider key");
        sqlx::query(
            "INSERT INTO app_settings (key, value) VALUES ('agent_email_password', 'smtp-legacy')",
        )
        .execute(&pool)
        .await
        .expect("agent email password");

        let migrated = migrate_plaintext_at_rest(&pool).await.expect("migration");
        assert!(migrated >= 1, "the provider row must be migrated");
        let stored: (String,) =
            sqlx::query_as("SELECT api_key FROM provider_keys WHERE provider = 'openai'")
                .fetch_one(&pool)
                .await
                .expect("stored");
        assert_eq!(stored.0, KEYCHAIN_MARKER);
        assert_eq!(
            crate::queries::ProviderKeyQueries::get(&pool, "openai")
                .await
                .expect("read"),
            Some("sk-legacy".to_string())
        );

        let stored: (String,) =
            sqlx::query_as("SELECT value FROM app_settings WHERE key = 'agent_email_password'")
                .fetch_one(&pool)
                .await
                .expect("stored agent password");
        assert_eq!(stored.0, KEYCHAIN_MARKER);
        assert_eq!(
            get_agent_email_password().await,
            Some("smtp-legacy".to_string())
        );

        let _ = std::fs::remove_dir_all(root);
    }
}
