//! Local multi-account support.
//!
//! The account registry is a small SQLite database at the root of the RAVENBOT
//! tree. Each account has its own data root, its own database and its own
//! keychain namespace. Passwords are Argon2id hashes; sessions are random
//! tokens stored as SHA-256 hashes with an expiry. A successful desktop
//! sign-in is remembered for 30 days so closing the app does not sign out. The
//! desktop app starts in a launcher window until a session is established,
//! then relaunches itself against the selected account root.
//!
//! Workspaces can be shared through owner/operator/viewer grants. The Tauri IPC
//! gate in `access.rs` enforces the effective role; the UI only reflects it.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use fs2::FileExt;
use rand::RngCore;
use ravenbot_core::expand_home;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use tauri::Manager;
use tokio::sync::Mutex;
use uuid::Uuid;

const REGISTRY_FILE: &str = "accounts.sqlite";
const SESSION_TTL_SECS: i64 = 8 * 60 * 60;
/// A remembered desktop session survives app restarts for a month.
const REMEMBERED_SESSION_TTL_SECS: i64 = 30 * 24 * 60 * 60;
const SESSION_FILE: &str = "last-session.json";
pub const LOGOUT_EXIT_CODE: i32 = 42;
const FAILURE_WINDOW_SECS: i64 = 5 * 60;
const MAX_FAILURES: i64 = 5;

static FULL_MODE: AtomicBool = AtomicBool::new(false);
static DUMMY_HASH: OnceLock<String> = OnceLock::new();

#[derive(Debug, Clone, Serialize)]
pub struct Account {
    pub id: String,
    pub username: String,
    pub role: String,
    pub is_owner: bool,
    pub disabled: bool,
    pub port_base: u16,
    pub data_root: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AccountSummary {
    pub username: String,
    pub role: String,
    pub owner: bool,
    pub port_base: u16,
    pub created_at: String,
}

impl Account {
    pub fn summary(&self) -> AccountSummary {
        AccountSummary {
            username: self.username.clone(),
            role: self.role.clone(),
            owner: self.is_owner,
            port_base: self.port_base,
            created_at: self.created_at.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Session {
    pub token: String,
    pub account: Account,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedSession {
    account_id: String,
    workspace_id: String,
    token: String,
}

fn persisted_session_path(base: &Path) -> PathBuf {
    base.join("config").join(SESSION_FILE)
}

/// Remember the session that opened this workspace. The file is owner-only on
/// Unix; the token is still only a session token, never a password.
pub fn persist_session(base: &Path, session: &Session, workspace_id: &str) -> Result<(), String> {
    let dir = base.join("config");
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("Could not create the account config directory: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    }
    let payload = serde_json::to_vec(&PersistedSession {
        account_id: session.account.id.clone(),
        workspace_id: workspace_id.to_string(),
        token: session.token.clone(),
    })
    .map_err(|_| "Could not encode the remembered session.".to_string())?;
    let path = persisted_session_path(base);
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, payload)
        .map_err(|error| format!("Could not write the remembered session: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&temp, &path)
        .map_err(|error| format!("Could not store the remembered session: {error}"))
}

fn read_persisted_session(base: &Path) -> Option<PersistedSession> {
    let path = persisted_session_path(base);
    let raw = std::fs::read(path).ok()?;
    serde_json::from_slice(&raw).ok()
}

pub fn clear_persisted_session(base: &Path) {
    let _ = std::fs::remove_file(persisted_session_path(base));
}

/// Forget a session only if it is the one currently remembered.
///
/// Concurrent account windows share one remembered-session file. Logging out
/// of one window must not erase the session another window expects to find.
pub fn forget_session(base: &Path, token: &str) {
    if read_persisted_session(base)
        .as_ref()
        .is_some_and(|persisted| persisted.token == token)
    {
        clear_persisted_session(base);
    }
}

/// Extend a remembered session so app restarts stay signed in.
async fn remember_session(base: &Path, token: &str) -> Result<(), String> {
    let pool = registry(base).await?;
    let now = chrono::Utc::now().timestamp();
    sqlx::query(
        "UPDATE sessions SET expires_at = ?
         WHERE token_hash = ? AND revoked_at IS NULL AND expires_at > ?",
    )
    .bind(now + REMEMBERED_SESSION_TTL_SECS)
    .bind(hash_token(token))
    .bind(now)
    .execute(&pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceChoice {
    pub id: String,
    pub username: String,
    pub role: String,
    pub owner: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CurrentAccount {
    pub username: String,
    pub workspace: String,
    pub role: String,
    pub workspace_owner: bool,
}
#[derive(Debug, Clone)]
pub struct FullAccount {
    pub actor: Account,
    pub workspace: Account,
    pub role: String,
    pub base: PathBuf,
    pub session_token: String,
}

#[derive(Debug)]
pub enum Startup {
    Launcher,
    Full(Box<FullAccount>),
}

#[derive(Debug, Serialize)]
pub struct LoginResult {
    pub login_id: String,
    pub workspaces: Vec<WorkspaceChoice>,
}

#[derive(Debug, Serialize)]
pub struct LauncherStatus {
    pub accounts: Vec<AccountSummary>,
    pub can_create_owner: bool,
}

/// The registry lives beside the default database, at the root of the
/// RAVENBOT tree. The child app receives the base root explicitly so it never
/// tries to find the registry inside its own account root.
pub fn base_root() -> PathBuf {
    std::env::var("RAVENBOT_BASE_HOME")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| expand_home(&value))
        .unwrap_or_else(ravenbot_core::raven_root)
}

pub fn is_full_mode() -> bool {
    FULL_MODE.load(Ordering::SeqCst)
}

pub fn mode_string() -> String {
    if is_full_mode() {
        "app".to_string()
    } else {
        "launcher".to_string()
    }
}

#[tauri::command]
pub fn launcher_mode() -> String {
    "launcher".to_string()
}

fn registry_path(base: &Path) -> PathBuf {
    base.join(REGISTRY_FILE)
}

#[cfg(unix)]
fn secure_dir(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700));
}

#[cfg(not(unix))]
fn secure_dir(_path: &Path) {}

#[cfg(unix)]
fn secure_file(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn secure_file(_path: &Path) {}

async fn registry(base: &Path) -> Result<SqlitePool, String> {
    tokio::fs::create_dir_all(base)
        .await
        .map_err(|error| format!("Could not create the account directory: {error}"))?;
    secure_dir(base);
    let path = registry_path(base);
    let options = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(options)
        .await
        .map_err(|error| format!("Could not open the account registry: {error}"))?;
    secure_file(&path);
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS accounts (
            id TEXT PRIMARY KEY NOT NULL,
            username TEXT NOT NULL UNIQUE COLLATE NOCASE,
            password_hash TEXT NOT NULL,
            role TEXT NOT NULL,
            data_root TEXT NOT NULL,
            is_owner INTEGER NOT NULL DEFAULT 0,
            disabled INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL
        )",
    )
    .execute(&pool)
    .await
    .map_err(|error| error.to_string())?;
    let columns = sqlx::query("PRAGMA table_info(accounts)")
        .fetch_all(&pool)
        .await
        .map_err(|error| error.to_string())?;
    let has_port_base = columns
        .iter()
        .any(|row| row.get::<String, _>("name") == "port_base");
    if !has_port_base {
        sqlx::query("ALTER TABLE accounts ADD COLUMN port_base INTEGER NOT NULL DEFAULT 0")
            .execute(&pool)
            .await
            .map_err(|error| error.to_string())?;
    }
    // Accounts created before per-workspace ports all defaulted to 0. Give
    // every non-owner a distinct block exactly once.
    let unassigned: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM accounts WHERE is_owner = 0 AND port_base = 0 ORDER BY created_at ASC",
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| error.to_string())?;
    for (index, id) in unassigned.iter().enumerate() {
        let port = 8_810i64 + (index as i64 * 10);
        sqlx::query("UPDATE accounts SET port_base = ? WHERE id = ?")
            .bind(port)
            .bind(id)
            .execute(&pool)
            .await
            .map_err(|error| error.to_string())?;
    }
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS sessions (
            token_hash TEXT PRIMARY KEY NOT NULL,
            account_id TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            expires_at INTEGER NOT NULL,
            revoked_at INTEGER
        )",
    )
    .execute(&pool)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS login_failures (
            username TEXT NOT NULL,
            attempted_at INTEGER NOT NULL
        )",
    )
    .execute(&pool)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS workspace_grants (
            workspace_id TEXT NOT NULL,
            grantee_id TEXT NOT NULL,
            role TEXT NOT NULL,
            granted_by TEXT NOT NULL,
            created_at TEXT NOT NULL,
            PRIMARY KEY (workspace_id, grantee_id)
        )",
    )
    .execute(&pool)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_sessions_account ON sessions(account_id)")
        .execute(&pool)
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_login_failures_username ON login_failures(username)",
    )
    .execute(&pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(pool)
}

fn validate_username(raw: &str) -> Result<String, String> {
    let username = raw.trim();
    if username.len() < 3 || username.len() > 64 {
        return Err("A username is between 3 and 64 characters.".to_string());
    }
    let mut chars = username.chars();
    let Some(first) = chars.next() else {
        return Err("Enter a username.".to_string());
    };
    if !first.is_ascii_alphanumeric()
        || !chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
    {
        return Err(
            "A username starts with a letter or number and uses letters, numbers, '.', '_' or '-'."
                .to_string(),
        );
    }
    Ok(username.to_string())
}

fn validate_password(raw: &str) -> Result<String, String> {
    if raw.len() < 10 || raw.len() > 1_024 {
        return Err("A password is between 10 and 1024 characters.".to_string());
    }
    if raw.chars().any(|ch| ch.is_control()) {
        return Err("The password contains an invalid character.".to_string());
    }
    Ok(raw.to_string())
}

fn hash_password(password: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|error| format!("Could not hash the password: {error}"))
}

fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash).ok().is_some_and(|parsed| {
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    })
}

fn dummy_hash() -> &'static str {
    DUMMY_HASH
        .get_or_init(|| hash_password("ravenbot-dummy-password").unwrap_or_else(|_| String::new()))
}

fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn new_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn row_to_account(row: &sqlx::sqlite::SqliteRow) -> Account {
    Account {
        id: row.get("id"),
        username: row.get("username"),
        role: row.get("role"),
        is_owner: row.get::<i64, _>("is_owner") == 1,
        disabled: row.get::<i64, _>("disabled") == 1,
        port_base: row.get::<i64, _>("port_base").max(0) as u16,
        data_root: row.get("data_root"),
        created_at: row.get("created_at"),
    }
}

pub async fn list_accounts(base: &Path) -> Result<Vec<Account>, String> {
    let pool = registry(base).await?;
    let rows = sqlx::query(
        "SELECT id, username, role, data_root, is_owner, disabled, port_base, created_at
         FROM accounts ORDER BY created_at ASC",
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(rows.iter().map(row_to_account).collect())
}

pub async fn account_by_username(base: &Path, username: &str) -> Result<Option<Account>, String> {
    let pool = registry(base).await?;
    let row = sqlx::query(
        "SELECT id, username, role, data_root, is_owner, disabled, port_base, created_at
         FROM accounts WHERE username = ? COLLATE NOCASE",
    )
    .bind(username.trim())
    .fetch_optional(&pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(row.as_ref().map(row_to_account))
}

/// Create an account. The first account is the owner and keeps the existing
/// RAVENBOT root, so an installation upgrading from single-user keeps all of
/// its data. Later accounts get an isolated root below `accounts/<id>`.
pub async fn create_account(
    base: &Path,
    username: &str,
    password: &str,
) -> Result<Account, String> {
    let username = validate_username(username)?;
    let password = validate_password(password)?;
    let pool = registry(base).await?;
    if sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM accounts WHERE username = ? COLLATE NOCASE",
    )
    .bind(&username)
    .fetch_one(&pool)
    .await
    .map_err(|error| error.to_string())?
        > 0
    {
        return Err("That username is already taken.".to_string());
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM accounts")
        .fetch_one(&pool)
        .await
        .map_err(|error| error.to_string())?;
    let is_owner = count == 0;
    let id = Uuid::new_v4().to_string();
    let port_base = if is_owner {
        0u16
    } else {
        let max_port: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(port_base), 0) FROM accounts")
            .fetch_one(&pool)
            .await
            .map_err(|error| error.to_string())?;
        if max_port == 0 {
            8_810
        } else {
            (max_port + 10).min(u16::MAX as i64) as u16
        }
    };
    let data_root = if is_owner {
        base.to_path_buf()
    } else {
        base.join("accounts").join(&id)
    };
    tokio::fs::create_dir_all(&data_root)
        .await
        .map_err(|error| format!("Could not create the account workspace: {error}"))?;
    secure_dir(&data_root);
    if !is_owner {
        if let Some(parent) = data_root.parent() {
            secure_dir(parent);
        }
    }
    let password_hash = hash_password(&password)?;
    sqlx::query(
        "INSERT INTO accounts
           (id, username, password_hash, role, data_root, is_owner, port_base, created_at)
         VALUES (?, ?, ?, 'owner', ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&username)
    .bind(&password_hash)
    .bind(data_root.to_string_lossy().to_string())
    .bind(if is_owner { 1 } else { 0 })
    .bind(port_base as i64)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(&pool)
    .await
    .map_err(|error| {
        if error.to_string().to_ascii_lowercase().contains("unique") {
            "That username is already taken.".to_string()
        } else {
            error.to_string()
        }
    })?;
    Ok(Account {
        id,
        username,
        role: "owner".to_string(),
        is_owner,
        disabled: false,
        port_base,
        data_root: data_root.to_string_lossy().to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
    })
}

pub async fn authenticate(base: &Path, username: &str, password: &str) -> Result<Session, String> {
    let username = username.trim();
    if username.is_empty() || password.is_empty() {
        return Err("Invalid username or password.".to_string());
    }
    let pool = registry(base).await?;
    let now = chrono::Utc::now().timestamp();
    sqlx::query("DELETE FROM login_failures WHERE attempted_at < ?")
        .bind(now - FAILURE_WINDOW_SECS)
        .execute(&pool)
        .await
        .map_err(|error| error.to_string())?;
    let failures: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM login_failures WHERE username = ? COLLATE NOCASE")
            .bind(username)
            .fetch_one(&pool)
            .await
            .map_err(|error| error.to_string())?;
    if failures >= MAX_FAILURES {
        return Err("Too many sign-in attempts. Try again in a few minutes.".to_string());
    }
    let row = sqlx::query(
        "SELECT id, username, password_hash, role, data_root, is_owner, disabled, port_base, created_at
         FROM accounts WHERE username = ? COLLATE NOCASE",
    )
    .bind(username)
    .fetch_optional(&pool)
    .await
    .map_err(|error| error.to_string())?;
    let Some(row) = row else {
        let _ = verify_password(password, dummy_hash());
        record_failure(&pool, username, now).await?;
        return Err("Invalid username or password.".to_string());
    };
    let account = row_to_account(&row);
    let stored_hash: String = row.get("password_hash");
    if !verify_password(password, &stored_hash) {
        record_failure(&pool, username, now).await?;
        return Err("Invalid username or password.".to_string());
    }
    if account.disabled {
        return Err("This account is disabled.".to_string());
    }
    sqlx::query("DELETE FROM login_failures WHERE username = ? COLLATE NOCASE")
        .bind(username)
        .execute(&pool)
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query("DELETE FROM sessions WHERE expires_at < ?")
        .bind(now)
        .execute(&pool)
        .await
        .map_err(|error| error.to_string())?;
    let token = new_token();
    sqlx::query(
        "INSERT INTO sessions (token_hash, account_id, created_at, expires_at)
         VALUES (?, ?, ?, ?)",
    )
    .bind(hash_token(&token))
    .bind(&account.id)
    .bind(now)
    .bind(now + SESSION_TTL_SECS)
    .execute(&pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(Session { token, account })
}

async fn record_failure(pool: &SqlitePool, username: &str, now: i64) -> Result<(), String> {
    sqlx::query("INSERT INTO login_failures (username, attempted_at) VALUES (?, ?)")
        .bind(username.to_ascii_lowercase())
        .bind(now)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub async fn validate_session(base: &Path, token: &str) -> Result<Option<Account>, String> {
    if token.trim().is_empty() {
        return Ok(None);
    }
    let pool = registry(base).await?;
    let row = sqlx::query(
        "SELECT a.id, a.username, a.role, a.data_root, a.is_owner, a.disabled, a.port_base, a.created_at
         FROM sessions s JOIN accounts a ON a.id = s.account_id
         WHERE s.token_hash = ? AND s.revoked_at IS NULL
           AND s.expires_at > ? AND a.disabled = 0",
    )
    .bind(hash_token(token))
    .bind(chrono::Utc::now().timestamp())
    .fetch_optional(&pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(row.as_ref().map(row_to_account))
}

pub async fn revoke_session(base: &Path, token: &str) -> Result<(), String> {
    let pool = registry(base).await?;
    sqlx::query("UPDATE sessions SET revoked_at = ? WHERE token_hash = ? AND revoked_at IS NULL")
        .bind(chrono::Utc::now().timestamp())
        .bind(hash_token(token))
        .execute(&pool)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub async fn revoke_account_sessions(base: &Path, account_id: &str) -> Result<(), String> {
    let pool = registry(base).await?;
    sqlx::query("UPDATE sessions SET revoked_at = ? WHERE account_id = ? AND revoked_at IS NULL")
        .bind(chrono::Utc::now().timestamp())
        .bind(account_id)
        .execute(&pool)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub async fn rename_account(
    base: &Path,
    username: &str,
    new_username: &str,
) -> Result<Account, String> {
    let new_username = validate_username(new_username)?;
    let pool = registry(base).await?;
    let account = account_by_username(base, username)
        .await?
        .ok_or_else(|| "That account does not exist.".to_string())?;
    let taken: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM accounts WHERE username = ? COLLATE NOCASE AND id <> ?",
    )
    .bind(&new_username)
    .bind(&account.id)
    .fetch_one(&pool)
    .await
    .map_err(|error| error.to_string())?;
    if taken > 0 {
        return Err("That username is already taken.".to_string());
    }
    sqlx::query("UPDATE accounts SET username = ? WHERE id = ?")
        .bind(&new_username)
        .bind(&account.id)
        .execute(&pool)
        .await
        .map_err(|error| error.to_string())?;
    revoke_account_sessions(base, &account.id).await?;
    let mut updated = account;
    updated.username = new_username;
    Ok(updated)
}

pub async fn delete_account(base: &Path, username: &str, purge: bool) -> Result<Account, String> {
    let pool = registry(base).await?;
    let account = account_by_username(base, username)
        .await?
        .ok_or_else(|| "That account does not exist.".to_string())?;
    if account.is_owner {
        return Err("The owner account cannot be deleted.".to_string());
    }
    sqlx::query("DELETE FROM sessions WHERE account_id = ?")
        .bind(&account.id)
        .execute(&pool)
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query("DELETE FROM workspace_grants WHERE workspace_id = ? OR grantee_id = ?")
        .bind(&account.id)
        .bind(&account.id)
        .execute(&pool)
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query("DELETE FROM accounts WHERE id = ?")
        .bind(&account.id)
        .execute(&pool)
        .await
        .map_err(|error| error.to_string())?;
    if purge {
        let root = PathBuf::from(&account.data_root);
        let accounts_root = base.join("accounts");
        if root.starts_with(&accounts_root) && root != accounts_root {
            std::fs::remove_dir_all(&root).map_err(|error| {
                format!("The account was removed but its workspace could not be deleted: {error}")
            })?;
        }
    }
    Ok(account)
}

pub async fn set_password(base: &Path, username: &str, password: &str) -> Result<(), String> {
    let password = validate_password(password)?;
    let pool = registry(base).await?;
    let account = account_by_username(base, username)
        .await?
        .ok_or_else(|| "That account does not exist.".to_string())?;
    let hash = hash_password(&password)?;
    sqlx::query("UPDATE accounts SET password_hash = ? WHERE id = ?")
        .bind(&hash)
        .bind(&account.id)
        .execute(&pool)
        .await
        .map_err(|error| error.to_string())?;
    revoke_account_sessions(base, &account.id).await?;
    Ok(())
}

pub async fn set_disabled(base: &Path, username: &str, disabled: bool) -> Result<(), String> {
    let pool = registry(base).await?;
    let account = account_by_username(base, username)
        .await?
        .ok_or_else(|| "That account does not exist.".to_string())?;
    if account.is_owner && disabled {
        let owners: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM accounts WHERE is_owner = 1 AND disabled = 0")
                .fetch_one(&pool)
                .await
                .map_err(|error| error.to_string())?;
        if owners <= 1 {
            return Err("The owner account cannot be disabled.".to_string());
        }
    }
    sqlx::query("UPDATE accounts SET disabled = ? WHERE id = ?")
        .bind(if disabled { 1 } else { 0 })
        .bind(&account.id)
        .execute(&pool)
        .await
        .map_err(|error| error.to_string())?;
    if disabled {
        revoke_account_sessions(base, &account.id).await?;
    }
    Ok(())
}

pub async fn account_by_id(base: &Path, id: &str) -> Result<Option<Account>, String> {
    let pool = registry(base).await?;
    let row = sqlx::query(
        "SELECT id, username, role, data_root, is_owner, disabled, port_base, created_at
         FROM accounts WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(row.as_ref().map(row_to_account))
}

fn valid_role(role: &str) -> bool {
    matches!(role, "owner" | "operator" | "viewer")
}

pub async fn grant_workspace(
    base: &Path,
    workspace_username: &str,
    grantee_username: &str,
    role: &str,
) -> Result<(), String> {
    let role = role.trim().to_ascii_lowercase();
    if !valid_role(&role) {
        return Err("The role must be owner, operator or viewer.".to_string());
    }
    let workspace = account_by_username(base, workspace_username)
        .await?
        .ok_or_else(|| "That workspace owner does not exist.".to_string())?;
    let grantee = account_by_username(base, grantee_username)
        .await?
        .ok_or_else(|| "That account does not exist.".to_string())?;
    if workspace.id == grantee.id {
        return Err("An account already owns its own workspace.".to_string());
    }
    if workspace.disabled {
        return Err("That workspace is disabled.".to_string());
    }
    let pool = registry(base).await?;
    sqlx::query(
        "INSERT INTO workspace_grants
           (workspace_id, grantee_id, role, granted_by, created_at)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(workspace_id, grantee_id) DO UPDATE SET
           role = excluded.role,
           granted_by = excluded.granted_by,
           created_at = excluded.created_at",
    )
    .bind(&workspace.id)
    .bind(&grantee.id)
    .bind(&role)
    .bind(&workspace.id)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(&pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub async fn revoke_workspace(
    base: &Path,
    workspace_username: &str,
    grantee_username: &str,
) -> Result<bool, String> {
    let Some(workspace) = account_by_username(base, workspace_username).await? else {
        return Err("That workspace owner does not exist.".to_string());
    };
    let Some(grantee) = account_by_username(base, grantee_username).await? else {
        return Err("That account does not exist.".to_string());
    };
    let pool = registry(base).await?;
    let result =
        sqlx::query("DELETE FROM workspace_grants WHERE workspace_id = ? AND grantee_id = ?")
            .bind(&workspace.id)
            .bind(&grantee.id)
            .execute(&pool)
            .await
            .map_err(|error| error.to_string())?;
    Ok(result.rows_affected() == 1)
}

pub async fn list_workspaces_for_account(
    base: &Path,
    actor_id: &str,
) -> Result<Vec<WorkspaceChoice>, String> {
    let pool = registry(base).await?;
    let actor = account_by_id(base, actor_id)
        .await?
        .ok_or_else(|| "That account does not exist.".to_string())?;
    let mut choices = vec![WorkspaceChoice {
        id: actor.id.clone(),
        username: actor.username.clone(),
        role: "owner".to_string(),
        owner: true,
    }];
    let rows = sqlx::query(
        "SELECT a.id, a.username, g.role
         FROM workspace_grants g JOIN accounts a ON a.id = g.workspace_id
         WHERE g.grantee_id = ? AND a.disabled = 0
         ORDER BY a.created_at ASC",
    )
    .bind(actor_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| error.to_string())?;
    for row in rows {
        choices.push(WorkspaceChoice {
            id: row.get("id"),
            username: row.get("username"),
            role: row.get("role"),
            owner: false,
        });
    }
    Ok(choices)
}

pub async fn workspace_for_actor(
    base: &Path,
    actor_id: &str,
    workspace_id: &str,
) -> Result<Option<(Account, String)>, String> {
    // A disabled account cannot act, even if a grant still names it. Sessions
    // are already filtered by `validate_session`, but this resolver must not
    // depend on the caller having validated one first: a stale grant row is
    // not a capability.
    let Some(actor) = account_by_id(base, actor_id).await? else {
        return Ok(None);
    };
    if actor.disabled {
        return Ok(None);
    }
    let Some(workspace) = account_by_id(base, workspace_id).await? else {
        return Ok(None);
    };
    if workspace.disabled {
        return Ok(None);
    }
    if workspace.id == actor_id {
        return Ok(Some((workspace, "owner".to_string())));
    }
    let pool = registry(base).await?;
    let role: Option<(String,)> = sqlx::query_as(
        "SELECT role FROM workspace_grants WHERE workspace_id = ? AND grantee_id = ?",
    )
    .bind(workspace_id)
    .bind(actor_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(role.and_then(|(role,)| valid_role(&role).then_some((workspace, role))))
}

/// Backs remote sign-in with the local account registry.
///
/// The remote surface never receives a desktop session token: a successful
/// password check is immediately exchanged for the workspace grant and the
/// unused session is revoked.
pub struct RemoteAccountAuthenticator {
    base: PathBuf,
    workspace_id: String,
    workspace_username: String,
}

impl RemoteAccountAuthenticator {
    pub fn new(base: PathBuf, workspace_id: String, workspace_username: String) -> Self {
        Self {
            base,
            workspace_id,
            workspace_username,
        }
    }

    async fn workspace_account(
        &self,
        account: Account,
    ) -> Result<Option<ravenbot_remote::RemoteAccount>, String> {
        tracing::debug!(
            workspace = %self.workspace_username,
            account = %account.username,
            "Resolving a remote account workspace grant"
        );
        if account.disabled {
            return Ok(None);
        }
        let Some((_workspace, role)) =
            workspace_for_actor(&self.base, &account.id, &self.workspace_id).await?
        else {
            return Ok(None);
        };
        Ok(Some(ravenbot_remote::RemoteAccount {
            id: account.id,
            username: account.username,
            role,
        }))
    }
}

/// Credential failures, a disabled account and the registry's own sign-in
/// throttle all mean "not authorized" to the remote surface. A database or I/O
/// error stays an error so the remote layer can fail closed with a 500 rather
/// than pretending the password was wrong.
fn remote_sign_in_rejected(error: &str) -> bool {
    matches!(
        error,
        "Invalid username or password." | "This account is disabled."
    ) || error.starts_with("Too many sign-in attempts")
}

#[async_trait::async_trait]
impl ravenbot_remote::AccountAuthenticator for RemoteAccountAuthenticator {
    async fn authenticate(
        &self,
        username: &str,
        password: &str,
    ) -> Result<Option<ravenbot_remote::RemoteAccount>, String> {
        let session = match authenticate(&self.base, username, password).await {
            Ok(session) => session,
            Err(error) if remote_sign_in_rejected(&error) => return Ok(None),
            Err(error) => return Err(error),
        };
        if let Err(error) = revoke_session(&self.base, &session.token).await {
            tracing::warn!(%error, "Could not revoke the unused desktop session for remote sign-in");
        }
        self.workspace_account(session.account).await
    }

    async fn revalidate(
        &self,
        account_id: &str,
    ) -> Result<Option<ravenbot_remote::RemoteAccount>, String> {
        let Some(account) = account_by_id(&self.base, account_id).await? else {
            return Ok(None);
        };
        self.workspace_account(account).await
    }
}

/// A member of one workspace, as the Team panel shows them.
#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceMember {
    pub username: String,
    pub role: String,
    pub owner: bool,
    pub disabled: bool,
    pub created_at: String,
}

/// The owner plus every account holding a grant on this workspace.
pub async fn workspace_members(
    base: &Path,
    workspace_username: &str,
) -> Result<Vec<WorkspaceMember>, String> {
    let workspace = account_by_username(base, workspace_username)
        .await?
        .ok_or_else(|| "That workspace owner does not exist.".to_string())?;
    let accounts = list_accounts(base).await?;
    let by_name: HashMap<String, Account> = accounts
        .into_iter()
        .map(|account| (account.username.to_ascii_lowercase(), account))
        .collect();

    let mut members = vec![WorkspaceMember {
        username: workspace.username.clone(),
        role: "owner".to_string(),
        owner: true,
        disabled: workspace.disabled,
        created_at: workspace.created_at.clone(),
    }];
    for (username, role) in list_workspace_grants(base, workspace_username).await? {
        let Some(account) = by_name.get(&username.to_ascii_lowercase()) else {
            continue;
        };
        members.push(WorkspaceMember {
            username: account.username.clone(),
            role,
            owner: false,
            disabled: account.disabled,
            created_at: account.created_at.clone(),
        });
    }
    members.sort_by(|a, b| {
        b.owner.cmp(&a.owner).then_with(|| {
            a.username
                .to_ascii_lowercase()
                .cmp(&b.username.to_ascii_lowercase())
        })
    });
    Ok(members)
}

/// Add an existing account, or create a new one and grant it in the same step.
///
/// A new account is rolled back if the grant fails, so a half-added member
/// never leaves an account with no workspace behind. An existing account is
/// never recreated or given a new password; the password argument is ignored
/// for it.
pub async fn add_workspace_member(
    base: &Path,
    workspace_username: &str,
    username: &str,
    password: Option<&str>,
    role: &str,
) -> Result<WorkspaceMember, String> {
    let role = role.trim().to_ascii_lowercase();
    if !valid_role(&role) {
        return Err("The role must be owner, operator or viewer.".to_string());
    }
    if username
        .trim()
        .eq_ignore_ascii_case(workspace_username.trim())
    {
        return Err("The workspace owner is already a member.".to_string());
    }
    let existing = account_by_username(base, username).await?;
    let (account, created) = match existing {
        Some(account) => {
            if account.disabled {
                return Err("That account is disabled. Enable it before adding it.".to_string());
            }
            (account, false)
        }
        None => {
            let password = password
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "A password is required for a new account.".to_string())?;
            (create_account(base, username, password).await?, true)
        }
    };
    if let Err(error) = grant_workspace(base, workspace_username, &account.username, &role).await {
        if created {
            let _ = delete_account(base, &account.username, true).await;
        }
        return Err(error);
    }
    Ok(WorkspaceMember {
        username: account.username,
        role,
        owner: false,
        disabled: account.disabled,
        created_at: account.created_at,
    })
}

/// Change an existing member's role. The owner's own role is not a grant.
pub async fn set_workspace_member_role(
    base: &Path,
    workspace_username: &str,
    member_username: &str,
    role: &str,
) -> Result<(), String> {
    if member_username
        .trim()
        .eq_ignore_ascii_case(workspace_username.trim())
    {
        return Err("The workspace owner's role cannot be changed.".to_string());
    }
    grant_workspace(base, workspace_username, member_username, role).await
}

pub async fn list_workspace_grants(
    base: &Path,
    workspace_username: &str,
) -> Result<Vec<(String, String)>, String> {
    let Some(workspace) = account_by_username(base, workspace_username).await? else {
        return Err("That workspace owner does not exist.".to_string());
    };
    let pool = registry(base).await?;
    let rows = sqlx::query(
        "SELECT a.username, g.role
         FROM workspace_grants g JOIN accounts a ON a.id = g.grantee_id
         WHERE g.workspace_id = ? ORDER BY a.username ASC",
    )
    .bind(&workspace.id)
    .fetch_all(&pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(rows
        .into_iter()
        .map(|row| (row.get("username"), row.get("role")))
        .collect())
}

#[cfg(unix)]
fn lock_file(path: &Path, mode: &str) -> Result<File, String> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)
        .map_err(|error| format!("Could not open {}: {error}", path.display()))?;
    file.try_lock_exclusive()
        .map_err(|_| format!("{mode} is already running."))?;
    let _ = file.set_len(0);
    let _ = writeln!(
        file,
        "pid={} mode={mode} since={}",
        std::process::id(),
        chrono::Utc::now().to_rfc3339()
    );
    let _ = file.flush();
    Ok(file)
}

#[cfg(not(unix))]
fn lock_file(path: &Path, mode: &str) -> Result<File, String> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|error| format!("Could not open {}: {error}", path.display()))?;
    file.try_lock_exclusive()
        .map_err(|_| format!("{mode} is already running."))?;
    let _ = file.set_len(0);
    let _ = writeln!(
        file,
        "pid={} mode={mode} since={}",
        std::process::id(),
        chrono::Utc::now().to_rfc3339()
    );
    let _ = file.flush();
    Ok(file)
}

/// Validate the environment handed over by the launcher and switch the process
/// to the account's root and keychain namespace.
pub async fn detect_startup() -> Result<Startup, String> {
    let base = base_root();
    if let Some(account_id) = std::env::var("RAVENBOT_ACCOUNT_ID")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        let token = std::env::var("RAVENBOT_ACCOUNT_SESSION")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "The account session is missing. Sign in again.".to_string())?;
        let actor = validate_session(&base, &token)
            .await?
            .ok_or_else(|| "Your session expired. Sign in again.".to_string())?;
        if actor.id != account_id {
            return Err("The account session does not match this account.".to_string());
        }
        let workspace_id = std::env::var("RAVENBOT_WORKSPACE_ID")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| actor.id.clone());
        let (workspace, role) = workspace_for_actor(&base, &actor.id, &workspace_id)
            .await?
            .ok_or_else(|| "You do not have access to that workspace.".to_string())?;
        return Ok(activate_startup(base, actor, workspace, role, token));
    }

    // No launcher environment: reopen the last workspace if its remembered
    // session is still valid. Closing the app must not sign the user out;
    // explicit logout clears this file.
    if let Some(persisted) = read_persisted_session(&base) {
        match validate_session(&base, &persisted.token).await {
            Ok(Some(actor)) if actor.id == persisted.account_id => {
                match workspace_for_actor(&base, &actor.id, &persisted.workspace_id).await {
                    Ok(Some((workspace, role))) => {
                        if let Err(error) = remember_session(&base, &persisted.token).await {
                            tracing::warn!(%error, "Could not extend the remembered session");
                        }
                        return Ok(activate_startup(
                            base,
                            actor,
                            workspace,
                            role,
                            persisted.token,
                        ));
                    }
                    Ok(None) => {
                        tracing::info!("The remembered workspace is no longer accessible");
                    }
                    Err(error) => {
                        // A transient database error must not throw away a
                        // valid remembered session.
                        tracing::warn!(
                            %error,
                            "Could not check the remembered workspace; keeping the session"
                        );
                        return Ok(Startup::Launcher);
                    }
                }
            }
            Ok(_) => tracing::info!("The remembered session is no longer valid"),
            Err(error) => {
                tracing::warn!(
                    %error,
                    "Could not validate the remembered session; keeping it"
                );
                return Ok(Startup::Launcher);
            }
        }
    }
    // Remove an expired, revoked, malformed or unreadable session so the next
    // launch does not retry it.
    clear_persisted_session(&base);
    Ok(Startup::Launcher)
}

fn activate_startup(
    base: PathBuf,
    actor: Account,
    workspace: Account,
    role: String,
    token: String,
) -> Startup {
    let data_root = PathBuf::from(&workspace.data_root);
    std::env::set_var("RAVENBOT_HOME", &data_root);
    std::env::set_var("RAVENBOT_DB", data_root.join("ravenbot.db"));
    std::env::set_var("RAVENBOT_BASE_HOME", &base);
    std::env::set_var("RAVENBOT_WORKSPACE_ID", &workspace.id);
    if workspace.is_owner {
        std::env::remove_var("RAVENBOT_KEYCHAIN_NAMESPACE");
        std::env::remove_var("RAVENBOT_WEBHOOK_PORT");
        std::env::remove_var("RAVENBOT_CHAT_WEBHOOK_PORT");
        std::env::remove_var("RAVENBOT_REMOTE_PORT");
        std::env::remove_var("RAVENBOT_SERVICE_NAME");
        std::env::remove_var("RAVENBOT_SERVICE_LABEL");
    } else {
        let short = &workspace.id[..workspace.id.len().min(8)];
        std::env::set_var("RAVENBOT_KEYCHAIN_NAMESPACE", &workspace.id);
        std::env::set_var("RAVENBOT_WEBHOOK_PORT", workspace.port_base.to_string());
        std::env::set_var(
            "RAVENBOT_CHAT_WEBHOOK_PORT",
            (workspace.port_base + 1).to_string(),
        );
        std::env::set_var(
            "RAVENBOT_REMOTE_PORT",
            (workspace.port_base + 2).to_string(),
        );
        std::env::set_var("RAVENBOT_SERVICE_NAME", format!("ravenbot-{short}"));
        std::env::set_var(
            "RAVENBOT_SERVICE_LABEL",
            format!("dev.ravenbot.background.{short}"),
        );
    }
    std::env::remove_var("RAVENBOT_ACCOUNT_SESSION");
    FULL_MODE.store(true, Ordering::SeqCst);
    Startup::Full(Box::new(FullAccount {
        actor,
        workspace,
        role,
        base,
        session_token: token,
    }))
}

pub fn detect_startup_blocking() -> Result<Startup, String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    rt.block_on(detect_startup())
}

pub fn watch_session(
    app: tauri::AppHandle,
    base: PathBuf,
    token: String,
    actor_id: String,
    workspace_id: String,
) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let actor = match validate_session(&base, &token).await {
                Ok(Some(account)) => account,
                Ok(None) => {
                    app.exit(1);
                    break;
                }
                Err(error) => {
                    tracing::warn!(%error, "Account session check failed");
                    continue;
                }
            };
            if actor.id != actor_id {
                app.exit(1);
                break;
            }
            match workspace_for_actor(&base, &actor_id, &workspace_id).await {
                Ok(Some(_)) => {}
                Ok(None) => {
                    app.exit(1);
                    break;
                }
                Err(error) => tracing::warn!(%error, "Workspace access check failed"),
            }
        }
    });
}

fn spawn_account_app(
    base: &Path,
    session: &Session,
    workspace: &Account,
) -> Result<std::process::Child, String> {
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let data_root = PathBuf::from(&workspace.data_root);
    let mut command = std::process::Command::new(exe);
    command
        .env("RAVENBOT_BASE_HOME", base)
        .env("RAVENBOT_HOME", &data_root)
        .env("RAVENBOT_DB", data_root.join("ravenbot.db"))
        .env("RAVENBOT_ACCOUNT_ID", &session.account.id)
        .env("RAVENBOT_ACCOUNT_SESSION", &session.token)
        .env("RAVENBOT_WORKSPACE_ID", &workspace.id)
        .stdin(std::process::Stdio::null());
    if tauri::is_dev() {
        // The hidden launcher process keeps `tauri dev` and Vite alive while
        // the account window runs. Logout exits with a marker code so the
        // launcher can show the login screen again without restarting Vite.
        command.env("RAVENBOT_DEV_LAUNCHER", "1");
    }
    if workspace.is_owner {
        command.env_remove("RAVENBOT_KEYCHAIN_NAMESPACE");
        command.env_remove("RAVENBOT_WEBHOOK_PORT");
        command.env_remove("RAVENBOT_CHAT_WEBHOOK_PORT");
        command.env_remove("RAVENBOT_REMOTE_PORT");
        command.env_remove("RAVENBOT_SERVICE_NAME");
        command.env_remove("RAVENBOT_SERVICE_LABEL");
    } else {
        let short = &workspace.id[..workspace.id.len().min(8)];
        command.env("RAVENBOT_KEYCHAIN_NAMESPACE", &workspace.id);
        command.env("RAVENBOT_WEBHOOK_PORT", workspace.port_base.to_string());
        command.env(
            "RAVENBOT_CHAT_WEBHOOK_PORT",
            (workspace.port_base + 1).to_string(),
        );
        command.env(
            "RAVENBOT_REMOTE_PORT",
            (workspace.port_base + 2).to_string(),
        );
        command.env("RAVENBOT_SERVICE_NAME", format!("ravenbot-{short}"));
        command.env(
            "RAVENBOT_SERVICE_LABEL",
            format!("dev.ravenbot.background.{short}"),
        );
    }
    command
        .spawn()
        .map_err(|error| format!("Could not start the account window: {error}"))
}

/// Start the account window. In release the launcher exits and the account
/// process owns the session. In dev the launcher must stay alive: it is the
/// process `tauri dev` watches, so exiting would stop Vite and leave the new
/// account window on a connection-error page.
async fn start_account_from_launcher(
    app: &tauri::AppHandle,
    base: &Path,
    session: &Session,
    workspace: &Account,
) -> Result<(), String> {
    if let Err(error) = persist_session(base, session, &workspace.id) {
        tracing::warn!(%error, "Could not persist the desktop session");
    }
    let _ = remember_session(base, &session.token).await;
    let mut child = spawn_account_app(base, session, workspace)?;
    if !tauri::is_dev() {
        std::process::exit(0);
    }

    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let status = child.wait();
        if status.ok().and_then(|status| status.code()) != Some(LOGOUT_EXIT_CODE) {
            // The account window was closed normally. Stop the launcher too;
            // the remembered session reopens the workspace on the next start.
            std::process::exit(0);
        }
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.set_focus();
            let _ = window.eval("window.location.reload()");
        }
    });
    Ok(())
}

pub fn spawn_launcher(base: &Path) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    std::process::Command::new(exe)
        .env("RAVENBOT_HOME", base)
        .env("RAVENBOT_BASE_HOME", base)
        .env_remove("RAVENBOT_DB")
        .env_remove("RAVENBOT_ACCOUNT_ID")
        .env_remove("RAVENBOT_ACCOUNT_SESSION")
        .env_remove("RAVENBOT_KEYCHAIN_NAMESPACE")
        .env_remove("RAVENBOT_DEV_LAUNCHER")
        .stdin(std::process::Stdio::null())
        .spawn()
        .map_err(|error| format!("Could not open the account launcher: {error}"))?;
    Ok(())
}

#[derive(Clone)]
struct LauncherState {
    base: PathBuf,
    _lock: std::sync::Arc<File>,
    pending: std::sync::Arc<Mutex<HashMap<String, Session>>>,
}

#[tauri::command]
async fn account_launcher_status(
    state: tauri::State<'_, LauncherState>,
) -> Result<LauncherStatus, String> {
    let accounts = list_accounts(&state.base).await?;
    Ok(LauncherStatus {
        can_create_owner: accounts.is_empty(),
        accounts: accounts.iter().map(Account::summary).collect(),
    })
}

#[tauri::command]
async fn create_owner_account(
    app: tauri::AppHandle,
    state: tauri::State<'_, LauncherState>,
    username: String,
    password: String,
) -> Result<(), String> {
    if !list_accounts(&state.base).await?.is_empty() {
        return Err("An owner account already exists.".to_string());
    }
    let account = create_account(&state.base, &username, &password).await?;
    let session = authenticate(&state.base, &account.username, &password).await?;
    if let Err(error) =
        start_account_from_launcher(&app, &state.base, &session, &session.account).await
    {
        let _ = revoke_session(&state.base, &session.token).await;
        return Err(error);
    }
    Ok(())
}

#[tauri::command]
async fn login_account(
    app: tauri::AppHandle,
    state: tauri::State<'_, LauncherState>,
    username: String,
    password: String,
) -> Result<Option<LoginResult>, String> {
    let session = authenticate(&state.base, &username, &password).await?;
    let choices = list_workspaces_for_account(&state.base, &session.account.id).await?;
    if choices.len() > 1 {
        let login_id = new_token();
        state.pending.lock().await.insert(login_id.clone(), session);
        return Ok(Some(LoginResult {
            login_id,
            workspaces: choices,
        }));
    }
    let workspace = choices
        .first()
        .map(|choice| choice.id.clone())
        .unwrap_or_else(|| session.account.id.clone());
    let workspace_account = account_by_id(&state.base, &workspace)
        .await?
        .ok_or_else(|| "That workspace no longer exists.".to_string())?;
    if let Err(error) =
        start_account_from_launcher(&app, &state.base, &session, &workspace_account).await
    {
        let _ = revoke_session(&state.base, &session.token).await;
        return Err(error);
    }
    // In dev the launcher stays alive; there is no workspace picker to show.
    Ok(None)
}

#[tauri::command]
async fn open_workspace(
    app: tauri::AppHandle,
    state: tauri::State<'_, LauncherState>,
    login_id: String,
    workspace_id: String,
) -> Result<(), String> {
    let session = state
        .pending
        .lock()
        .await
        .remove(&login_id)
        .ok_or_else(|| "That sign-in expired. Sign in again.".to_string())?;
    let Some((workspace, _role)) =
        workspace_for_actor(&state.base, &session.account.id, &workspace_id).await?
    else {
        return Err("You do not have access to that workspace.".to_string());
    };
    if let Err(error) = start_account_from_launcher(&app, &state.base, &session, &workspace).await {
        let _ = revoke_session(&state.base, &session.token).await;
        return Err(error);
    }
    Ok(())
}

#[tauri::command]
async fn create_launcher_account(
    app: tauri::AppHandle,
    state: tauri::State<'_, LauncherState>,
    username: String,
    password: String,
) -> Result<(), String> {
    if list_accounts(&state.base).await?.is_empty() {
        return Err("Create the owner account first.".to_string());
    }
    let account = create_account(&state.base, &username, &password).await?;
    let session = authenticate(&state.base, &account.username, &password).await?;
    if let Err(error) = start_account_from_launcher(&app, &state.base, &session, &account).await {
        let _ = revoke_session(&state.base, &session.token).await;
        return Err(error);
    }
    Ok(())
}

/// Start the launcher window. Called before the full app setup when no account
/// session was handed over.
pub fn run_launcher() {
    let base = base_root();
    let lock = match lock_file(&base.join("launcher.lock"), "RAVENBOT account launcher") {
        Ok(file) => file,
        Err(error) => {
            crate::fleet::show_error("RAVENBOT", &error);
            return;
        }
    };
    let state = LauncherState {
        base,
        _lock: std::sync::Arc::new(lock),
        pending: std::sync::Arc::new(Mutex::new(HashMap::new())),
    };
    let result = tauri::Builder::default()
        .setup(move |app| {
            app.manage(state);
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            launcher_mode,
            account_launcher_status,
            create_owner_account,
            create_launcher_account,
            login_account,
            open_workspace
        ])
        .run(tauri::generate_context!());
    if let Err(error) = result {
        eprintln!("RAVENBOT account launcher error: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_base() -> PathBuf {
        std::env::temp_dir().join(format!("ravenbot-accounts-{}", Uuid::new_v4()))
    }

    #[tokio::test]
    async fn a_remembered_session_survives_restart_and_is_forgotten_on_logout() {
        let base = temp_base();
        let account = create_account(&base, "owner", "correct horse battery")
            .await
            .expect("owner");
        let session = authenticate(&base, "owner", "correct horse battery")
            .await
            .expect("sign in");
        persist_session(&base, &session, &account.id).expect("persist");
        let stored = read_persisted_session(&base).expect("read remembered session");
        assert_eq!(stored.account_id, account.id);
        assert_eq!(stored.workspace_id, account.id);
        assert_eq!(stored.token, session.token);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(persisted_session_path(&base))
                .expect("metadata")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        remember_session(&base, &session.token)
            .await
            .expect("remember");
        let pool = registry(&base).await.expect("registry");
        let expires: i64 =
            sqlx::query_scalar("SELECT expires_at FROM sessions WHERE token_hash = ?")
                .bind(hash_token(&session.token))
                .fetch_one(&pool)
                .await
                .expect("expiry");
        assert!(
            expires > chrono::Utc::now().timestamp() + 29 * 24 * 60 * 60,
            "remembered sessions must outlive a restart"
        );
        forget_session(&base, "a-different-session");
        assert!(
            read_persisted_session(&base).is_some(),
            "logging out of another window must not forget this session"
        );
        forget_session(&base, &session.token);
        assert!(read_persisted_session(&base).is_none());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn the_owner_uses_the_existing_root_and_can_sign_in() {
        let base = temp_base();
        let account = create_account(&base, "owner", "correct horse battery")
            .await
            .expect("owner");
        assert!(account.is_owner);
        assert_eq!(PathBuf::from(&account.data_root), base);
        let session = authenticate(&base, "owner", "correct horse battery")
            .await
            .expect("sign in");
        assert_eq!(session.token.len(), 64);
        let validated = validate_session(&base, &session.token)
            .await
            .expect("validate")
            .expect("session");
        assert_eq!(validated.username, "owner");
        revoke_session(&base, &session.token).await.expect("revoke");
        assert!(validate_session(&base, &session.token)
            .await
            .expect("validate")
            .is_none());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn a_second_account_gets_a_separate_workspace() {
        let base = temp_base();
        let owner = create_account(&base, "owner", "correct horse battery")
            .await
            .expect("owner");
        let second = create_account(&base, "second", "another good password")
            .await
            .expect("second");
        assert!(!second.is_owner);
        assert_eq!(second.port_base, 8_810);
        assert_ne!(second.data_root, owner.data_root);
        assert!(PathBuf::from(&second.data_root).starts_with(base.join("accounts")));
        let accounts = list_accounts(&base).await.expect("list");
        assert_eq!(accounts.len(), 2);
        assert!(accounts.iter().any(|account| account.username == "second"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn password_changes_and_disabling_revoke_sessions() {
        let base = temp_base();
        create_account(&base, "owner", "correct horse battery")
            .await
            .expect("owner");
        let second = create_account(&base, "second", "another good password")
            .await
            .expect("second");
        let session = authenticate(&base, "second", "another good password")
            .await
            .expect("sign in");
        set_password(&base, "second", "a brand new password")
            .await
            .expect("password");
        assert!(validate_session(&base, &session.token)
            .await
            .expect("validate")
            .is_none());
        assert!(authenticate(&base, "second", "another good password")
            .await
            .is_err());
        let new_session = authenticate(&base, "second", "a brand new password")
            .await
            .expect("new password");
        set_disabled(&base, "second", true).await.expect("disable");
        assert!(validate_session(&base, &new_session.token)
            .await
            .expect("validate")
            .is_none());
        assert!(authenticate(&base, "second", "a brand new password")
            .await
            .is_err());
        assert!(set_disabled(&base, "owner", true).await.is_err());
        let _ = second;
        let _ = std::fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn workspace_grants_resolve_roles_and_can_be_revoked() {
        let base = temp_base();
        create_account(&base, "owner", "correct horse battery")
            .await
            .expect("owner");
        let grantee = create_account(&base, "grantee", "another good password")
            .await
            .expect("grantee");
        grant_workspace(&base, "owner", "grantee", "operator")
            .await
            .expect("grant");
        let workspaces = list_workspaces_for_account(&base, &grantee.id)
            .await
            .expect("workspaces");
        assert_eq!(workspaces.len(), 2);
        assert!(workspaces
            .iter()
            .any(|workspace| workspace.username == "owner" && workspace.role == "operator"));
        assert_eq!(
            workspace_for_actor(&base, &grantee.id, &workspaces[0].id)
                .await
                .expect("resolve")
                .expect("workspace")
                .1,
            "owner"
        );
        let owner = account_by_username(&base, "owner")
            .await
            .expect("owner lookup")
            .expect("owner");
        assert_eq!(
            workspace_for_actor(&base, &grantee.id, &owner.id)
                .await
                .expect("resolve")
                .expect("shared workspace")
                .1,
            "operator"
        );
        assert!(revoke_workspace(&base, "owner", "grantee")
            .await
            .expect("revoke"));
        assert!(workspace_for_actor(&base, &grantee.id, &owner.id)
            .await
            .expect("resolve")
            .is_none());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn remote_sign_in_requires_the_workspace_grant_and_revalidates() {
        use ravenbot_remote::AccountAuthenticator as _;

        let base = temp_base();
        let owner = create_account(&base, "owner", "correct horse battery")
            .await
            .expect("owner");
        let member = create_account(&base, "member", "another good password")
            .await
            .expect("member");
        let authenticator =
            RemoteAccountAuthenticator::new(base.clone(), owner.id.clone(), owner.username.clone());

        // A valid account without a grant on this workspace is not authorized.
        assert!(authenticator
            .authenticate("member", "another good password")
            .await
            .expect("authenticate")
            .is_none());

        grant_workspace(&base, "owner", "member", "operator")
            .await
            .expect("grant");
        let signed_in = authenticator
            .authenticate("member", "another good password")
            .await
            .expect("authenticate")
            .expect("granted account");
        assert_eq!(signed_in.username, "member");
        assert_eq!(signed_in.role, "operator");

        // Wrong credentials are indistinguishable from no grant at this layer.
        assert!(authenticator
            .authenticate("member", "wrong password")
            .await
            .expect("authenticate")
            .is_none());

        assert_eq!(
            authenticator
                .revalidate(&member.id)
                .await
                .expect("revalidate")
                .expect("granted account")
                .role,
            "operator"
        );

        revoke_workspace(&base, "owner", "member")
            .await
            .expect("revoke");
        assert!(authenticator
            .revalidate(&member.id)
            .await
            .expect("revalidate")
            .is_none());

        grant_workspace(&base, "owner", "member", "viewer")
            .await
            .expect("grant again");
        set_disabled(&base, "member", true).await.expect("disable");
        assert!(authenticator
            .revalidate(&member.id)
            .await
            .expect("revalidate")
            .is_none());
        set_disabled(&base, "member", false).await.expect("enable");
        delete_account(&base, "member", false)
            .await
            .expect("delete");
        assert!(authenticator
            .revalidate(&member.id)
            .await
            .expect("revalidate")
            .is_none());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn accounts_can_be_renamed_and_deleted() {
        let base = temp_base();
        create_account(&base, "owner", "correct horse battery")
            .await
            .expect("owner");
        create_account(&base, "second", "another good password")
            .await
            .expect("second");
        let renamed = rename_account(&base, "second", "renamed")
            .await
            .expect("rename");
        assert_eq!(renamed.username, "renamed");
        assert!(account_by_username(&base, "second")
            .await
            .expect("lookup")
            .is_none());
        assert!(delete_account(&base, "owner", false).await.is_err());
        let purged = delete_account(&base, "renamed", true)
            .await
            .expect("delete");
        assert_eq!(purged.username, "renamed");
        assert!(!PathBuf::from(&purged.data_root).exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn a_new_member_is_created_granted_promoted_and_removed() {
        let base = temp_base();
        create_account(&base, "owner", "correct horse battery")
            .await
            .expect("owner");
        let member = add_workspace_member(
            &base,
            "owner",
            "alice",
            Some("alice password 123"),
            "operator",
        )
        .await
        .expect("add alice");
        assert_eq!(member.username, "alice");
        assert_eq!(member.role, "operator");
        assert!(!member.owner);
        assert!(!member.disabled);
        assert!(account_by_username(&base, "alice")
            .await
            .expect("lookup")
            .is_some());

        let members = workspace_members(&base, "owner").await.expect("members");
        assert_eq!(members.len(), 2);
        assert!(members[0].owner);
        assert_eq!(members[1].username, "alice");

        set_workspace_member_role(&base, "owner", "alice", "viewer")
            .await
            .expect("promote");
        let members = workspace_members(&base, "owner").await.expect("members");
        assert_eq!(members[1].role, "viewer");

        assert!(revoke_workspace(&base, "owner", "alice")
            .await
            .expect("revoke"));
        let members = workspace_members(&base, "owner").await.expect("members");
        assert_eq!(members.len(), 1);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn an_existing_account_is_granted_without_a_password() {
        let base = temp_base();
        create_account(&base, "owner", "correct horse battery")
            .await
            .expect("owner");
        create_account(&base, "bob", "bob password 123")
            .await
            .expect("bob");
        let member = add_workspace_member(&base, "owner", "bob", None, "viewer")
            .await
            .expect("grant bob");
        assert_eq!(member.username, "bob");
        assert_eq!(member.role, "viewer");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn the_owner_cannot_be_added_or_demoted_and_a_bad_role_is_rejected() {
        let base = temp_base();
        create_account(&base, "owner", "correct horse battery")
            .await
            .expect("owner");
        assert!(
            add_workspace_member(&base, "owner", "owner", None, "viewer")
                .await
                .is_err()
        );
        create_account(&base, "bob", "bob password 123")
            .await
            .expect("bob");
        assert!(set_workspace_member_role(&base, "owner", "owner", "viewer")
            .await
            .is_err());
        assert!(add_workspace_member(&base, "owner", "bob", None, "admin")
            .await
            .is_err());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn a_failed_grant_does_not_leave_a_new_account_behind() {
        let base = temp_base();
        create_account(&base, "owner", "correct horse battery")
            .await
            .expect("owner");
        assert!(add_workspace_member(
            &base,
            "missing",
            "ghost",
            Some("ghost password 123"),
            "viewer"
        )
        .await
        .is_err());
        assert!(account_by_username(&base, "ghost")
            .await
            .expect("lookup")
            .is_none());
        assert!(
            add_workspace_member(&base, "owner", "nopassword", None, "viewer")
                .await
                .is_err()
        );
        assert!(account_by_username(&base, "nopassword")
            .await
            .expect("lookup")
            .is_none());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn repeated_failures_are_throttled() {
        let base = temp_base();
        create_account(&base, "owner", "correct horse battery")
            .await
            .expect("owner");
        for _ in 0..MAX_FAILURES {
            assert!(authenticate(&base, "owner", "wrong password")
                .await
                .is_err());
        }
        let error = authenticate(&base, "owner", "correct horse battery")
            .await
            .expect_err("throttled");
        assert!(error.contains("Too many"), "{error}");
        let _ = std::fs::remove_dir_all(&base);
    }
}
