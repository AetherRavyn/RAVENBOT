//! Authenticated remote access for RAVENBOT.
//!
//! This is deliberately a small, local-first surface:
//!
//! * loopback by default; LAN binding is an explicit opt-in with a warning;
//! * pairing codes are one-time, short-lived and rate-limited;
//! * tokens are random, shown once, and stored only as SHA-256 hashes;
//! * a paired device must additionally sign in with a local RAVENBOT account
//!   when the host enables account sign-in; the account's workspace grant caps
//!   the paired role (`owner` → admin, `operator`, `viewer`);
//! * the account binding is revalidated at most once a minute and fails closed
//!   when the account is disabled, deleted or loses its workspace grant;
//! * roles are `viewer` (read), `operator` (send/approve/cancel) and `admin`
//!   (operator plus the audit view);
//! * device management (mint/revoke) is a local desktop action, never an HTTP
//!   endpoint, so a stolen token cannot mint more tokens;
//! * the mobile page has no third-party scripts and builds its DOM with
//!   `textContent`, not `innerHTML`.
//!
//! The API is intentionally not a general RPC bridge. It exposes conversations,
//! approvals, questions and runs; it does not expose shell, files, settings, or
//! plugin management.

use std::collections::{HashMap, VecDeque};
use std::net::{IpAddr, SocketAddr};
use std::path::{Path as FsPath, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, DefaultBodyLimit, Extension, Path, Query, Request, State};
use axum::http::{header, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use tokio_rustls::TlsAcceptor;
use uuid::Uuid;

use ravenbot_db::queries::{
    ApprovalQueries, MessageQueries, NewRemoteSsoAttempt, QuestionQueries, RemoteAccessQueries,
    RemoteSsoQueries, RunQueries, ThreadQueries, ThreadShareQueries,
};
use ravenbot_runtime::Runtime;

mod scim;
mod sso;
pub use sso::SsoConfig;

pub const DEFAULT_PORT: u16 = 8790;
const MAX_PAIRING_ATTEMPTS_PER_MINUTE: usize = 5;
const MAX_GLOBAL_PAIRING_ATTEMPTS_PER_MINUTE: usize = 30;
/// Sign-in has its own counters: a pairing flood must not lock out an existing
/// device, and failed credentials must not consume pairing attempts.
const MAX_LOGIN_ATTEMPTS_PER_MINUTE: usize = 5;
const MAX_GLOBAL_LOGIN_ATTEMPTS_PER_MINUTE: usize = 30;
/// A callback is a public GET, so it has its own counters. Thirty per minute
/// per IP leaves room for a person retrying a broken IdP, while the global cap
/// keeps a spray from filling the database or the provider's logs.
const MAX_SSO_CALLBACKS_PER_MINUTE: usize = 30;
const MAX_GLOBAL_SSO_CALLBACKS_PER_MINUTE: usize = 300;
/// SCIM is a machine-to-machine surface: a directory sync can burst, so the
/// per-IP cap is generous but not unbounded. The global cap keeps one IdP from
/// crowding out the mobile API.
const MAX_SCIM_REQUESTS_PER_MINUTE: usize = 600;
const MAX_GLOBAL_SCIM_REQUESTS_PER_MINUTE: usize = 3_000;
/// No SCIM list is allowed to materialise an unbounded result set.
pub const SCIM_MAX_RESULTS: usize = 1_000;
/// Bounds how long a revoked account can keep using a device. Revalidation is
/// cheap but goes to the account registry, so it is not done per poll.
const ACCOUNT_REVALIDATE_INTERVAL: Duration = Duration::from_secs(60);
const MAX_MESSAGE_CHARS: usize = 20_000;
const MAX_ANSWER_BYTES: usize = 64 * 1024;
const MAX_SHARE_MESSAGES: usize = 300;

const CSP: &str = "default-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'; img-src 'self' data:; style-src 'self'; script-src 'self'; connect-src 'self'";

// ─────────────────────────────────────────────────────────────────────────────
// Roles and configuration
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteRole {
    Viewer,
    Operator,
    Admin,
}

impl RemoteRole {
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "operator" => Self::Operator,
            "admin" => Self::Admin,
            _ => Self::Viewer,
        }
    }

    /// The workspace role is `owner`/`operator`/`viewer`; the remote scale is
    /// `admin`/`operator`/`viewer`. An unknown role is treated as a viewer so a
    /// future role name cannot widen access by accident.
    fn from_workspace(role: &str) -> Self {
        match role.trim().to_ascii_lowercase().as_str() {
            "owner" => Self::Admin,
            "operator" => Self::Operator,
            "viewer" => Self::Viewer,
            _ => Self::Viewer,
        }
    }

    fn rank(self) -> u8 {
        match self {
            Self::Viewer => 0,
            Self::Operator => 1,
            Self::Admin => 2,
        }
    }

    /// The account grant caps the paired role; it never raises it.
    fn capped_by(self, account_role: &str) -> Self {
        let account = Self::from_workspace(account_role);
        if account.rank() < self.rank() {
            account
        } else {
            self
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Viewer => "viewer",
            Self::Operator => "operator",
            Self::Admin => "admin",
        }
    }

    pub fn can_write(self) -> bool {
        matches!(self, Self::Operator | Self::Admin)
    }

    pub fn can_admin(self) -> bool {
        matches!(self, Self::Admin)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScimConfig {
    /// SHA-256 hex of the bearer token. The plaintext exists only in the
    /// desktop command response that generated it.
    pub token_hash: String,
    /// The only role SCIM may grant. Never `owner`.
    pub default_role: String,
}

impl ScimConfig {
    pub const DEFAULT_ROLE: &'static str = "viewer";

    /// Validate and normalize a saved configuration. Roles are lowercase and
    /// limited to `viewer`/`operator`; the hash must be a full SHA-256 hex.
    pub fn new(token_hash: &str, default_role: &str) -> Result<Self, String> {
        let default_role = default_role.trim().to_ascii_lowercase();
        let config = Self {
            token_hash: token_hash.trim().to_ascii_lowercase(),
            default_role: if default_role.is_empty() {
                Self::DEFAULT_ROLE.to_string()
            } else {
                default_role
            },
        };
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.token_hash.len() != 64 || !self.token_hash.chars().all(|ch| ch.is_ascii_hexdigit())
        {
            return Err("The SCIM token hash must be 64 hexadecimal characters.".to_string());
        }
        if !matches!(self.default_role.as_str(), "viewer" | "operator") {
            return Err("The SCIM default role must be viewer or operator.".to_string());
        }
        Ok(())
    }

    pub fn default_role(&self) -> &str {
        &self.default_role
    }
}

/// A safe, non-secret summary for the owner's settings card. It never contains
/// the token or its hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RemoteScimStatus {
    /// The saved checkbox, independent of whether it validates.
    pub enabled: bool,
    /// True only when the running listener loaded a valid configuration.
    pub active: bool,
    pub token_configured: bool,
    pub default_role: String,
    pub last_used_at: Option<String>,
}

impl Default for RemoteScimStatus {
    fn default() -> Self {
        Self {
            enabled: false,
            active: false,
            token_configured: false,
            default_role: ScimConfig::DEFAULT_ROLE.to_string(),
            last_used_at: None,
        }
    }
}

/// A user as SCIM sees it. Only the core attributes RAVENBOT can honour are
/// exposed; emails, groups, phone numbers and passwords are deliberately absent.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ScimUser {
    pub id: String,
    pub user_name: String,
    pub external_id: Option<String>,
    pub active: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProvisionError {
    Conflict(String),
    Invalid(String),
    Forbidden(String),
    Internal(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScimFilterField {
    Id,
    UserName,
    ExternalId,
}

/// The account-registry write path used by SCIM.
///
/// This is deliberately separate from `AccountAuthenticator`: sign-in and
/// provisioning have different failure semantics, and SCIM must stay opt-in.
/// Every method is scoped to the workspace the implementation was built for.
#[async_trait::async_trait]
pub trait AccountProvisioner: Send + Sync {
    async fn list_users(&self) -> Result<Vec<ScimUser>, String>;

    async fn get_user(&self, id: &str) -> Result<Option<ScimUser>, String>;

    async fn find_user(
        &self,
        field: ScimFilterField,
        value: &str,
    ) -> Result<Option<ScimUser>, String>;

    async fn create_user(
        &self,
        user_name: &str,
        external_id: Option<&str>,
        active: bool,
        role: &str,
    ) -> Result<ScimUser, ProvisionError>;

    async fn set_active(&self, id: &str, active: bool) -> Result<ScimUser, ProvisionError>;

    async fn rename(&self, id: &str, user_name: &str) -> Result<ScimUser, ProvisionError>;
}

/// Generate a 32-byte bearer token as 64 hex characters. The caller returns it
/// once and persists only its hash.
pub fn generate_scim_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteConfig {
    pub port: u16,
    /// Bind `0.0.0.0` instead of loopback. The UI warns about cleartext LAN
    /// traffic; a tunnel (Tailscale, SSH) is the recommended remote path.
    pub allow_lan: bool,
    /// Serve HTTPS with a generated self-signed certificate. A real certificate
    /// belongs in front of the listener as a reverse proxy; this option makes a
    /// direct LAN connection encrypted, at the cost of a browser trust warning.
    pub tls: bool,
    /// Test/embedding override for the certificate directory. `None` uses
    /// `~/RAVENBOT/config/remote-tls`.
    #[serde(skip)]
    pub tls_dir: Option<PathBuf>,
    /// OIDC single sign-on for the mobile sign-in page. `None` leaves the
    /// username/password form as the only account path.
    #[serde(default)]
    pub sso: Option<SsoConfig>,
    /// SCIM 2.0 user provisioning. `None` disables every `/scim/v2` route.
    #[serde(default)]
    pub scim: Option<ScimConfig>,
}

impl Default for RemoteConfig {
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            allow_lan: false,
            tls: false,
            tls_dir: None,
            sso: None,
            scim: None,
        }
    }
}

impl RemoteConfig {
    pub fn bind_addr(&self) -> &'static str {
        if self.allow_lan {
            "0.0.0.0"
        } else {
            "127.0.0.1"
        }
    }

    pub fn normalized_port(&self) -> u16 {
        if self.port != 0 && self.port < 1024 {
            DEFAULT_PORT
        } else {
            self.port
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PairingCode {
    pub code: String,
    pub role: RemoteRole,
    pub expires_at: DateTime<Utc>,
}

/// A local RAVENBOT account, as the remote surface sees it.
#[derive(Debug, Clone, Serialize)]
pub struct RemoteAccount {
    pub id: String,
    pub username: String,
    /// The workspace role: `owner`, `operator` or `viewer`.
    pub role: String,
}

/// Backs remote sign-in with the local account registry.
///
/// `Ok(None)` means bad credentials or an account that is no longer allowed on
/// this workspace; `Err` is an internal backend failure. The HTTP boundary
/// deliberately uses generic copy for both so it never becomes an account
/// enumeration oracle.
#[async_trait::async_trait]
pub trait AccountAuthenticator: Send + Sync {
    async fn authenticate(
        &self,
        username: &str,
        password: &str,
    ) -> Result<Option<RemoteAccount>, String>;

    async fn revalidate(&self, account_id: &str) -> Result<Option<RemoteAccount>, String>;

    /// Resolve a verified SSO identity (usually `preferred_username`) to an
    /// existing local account on this workspace, case-insensitively. `Ok(None)`
    /// covers an unknown, disabled or grant-less account, exactly like a bad
    /// password: the HTTP layer must not distinguish them.
    async fn account_by_identity(&self, identity: &str) -> Result<Option<RemoteAccount>, String>;
}

#[derive(Debug, Clone, Serialize)]
pub struct PairedDevice {
    pub token: String,
    pub device_id: Uuid,
    pub name: String,
    pub role: RemoteRole,
    /// True when the host accepts account sign-in, so the page asks for the
    /// account credentials before showing conversations.
    pub account_login: bool,
}

/// Generate a one-time code and store only its hash. Generating a new code
/// invalidates every previous unused code.
pub async fn create_pairing(
    pool: &SqlitePool,
    role: RemoteRole,
    ttl: Duration,
) -> Result<PairingCode, String> {
    sqlx::query("UPDATE remote_pairings SET used_at = datetime('now') WHERE used_at IS NULL")
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    let code = generate_pairing_code();
    let ttl_secs = ttl.as_secs().clamp(60, 600) as i64;
    RemoteAccessQueries::create_pairing(pool, &hash_secret(&code), role.as_str(), ttl_secs)
        .await
        .map_err(|error| error.to_string())?;
    Ok(PairingCode {
        code,
        role,
        expires_at: Utc::now() + chrono::Duration::seconds(ttl_secs),
    })
}

pub fn generate_pairing_code() -> String {
    // 6 digits from 122 bits of UUID entropy. The short code is safe only
    // because it expires in minutes, is single-use, and attempts are limited.
    let n = Uuid::new_v4().as_u128() % 1_000_000;
    format!("{n:06}")
}

pub fn generate_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

pub fn hash_secret(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// Server
// ─────────────────────────────────────────────────────────────────────────────

pub struct RemoteServer {
    addr: SocketAddr,
    bind_all: bool,
    tls: bool,
    fingerprint: Option<String>,
    task: tokio::task::JoinHandle<()>,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
}

impl RemoteServer {
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn tls(&self) -> bool {
        self.tls
    }

    /// SHA-256 of the self-signed certificate, for the Settings card. `None`
    /// on plain HTTP or when a reverse proxy owns the certificate.
    pub fn fingerprint(&self) -> Option<&str> {
        self.fingerprint.as_deref()
    }

    /// The URL a phone on the same host can use. For a LAN bind this returns a
    /// best-effort LAN address; the UI also shows the port for manual entry.
    pub fn url(&self) -> String {
        let scheme = if self.tls { "https" } else { "http" };
        if self.bind_all {
            if let Some(ip) = lan_ip() {
                return format!("{scheme}://{}:{}", ip, self.addr.port());
            }
        }
        format!("{scheme}://127.0.0.1:{}", self.addr.port())
    }

    pub async fn shutdown(mut self) {
        if let Some(sender) = self.shutdown.take() {
            let _ = sender.send(());
        }
        let _ = tokio::time::timeout(Duration::from_secs(3), &mut self.task).await;
        self.task.abort();
    }
}

impl Drop for RemoteServer {
    fn drop(&mut self) {
        if let Some(sender) = self.shutdown.take() {
            let _ = sender.send(());
        }
        self.task.abort();
    }
}

fn lan_ip() -> Option<IpAddr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:80").ok()?;
    Some(socket.local_addr().ok()?.ip())
}

/// A self-signed certificate plus the SAN set it was generated for. The set is
/// persisted so a stable network does not force the user to re-trust the
/// certificate on every launch.
fn ensure_self_signed(dir: &FsPath, allow_lan: bool) -> Result<(Vec<u8>, Vec<u8>, String), String> {
    let mut sans = vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
    ];
    if allow_lan {
        if let Some(ip) = lan_ip() {
            sans.push(ip.to_string());
        }
    }
    sans.sort();
    sans.dedup();
    let meta = sans.join("\n");
    let cert_path = dir.join("cert.pem");
    let key_path = dir.join("key.pem");
    let meta_path = dir.join("sans.txt");
    let existing_meta = std::fs::read_to_string(&meta_path).ok();
    if existing_meta.as_deref() == Some(meta.as_str()) {
        if let (Ok(cert), Ok(key)) = (std::fs::read(&cert_path), std::fs::read(&key_path)) {
            if !cert.is_empty() && !key.is_empty() {
                let fingerprint = cert_fingerprint(&cert)?;
                return Ok((cert, key, fingerprint));
            }
        }
    }
    let mut params = rcgen::CertificateParams::new(sans)
        .map_err(|error| format!("Cannot build TLS certificate parameters: {error}"))?;
    params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
    params.key_usages = vec![rcgen::KeyUsagePurpose::DigitalSignature];
    params.distinguished_name = rcgen::DistinguishedName::new();
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "RAVENBOT Remote");
    let signing_key = rcgen::KeyPair::generate()
        .map_err(|error| format!("Cannot generate a TLS key: {error}"))?;
    let cert = params
        .self_signed(&signing_key)
        .map_err(|error| format!("Cannot sign the TLS certificate: {error}"))?;
    let cert_pem = cert.pem();
    let key_pem = signing_key.serialize_pem();
    std::fs::create_dir_all(dir)
        .map_err(|error| format!("Cannot create TLS directory {}: {error}", dir.display()))?;
    std::fs::write(&cert_path, &cert_pem)
        .map_err(|error| format!("Cannot write TLS certificate: {error}"))?;
    std::fs::write(&key_path, &key_pem)
        .map_err(|error| format!("Cannot write TLS key: {error}"))?;
    std::fs::write(&meta_path, &meta)
        .map_err(|error| format!("Cannot write TLS metadata: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
        let _ = std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600));
    }
    let fingerprint = cert_fingerprint(cert_pem.as_bytes())?;
    Ok((cert_pem.into_bytes(), key_pem.into_bytes(), fingerprint))
}

fn cert_fingerprint(cert_pem: &[u8]) -> Result<String, String> {
    use rustls::pki_types::{pem::PemObject, CertificateDer};
    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(cert_pem)
        .collect::<Result<_, _>>()
        .map_err(|error| format!("Cannot parse TLS certificate: {error}"))?;
    let first = certs
        .first()
        .ok_or_else(|| "TLS certificate file is empty".to_string())?;
    let digest = Sha256::digest(first.as_ref());
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn tls_acceptor(cert_pem: &[u8], key_pem: &[u8]) -> Result<TlsAcceptor, String> {
    use rustls::pki_types::{pem::PemObject, CertificateDer, PrivateKeyDer};
    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(cert_pem)
        .collect::<Result<_, _>>()
        .map_err(|error| format!("Cannot parse TLS certificate: {error}"))?;
    if certs.is_empty() {
        return Err("TLS certificate file is empty".to_string());
    }
    let key = PrivateKeyDer::from_pem_slice(key_pem)
        .map_err(|error| format!("Cannot parse TLS key: {error}"))?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|error| format!("Cannot configure TLS versions: {error}"))?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|error| format!("Cannot configure TLS certificate: {error}"))?;
    Ok(TlsAcceptor::from(Arc::new(config)))
}

struct TlsListener {
    inner: tokio::net::TcpListener,
    acceptor: TlsAcceptor,
}

/// Peer address for both listeners. A local newtype is required because the
/// `Connected` trait and `IncomingStream` are both foreign; implementing it for
/// `SocketAddr` directly trips the orphan rule.
#[derive(Debug, Clone, Copy)]
struct PeerAddr(SocketAddr);

impl<'a>
    axum::extract::connect_info::Connected<axum::serve::IncomingStream<'a, tokio::net::TcpListener>>
    for PeerAddr
{
    fn connect_info(stream: axum::serve::IncomingStream<'a, tokio::net::TcpListener>) -> Self {
        Self(*stream.remote_addr())
    }
}

impl<'a> axum::extract::connect_info::Connected<axum::serve::IncomingStream<'a, TlsListener>>
    for PeerAddr
{
    fn connect_info(stream: axum::serve::IncomingStream<'a, TlsListener>) -> Self {
        Self(*stream.remote_addr())
    }
}

impl axum::serve::Listener for TlsListener {
    type Io = tokio_rustls::server::TlsStream<tokio::net::TcpStream>;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            match self.inner.accept().await {
                Ok((stream, addr)) => match self.acceptor.accept(stream).await {
                    Ok(tls) => return (tls, addr),
                    Err(error) => {
                        tracing::debug!(%error, "TLS handshake failed");
                    }
                },
                Err(error) => {
                    tracing::warn!(%error, "TLS accept failed");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        self.inner.local_addr()
    }
}

pub async fn start(
    pool: SqlitePool,
    runtime: Arc<Runtime>,
    config: RemoteConfig,
) -> Result<RemoteServer, String> {
    start_with_authenticator(pool, runtime, config, None).await
}

/// Start the listener with optional account sign-in. `None` keeps the original
/// device-role-only behavior (and is what tests and embedded uses pass).
pub async fn start_with_authenticator(
    pool: SqlitePool,
    runtime: Arc<Runtime>,
    config: RemoteConfig,
    authenticator: Option<Arc<dyn AccountAuthenticator>>,
) -> Result<RemoteServer, String> {
    start_with_services(pool, runtime, config, authenticator, None).await
}

/// Start the listener with optional account sign-in and SCIM provisioning.
///
/// Both services are optional and fail closed: an invalid saved configuration
/// disables that feature with a warning but never stops the listener. SCIM
/// without a provisioner is unusable, so it is disabled rather than mounted as
/// a route that always returns 500.
pub async fn start_with_services(
    pool: SqlitePool,
    runtime: Arc<Runtime>,
    config: RemoteConfig,
    authenticator: Option<Arc<dyn AccountAuthenticator>>,
    provisioner: Option<Arc<dyn AccountProvisioner>>,
) -> Result<RemoteServer, String> {
    // Bound the pairing, audit and SSO attempt tables before the listener
    // accepts traffic.
    if let Err(error) = RemoteAccessQueries::prune(&pool).await {
        tracing::warn!(%error, "Could not prune remote access tables");
    }
    if let Err(error) = RemoteSsoQueries::prune_expired(&pool).await {
        tracing::warn!(%error, "Could not prune remote SSO attempts");
    }
    let port = config.normalized_port();
    let bind = format!("{}:{}", config.bind_addr(), port);
    // An invalid saved config disables SSO instead of stopping the listener.
    // Without an account authenticator no identity can be mapped, so SSO stays
    // off too rather than showing a button that can never complete.
    let sso = match config.sso.clone() {
        Some(config) => match (config.validate(), authenticator.is_some()) {
            (Ok(()), true) => Some(config),
            (Ok(()), false) => {
                tracing::warn!("Remote SSO is configured without an account authenticator");
                None
            }
            (Err(error), _) => {
                tracing::warn!(%error, "Invalid remote SSO settings; SSO is disabled");
                None
            }
        },
        None => None,
    };
    let scim = match config.scim.clone() {
        Some(config) => match (config.validate(), provisioner.is_some()) {
            (Ok(()), true) => Some(config),
            (Ok(()), false) => {
                tracing::warn!("Remote SCIM is configured without an account provisioner");
                None
            }
            (Err(error), _) => {
                tracing::warn!(%error, "Invalid remote SCIM settings; SCIM is disabled");
                None
            }
        },
        None => None,
    };
    let state = Arc::new(RemoteState::new_with_services(
        pool,
        runtime,
        authenticator,
        sso,
        scim,
        provisioner,
    ));
    let app = router(state);
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let allow_lan = config.allow_lan;
    let tls = config.tls;

    let (addr, task, fingerprint) = if tls {
        let dir = config
            .tls_dir
            .clone()
            .unwrap_or_else(|| ravenbot_core::config_dir().join("remote-tls"));
        let (cert, key, fingerprint) = ensure_self_signed(&dir, allow_lan)?;
        let acceptor = tls_acceptor(&cert, &key)?;
        let inner = tokio::net::TcpListener::bind(&bind)
            .await
            .map_err(|error| format!("Cannot bind remote access to {bind}: {error}"))?;
        let addr = inner
            .local_addr()
            .map_err(|error| format!("No local address for remote access: {error}"))?;
        let listener = TlsListener { inner, acceptor };
        let task = tokio::spawn(async move {
            let server = axum::serve(
                listener,
                app.into_make_service_with_connect_info::<PeerAddr>(),
            )
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            });
            if let Err(error) = server.await {
                tracing::warn!(%error, "remote access TLS server stopped");
            }
        });
        (addr, task, Some(fingerprint))
    } else {
        let listener = tokio::net::TcpListener::bind(&bind)
            .await
            .map_err(|error| format!("Cannot bind remote access to {bind}: {error}"))?;
        let addr = listener
            .local_addr()
            .map_err(|error| format!("No local address for remote access: {error}"))?;
        let task = tokio::spawn(async move {
            let server = axum::serve(
                listener,
                app.into_make_service_with_connect_info::<PeerAddr>(),
            )
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            });
            if let Err(error) = server.await {
                tracing::warn!(%error, "remote access server stopped");
            }
        });
        (addr, task, None)
    };

    tracing::info!(%addr, allow_lan, tls, "Remote access listening");
    Ok(RemoteServer {
        addr,
        bind_all: allow_lan,
        tls,
        fingerprint,
        task,
        shutdown: Some(shutdown_tx),
    })
}

#[derive(Debug, Clone)]
struct CachedGrant {
    account: RemoteAccount,
    checked_at: Instant,
}

struct ActiveSso {
    config: SsoConfig,
    client: reqwest::Client,
}

pub struct RemoteState {
    pool: SqlitePool,
    runtime: Arc<Runtime>,
    authenticator: Option<Arc<dyn AccountAuthenticator>>,
    provisioner: Option<Arc<dyn AccountProvisioner>>,
    /// A validated, active SCIM configuration. `None` means every SCIM route
    /// returns 404, including with a token.
    scim: Option<ScimConfig>,
    sso: Option<ActiveSso>,
    pairing_attempts: Mutex<HashMap<IpAddr, VecDeque<Instant>>>,
    global_pairing_attempts: Mutex<VecDeque<Instant>>,
    login_attempts: Mutex<HashMap<IpAddr, VecDeque<Instant>>>,
    global_login_attempts: Mutex<VecDeque<Instant>>,
    sso_attempts: Mutex<HashMap<IpAddr, VecDeque<Instant>>>,
    global_sso_attempts: Mutex<VecDeque<Instant>>,
    scim_attempts: Mutex<HashMap<IpAddr, VecDeque<Instant>>>,
    global_scim_attempts: Mutex<VecDeque<Instant>>,
    grants: Mutex<HashMap<String, CachedGrant>>,
    last_touch: Mutex<HashMap<String, Instant>>,
}

impl RemoteState {
    #[cfg(test)]
    fn new(pool: SqlitePool, runtime: Arc<Runtime>) -> Self {
        Self::new_with_services(pool, runtime, None, None, None, None)
    }

    #[cfg(test)]
    fn new_with_authenticator(
        pool: SqlitePool,
        runtime: Arc<Runtime>,
        authenticator: Option<Arc<dyn AccountAuthenticator>>,
        sso: Option<SsoConfig>,
    ) -> Self {
        Self::new_with_services(pool, runtime, authenticator, sso, None, None)
    }

    fn new_with_services(
        pool: SqlitePool,
        runtime: Arc<Runtime>,
        authenticator: Option<Arc<dyn AccountAuthenticator>>,
        sso: Option<SsoConfig>,
        scim: Option<ScimConfig>,
        provisioner: Option<Arc<dyn AccountProvisioner>>,
    ) -> Self {
        let sso = sso.and_then(|config| match sso::http_client() {
            Ok(client) => Some(ActiveSso { config, client }),
            Err(error) => {
                tracing::warn!(%error, "Could not build the SSO HTTP client; SSO is disabled");
                None
            }
        });
        Self {
            pool,
            runtime,
            authenticator,
            provisioner,
            scim,
            sso,
            pairing_attempts: Mutex::new(HashMap::new()),
            global_pairing_attempts: Mutex::new(VecDeque::new()),
            login_attempts: Mutex::new(HashMap::new()),
            global_login_attempts: Mutex::new(VecDeque::new()),
            sso_attempts: Mutex::new(HashMap::new()),
            global_sso_attempts: Mutex::new(VecDeque::new()),
            scim_attempts: Mutex::new(HashMap::new()),
            global_scim_attempts: Mutex::new(VecDeque::new()),
            grants: Mutex::new(HashMap::new()),
            last_touch: Mutex::new(HashMap::new()),
        }
    }

    fn pairing_allowed(&self, ip: IpAddr) -> bool {
        rate_limit_allowed(
            &self.pairing_attempts,
            &self.global_pairing_attempts,
            ip,
            MAX_PAIRING_ATTEMPTS_PER_MINUTE,
            MAX_GLOBAL_PAIRING_ATTEMPTS_PER_MINUTE,
        )
    }

    fn login_allowed(&self, ip: IpAddr) -> bool {
        rate_limit_allowed(
            &self.login_attempts,
            &self.global_login_attempts,
            ip,
            MAX_LOGIN_ATTEMPTS_PER_MINUTE,
            MAX_GLOBAL_LOGIN_ATTEMPTS_PER_MINUTE,
        )
    }

    fn sso_callback_allowed(&self, ip: IpAddr) -> bool {
        rate_limit_allowed(
            &self.sso_attempts,
            &self.global_sso_attempts,
            ip,
            MAX_SSO_CALLBACKS_PER_MINUTE,
            MAX_GLOBAL_SSO_CALLBACKS_PER_MINUTE,
        )
    }

    fn scim_allowed(&self, ip: IpAddr) -> bool {
        rate_limit_allowed(
            &self.scim_attempts,
            &self.global_scim_attempts,
            ip,
            MAX_SCIM_REQUESTS_PER_MINUTE,
            MAX_GLOBAL_SCIM_REQUESTS_PER_MINUTE,
        )
    }

    /// A grant is trusted for a minute; after that the next request asks the
    /// account registry again. The caller recomputes the effective role on
    /// every request even when the grant itself is cached.
    fn cached_account(&self, device_id: &str) -> Option<RemoteAccount> {
        let now = Instant::now();
        let mut grants = self.grants.lock().expect("grant lock");
        match grants.get(device_id) {
            Some(grant) if now.duration_since(grant.checked_at) < ACCOUNT_REVALIDATE_INTERVAL => {
                Some(grant.account.clone())
            }
            Some(_) => {
                grants.remove(device_id);
                None
            }
            None => None,
        }
    }

    fn cache_account(&self, device_id: &str, account: &RemoteAccount) {
        self.grants.lock().expect("grant lock").insert(
            device_id.to_string(),
            CachedGrant {
                account: account.clone(),
                checked_at: Instant::now(),
            },
        );
    }

    fn clear_cached_account(&self, device_id: &str) {
        self.grants.lock().expect("grant lock").remove(device_id);
    }

    #[cfg(test)]
    fn clear_all_cached_accounts(&self) {
        self.grants.lock().expect("grant lock").clear();
    }

    fn should_touch(&self, device_id: &str) -> bool {
        let now = Instant::now();
        let mut last = self.last_touch.lock().expect("touch lock");
        match last.get(device_id) {
            Some(at) if now.duration_since(*at) < Duration::from_secs(60) => false,
            _ => {
                last.insert(device_id.to_string(), now);
                true
            }
        }
    }
}

fn rate_limit_allowed(
    by_ip: &Mutex<HashMap<IpAddr, VecDeque<Instant>>>,
    global: &Mutex<VecDeque<Instant>>,
    ip: IpAddr,
    per_ip_max: usize,
    global_max: usize,
) -> bool {
    let now = Instant::now();
    let mut global = global.lock().expect("rate limit lock");
    while global
        .front()
        .is_some_and(|at| now.duration_since(*at) > Duration::from_secs(60))
    {
        global.pop_front();
    }
    if global.len() >= global_max {
        return false;
    }
    let mut by_ip = by_ip.lock().expect("rate limit lock");
    let attempts = by_ip.entry(ip).or_default();
    while attempts
        .front()
        .is_some_and(|at| now.duration_since(*at) > Duration::from_secs(60))
    {
        attempts.pop_front();
    }
    if attempts.len() >= per_ip_max {
        return false;
    }
    attempts.push_back(now);
    global.push_back(now);
    true
}

pub fn router(state: Arc<RemoteState>) -> Router {
    let public = Router::new()
        .route("/", get(index))
        .route("/app.css", get(app_css))
        .route("/app.js", get(app_js))
        .route("/api/health", get(health))
        .route("/api/pair", post(pair))
        .route("/api/sso/callback", get(sso_callback))
        .route("/share/{token}", get(share_page));

    let protected = Router::new()
        .route("/api/status", get(status))
        .route("/api/login", post(login))
        .route("/api/sso/start", post(sso_start))
        .route("/api/bots", get(bots))
        .route("/api/threads", get(threads))
        .route(
            "/api/threads/{thread_id}/messages",
            get(messages).post(send_message),
        )
        .route("/api/attention", get(attention))
        .route("/api/approvals", get(approvals))
        .route("/api/approvals/{approval_id}", post(decide_approval))
        .route("/api/questions", get(questions))
        .route("/api/questions/{question_id}", post(answer_question))
        .route("/api/runs/{run_id}", get(run_status))
        .route("/api/runs/{run_id}/cancel", post(cancel_run))
        .route("/api/device/revoke", post(revoke_self))
        .route("/api/remote/audit", get(remote_audit))
        .route_layer(middleware::from_fn_with_state(state.clone(), authenticate));

    public
        .merge(protected)
        .merge(scim::router(state.clone()))
        .with_state(state)
        .layer(DefaultBodyLimit::max(MAX_ANSWER_BYTES))
}

// ─────────────────────────────────────────────────────────────────────────────
// Errors and authentication
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
    code: Option<&'static str>,
}

impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
            code: None,
        }
    }

    fn coded(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
            code: Some(code),
        }
    }

    fn bad(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    fn unauthorized() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "Pairing is required. Generate a code in RAVENBOT Settings → Remote access.",
        )
    }

    fn login_required() -> Self {
        Self::coded(
            StatusCode::UNAUTHORIZED,
            "login_required",
            "Sign in with your RAVENBOT account to use remote access.",
        )
    }

    fn account_revoked() -> Self {
        Self::coded(
            StatusCode::UNAUTHORIZED,
            "account_revoked",
            "Your account no longer has access to this workspace. Sign in again.",
        )
    }

    fn invalid_credentials() -> Self {
        Self::coded(
            StatusCode::UNAUTHORIZED,
            "invalid_credentials",
            "Invalid username or password.",
        )
    }

    fn forbidden() -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            "This device's role does not allow that action.",
        )
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, message)
    }

    fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, message)
    }

    fn internal(context: &str, error: impl std::fmt::Display) -> Self {
        tracing::error!(%error, context, "remote access error");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "The local RAVENBOT service could not complete that request.",
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut body = json!({ "error": self.message });
        if let Some(code) = self.code {
            body["code"] = json!(code);
        }
        (self.status, Json(body)).into_response()
    }
}

type ApiResult<T> = Result<T, ApiError>;

#[derive(Debug, Clone)]
struct AuthDevice {
    id: String,
    name: String,
    /// The paired role, before the account grant caps it.
    device_role: RemoteRole,
    /// Effective role: the lower of the pair and the account grant.
    role: RemoteRole,
    account: Option<RemoteAccount>,
    login_required: bool,
}

impl AuthDevice {
    fn account_name(&self) -> Option<&str> {
        self.account
            .as_ref()
            .map(|account| account.username.as_str())
    }
}

fn bearer_token(request: &Request) -> Option<String> {
    let value = request
        .headers()
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?;
    let token = value.strip_prefix("Bearer ")?.trim();
    if token.is_empty() || token.len() > 256 {
        return None;
    }
    Some(token.to_string())
}

async fn authenticate(
    State(state): State<Arc<RemoteState>>,
    mut request: Request,
    next: Next,
) -> Response {
    let Some(token) = bearer_token(&request) else {
        return ApiError::unauthorized().into_response();
    };
    let device =
        match RemoteAccessQueries::device_for_token(&state.pool, &hash_secret(&token)).await {
            Ok(Some(device)) => device,
            Ok(None) => return ApiError::unauthorized().into_response(),
            Err(error) => return ApiError::internal("remote token lookup", error).into_response(),
        };
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    // These stay reachable before the device is bound: sign in, leave, and
    // discover that sign-in is required.
    let is_login = path == "/api/login";
    let is_sso_start = path == "/api/sso/start";
    let self_revoke = path == "/api/device/revoke";
    let device_role = RemoteRole::parse(&device.role);
    let mut account = None;
    let mut login_required = false;

    if let Some(authenticator) = state.authenticator.clone() {
        match device.account_id.as_deref() {
            Some(account_id) => {
                let outcome = match state.cached_account(&device.id) {
                    Some(cached) => Ok(Some(cached)),
                    None => authenticator.revalidate(account_id).await,
                };
                let granted = match outcome {
                    Ok(Some(granted)) => Some(granted),
                    Ok(None) => None,
                    Err(error) => {
                        tracing::warn!(%error, "remote account revalidation failed");
                        None
                    }
                };
                match granted {
                    Some(granted) => {
                        state.cache_account(&device.id, &granted);
                        account = Some(granted);
                    }
                    None => {
                        // Fail closed and drop the binding so the page asks for
                        // credentials instead of retrying a dead grant forever.
                        state.clear_cached_account(&device.id);
                        let _ = RemoteAccessQueries::clear_account_binding(&state.pool, &device.id)
                            .await;
                        let _ = RemoteAccessQueries::audit(
                            &state.pool,
                            Some(&device.id),
                            Some(&device.name),
                            None,
                            None,
                            method.as_str(),
                            &path,
                            401,
                        )
                        .await;
                        return ApiError::account_revoked().into_response();
                    }
                }
            }
            None => {
                login_required = true;
                if !(is_login || is_sso_start || self_revoke || path == "/api/status") {
                    return ApiError::login_required().into_response();
                }
            }
        }
    }

    let role = account
        .as_ref()
        .map_or(device_role, |account| device_role.capped_by(&account.role));
    // A device may revoke itself even as a viewer; sign-in must be reachable
    // with a viewer pairing. Every other mutation needs the effective role.
    if method != Method::GET
        && method != Method::HEAD
        && !self_revoke
        && !is_login
        && !is_sso_start
        && !role.can_write()
    {
        return ApiError::forbidden().into_response();
    }
    let auth = AuthDevice {
        id: device.id.clone(),
        name: device.name.clone(),
        device_role,
        role,
        account,
        login_required,
    };
    request.extensions_mut().insert(auth.clone());
    let response = next.run(request).await;
    let status = response.status().as_u16() as i64;
    // Reads are polled; only mutations and failures are persisted, so polling
    // does not turn into a write every three seconds. Sign-in writes its own
    // row so it can carry the newly bound account.
    if !is_login && (method != Method::GET || status >= 400) {
        let _ = RemoteAccessQueries::audit(
            &state.pool,
            Some(&auth.id),
            Some(&auth.name),
            Some(auth.role.as_str()),
            auth.account_name(),
            method.as_str(),
            &path,
            status,
        )
        .await;
    }
    if state.should_touch(&auth.id) {
        let _ = RemoteAccessQueries::touch_device(&state.pool, &auth.id).await;
    }
    response
}

// ─────────────────────────────────────────────────────────────────────────────
// Public endpoints
// ─────────────────────────────────────────────────────────────────────────────

async fn index() -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CONTENT_SECURITY_POLICY, CSP),
            (header::CACHE_CONTROL, "no-store"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::X_FRAME_OPTIONS, "DENY"),
            (header::REFERRER_POLICY, "no-referrer"),
        ],
        Html(include_str!("assets/index.html")),
    )
        .into_response()
}

async fn app_css() -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        include_str!("assets/app.css"),
    )
        .into_response()
}

async fn app_js() -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        include_str!("assets/app.js"),
    )
        .into_response()
}

/// A read-only public conversation page. The token is an unguessable bearer
/// capability and the page exposes only user/assistant text; tool calls,
/// reasoning, attachments, approvals and questions are never included.
async fn share_page(State(state): State<Arc<RemoteState>>, Path(token): Path<String>) -> Response {
    let token = token.trim();
    if token.len() < 32 || token.len() > 128 {
        return share_not_found();
    }
    let share = match ThreadShareQueries::for_token(&state.pool, &hash_secret(token)).await {
        Ok(Some(share)) => share,
        Ok(None) => return share_not_found(),
        Err(error) => {
            tracing::warn!(%error, "share lookup failed");
            return share_not_found();
        }
    };
    let thread = match ThreadQueries::get(&state.pool, share.thread_id).await {
        Ok(Some(thread)) => thread,
        Ok(None) => return share_not_found(),
        Err(error) => {
            tracing::warn!(%error, "share thread lookup failed");
            return share_not_found();
        }
    };
    let cutoff = match DateTime::parse_from_rfc3339(&share.created_at) {
        Ok(value) => value.with_timezone(&Utc),
        Err(_) => return share_not_found(),
    };
    let messages = match MessageQueries::list_by_thread(&state.pool, share.thread_id).await {
        Ok(messages) => messages,
        Err(error) => {
            tracing::warn!(%error, "share message lookup failed");
            return share_not_found();
        }
    };
    let visible: Vec<&ravenbot_core::Message> = messages
        .iter()
        .filter(|message| message.created_at <= cutoff)
        .filter(|message| {
            matches!(
                message.role,
                ravenbot_core::MessageRole::User | ravenbot_core::MessageRole::Assistant
            )
        })
        .filter(|message| share_message_text(message).is_some())
        .collect();
    let truncated = visible.len() > MAX_SHARE_MESSAGES;
    let visible = &visible[visible.len().saturating_sub(MAX_SHARE_MESSAGES)..];
    let html = render_share_page(
        &thread.title,
        visible,
        &share.expires_at,
        &share.created_at,
        truncated,
    );
    let _ = ThreadShareQueries::record_view(&state.pool, share.id).await;
    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CONTENT_SECURITY_POLICY, CSP),
            (header::CACHE_CONTROL, "no-store"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::X_FRAME_OPTIONS, "DENY"),
            (header::REFERRER_POLICY, "no-referrer"),
            (
                header::HeaderName::from_static("x-robots-tag"),
                "noindex, nofollow",
            ),
        ],
        Html(html),
    )
        .into_response()
}

fn share_not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CONTENT_SECURITY_POLICY, CSP),
            (header::CACHE_CONTROL, "no-store"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::X_FRAME_OPTIONS, "DENY"),
            (header::REFERRER_POLICY, "no-referrer"),
            (
                header::HeaderName::from_static("x-robots-tag"),
                "noindex, nofollow",
            ),
        ],
        Html(
            "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
             <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
             <meta name=\"robots\" content=\"noindex,nofollow\">\
             <title>Not found — RAVENBOT</title><link rel=\"stylesheet\" href=\"/app.css\">\
             </head><body><main class=\"share-page\"><h1>404</h1>\
             <p>This shared conversation is unavailable.</p></main></body></html>",
        ),
    )
        .into_response()
}

/// Public text for one message. Reasoning, tool traces, tool results and
/// attachment bytes are deliberately absent.
fn share_message_text(message: &ravenbot_core::Message) -> Option<String> {
    let mut text = match &message.content {
        ravenbot_core::MessageContent::Text { text, .. } => text.clone(),
        ravenbot_core::MessageContent::Checklist { text, items } => {
            let mut out = text.clone().unwrap_or_default();
            for item in items {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str("- ");
                out.push_str(&item.label);
            }
            out
        }
        _ => return None,
    };
    if text.trim().is_empty() {
        return None;
    }
    if !message.attachments.is_empty() {
        text.push_str("\n\n[attachment omitted from this share]");
    }
    Some(text)
}

fn escape_html(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

fn render_share_page(
    title: &str,
    messages: &[&ravenbot_core::Message],
    expires_at: &str,
    cutoff: &str,
    truncated: bool,
) -> String {
    let mut articles = String::new();
    for message in messages {
        let Some(text) = share_message_text(message) else {
            continue;
        };
        let is_user = matches!(message.role, ravenbot_core::MessageRole::User);
        let role = if is_user {
            "User".to_string()
        } else {
            message
                .sender_name
                .clone()
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| "Assistant".to_string())
        };
        articles.push_str(&format!(
            "<article class=\"share-message{}\" role=\"article\">\
             <div class=\"share-role\">{}</div>\
             <div class=\"share-text\">{}</div>\
             <div class=\"share-time\">{}</div>\
             </article>",
            if is_user { " share-message--user" } else { "" },
            escape_html(&role),
            escape_html(&text),
            escape_html(
                &message
                    .created_at
                    .format("%b %d, %Y · %H:%M UTC")
                    .to_string()
            ),
        ));
    }
    let empty = if messages.is_empty() {
        "<p class=\"share-empty\">No messages were shared.</p>".to_string()
    } else {
        String::new()
    };
    let truncation = if truncated {
        format!(
            "<p class=\"share-note\">Only the most recent {} messages are shown.</p>",
            MAX_SHARE_MESSAGES
        )
    } else {
        String::new()
    };
    let expires = DateTime::parse_from_rfc3339(expires_at)
        .map(|value| {
            value
                .with_timezone(&Utc)
                .format("%b %d, %Y · %H:%M UTC")
                .to_string()
        })
        .unwrap_or_else(|_| expires_at.to_string());
    let cutoff = DateTime::parse_from_rfc3339(cutoff)
        .map(|value| {
            value
                .with_timezone(&Utc)
                .format("%b %d, %Y · %H:%M UTC")
                .to_string()
        })
        .unwrap_or_else(|_| cutoff.to_string());
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <meta name=\"robots\" content=\"noindex,nofollow\">\
         <title>{} — RAVENBOT</title><link rel=\"stylesheet\" href=\"/app.css\">\
         </head><body><main class=\"share-page\">\
         <header class=\"share-header\"><div class=\"mark\">R</div>\
         <div><div class=\"share-kicker\">Shared conversation</div>\
         <h1>{}</h1></div></header>\
         <p class=\"share-note\">Read-only. Messages after {} are not included. \
         Tool calls, reasoning, attachments, approvals and questions are omitted.</p>{}\
         <section class=\"share-messages\">{}{}</section>\
         <footer class=\"share-footer\">Link expires {}. Shared by RAVENBOT.</footer>\
         </main></body></html>",
        escape_html(title),
        escape_html(title),
        escape_html(&cutoff),
        truncation,
        articles,
        empty,
        escape_html(&expires),
    )
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "ok": true, "version": env!("CARGO_PKG_VERSION") }))
}

#[derive(Debug, Deserialize)]
struct PairBody {
    code: String,
    device_name: String,
}

async fn pair(
    State(state): State<Arc<RemoteState>>,
    ConnectInfo(peer): ConnectInfo<PeerAddr>,
    Json(body): Json<PairBody>,
) -> ApiResult<Json<PairedDevice>> {
    if !state.pairing_allowed(peer.0.ip()) {
        return Err(ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many pairing attempts. Wait a minute and try again.",
        ));
    }
    let code: String = body.code.chars().filter(|ch| ch.is_ascii_digit()).collect();
    if code.len() != 6 {
        return Err(ApiError::bad("The pairing code is six digits."));
    }
    let name = body.device_name.trim();
    if name.is_empty() || name.chars().count() > 64 {
        return Err(ApiError::bad("Give this device a name (1–64 characters)."));
    }
    if name.chars().any(|ch| ch.is_control()) {
        return Err(ApiError::bad(
            "The device name contains invalid characters.",
        ));
    }
    let Some((_pairing_id, role)) =
        RemoteAccessQueries::consume_pairing(&state.pool, &hash_secret(&code))
            .await
            .map_err(|error| ApiError::internal("consume pairing", error))?
    else {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "That pairing code is wrong or expired. Generate a new one.",
        ));
    };
    let role = RemoteRole::parse(&role);
    let token = generate_token();
    let device_id =
        RemoteAccessQueries::create_device(&state.pool, name, role.as_str(), &hash_secret(&token))
            .await
            .map_err(|error| ApiError::internal("create remote device", error))?;
    let _ = RemoteAccessQueries::audit(
        &state.pool,
        Some(&device_id.to_string()),
        Some(name),
        Some(role.as_str()),
        None,
        "PAIR",
        "/api/pair",
        200,
    )
    .await;
    Ok(Json(PairedDevice {
        token,
        device_id,
        name: name.to_string(),
        role,
        account_login: state.authenticator.is_some(),
    }))
}

// ─────────────────────────────────────────────────────────────────────────────
// Authenticated endpoints
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct LoginBody {
    username: String,
    password: String,
}

/// Bind this paired device to a local account. The device token is required,
/// but the device does not need to be bound yet; that is the point of this
/// endpoint. Rate limits are independent of pairing.
async fn login(
    State(state): State<Arc<RemoteState>>,
    ConnectInfo(peer): ConnectInfo<PeerAddr>,
    Extension(auth): Extension<AuthDevice>,
    Json(body): Json<LoginBody>,
) -> ApiResult<Json<serde_json::Value>> {
    if !state.login_allowed(peer.0.ip()) {
        return Err(ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many sign-in attempts. Wait a minute and try again.",
        ));
    }
    let Some(authenticator) = state.authenticator.clone() else {
        return Err(ApiError::conflict(
            "Account sign-in is not enabled on this host.",
        ));
    };
    let username = body.username.trim();
    if username.is_empty() || body.password.is_empty() {
        let _ = RemoteAccessQueries::audit(
            &state.pool,
            Some(&auth.id),
            Some(&auth.name),
            None,
            None,
            "POST",
            "/api/login",
            401,
        )
        .await;
        return Err(ApiError::invalid_credentials());
    }
    match authenticator.authenticate(username, &body.password).await {
        Ok(Some(account)) => {
            let role = auth.device_role.capped_by(&account.role);
            let bound = RemoteAccessQueries::bind_account(
                &state.pool,
                &auth.id,
                &account.id,
                &account.username,
                &account.role,
            )
            .await
            .map_err(|error| ApiError::internal("bind remote account", error))?;
            if !bound {
                // The device was revoked between lookup and binding.
                return Err(ApiError::unauthorized());
            }
            state.cache_account(&auth.id, &account);
            let _ = RemoteAccessQueries::audit(
                &state.pool,
                Some(&auth.id),
                Some(&auth.name),
                Some(role.as_str()),
                Some(&account.username),
                "POST",
                "/api/login",
                200,
            )
            .await;
            Ok(Json(json!({
                "account": { "id": account.id, "username": account.username },
                "role": role.as_str(),
            })))
        }
        Ok(None) => {
            let _ = RemoteAccessQueries::audit(
                &state.pool,
                Some(&auth.id),
                Some(&auth.name),
                None,
                None,
                "POST",
                "/api/login",
                401,
            )
            .await;
            Err(ApiError::invalid_credentials())
        }
        Err(error) => {
            let _ = RemoteAccessQueries::audit(
                &state.pool,
                Some(&auth.id),
                Some(&auth.name),
                None,
                None,
                "POST",
                "/api/login",
                500,
            )
            .await;
            Err(ApiError::internal("remote account authentication", error))
        }
    }
}

/// Begin an OIDC authorization-code flow for this paired device. The device
/// bearer token is required, so a state can only be minted for a device the
/// host already knows. The session is not bound until the public callback
/// verifies a provider-issued ID token and maps it to a local account.
async fn sso_start(
    State(state): State<Arc<RemoteState>>,
    Extension(auth): Extension<AuthDevice>,
) -> ApiResult<Response> {
    let Some(active) = state.sso.as_ref() else {
        return Err(ApiError::conflict(
            "Single sign-on is not configured on this host.",
        ));
    };
    if state.authenticator.is_none() {
        return Err(ApiError::conflict(
            "Single sign-on is not configured on this host.",
        ));
    }
    if let Err(error) = RemoteSsoQueries::prune_expired(&state.pool).await {
        return Err(ApiError::internal("prune remote SSO attempts", error));
    }
    let outstanding = RemoteSsoQueries::outstanding_for_device(&state.pool, &auth.id)
        .await
        .map_err(|error| ApiError::internal("count remote SSO attempts", error))?;
    let total = RemoteSsoQueries::outstanding_total(&state.pool)
        .await
        .map_err(|error| ApiError::internal("count remote SSO attempts", error))?;
    if outstanding >= sso::MAX_ATTEMPTS_PER_DEVICE || total >= sso::MAX_GLOBAL_ATTEMPTS {
        return Err(ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many sign-in attempts. Wait ten minutes and try again.",
        ));
    }
    let provider = sso::discover(&active.client, &active.config)
        .await
        .map_err(|failure| {
            tracing::warn!(
                reason = failure.reason(),
                "Remote SSO discovery failed while starting a flow"
            );
            ApiError::new(
                StatusCode::BAD_GATEWAY,
                "Single sign-on could not start. Ask the host owner to check the identity provider settings.",
            )
        })?;
    let secrets = sso::generate_secrets();
    let created = RemoteSsoQueries::create(
        &state.pool,
        &NewRemoteSsoAttempt {
            state_hash: &hash_secret(&secrets.state),
            device_id: &auth.id,
            nonce: &secrets.nonce,
            verifier: &secrets.verifier,
            issuer: &active.config.issuer,
            client_id: &active.config.client_id,
            redirect_url: &active.config.redirect_url,
            token_endpoint: &provider.token_endpoint,
            jwks_uri: &provider.jwks_uri,
            username_claim: &active.config.username_claim,
            ttl_secs: sso::ATTEMPT_TTL_SECS,
        },
        sso::MAX_ATTEMPTS_PER_DEVICE,
        sso::MAX_GLOBAL_ATTEMPTS,
    )
    .await
    .map_err(|error| ApiError::internal("store remote SSO attempt", error))?;
    if !created {
        return Err(ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many sign-in attempts. Wait ten minutes and try again.",
        ));
    }
    let url = sso::authorization_url(&active.config, &provider, &secrets).map_err(|failure| {
        tracing::warn!(
            reason = failure.reason(),
            "Remote SSO authorization URL could not be built"
        );
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            "Single sign-on could not start. Ask the host owner to check the identity provider settings.",
        )
    })?;
    Ok((
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::REFERRER_POLICY, "no-referrer"),
        ],
        Json(json!({ "url": url })),
    )
        .into_response())
}

#[derive(Debug, Deserialize)]
struct SsoCallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

fn sso_redirect(active: &ActiveSso, failure: Option<sso::SsoFailure>) -> Response {
    let Ok(origin) = active.config.redirect_origin() else {
        return ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Single sign-on is misconfigured on this host.",
        )
        .into_response();
    };
    let location = match failure {
        None => format!("{origin}/?sso=ok"),
        Some(failure) => format!("{origin}/?sso=error&reason={}", failure.reason()),
    };
    (
        [
            (header::LOCATION, location),
            (header::CACHE_CONTROL, "no-store".to_string()),
            (header::REFERRER_POLICY, "no-referrer".to_string()),
        ],
        StatusCode::SEE_OTHER,
    )
        .into_response()
}

async fn sso_failed(
    state: &RemoteState,
    active: &ActiveSso,
    attempt: Option<&ravenbot_db::queries::RemoteSsoAttempt>,
    failure: sso::SsoFailure,
    audit_status: i64,
) -> Response {
    let device = match attempt {
        Some(attempt) => RemoteAccessQueries::device_by_id(&state.pool, &attempt.device_id)
            .await
            .ok()
            .flatten(),
        None => None,
    };
    let _ = RemoteAccessQueries::audit(
        &state.pool,
        device
            .as_ref()
            .map(|device| device.id.as_str())
            .or_else(|| attempt.map(|attempt| attempt.device_id.as_str())),
        device.as_ref().map(|device| device.name.as_str()),
        None,
        None,
        "SSO",
        "/api/sso/callback",
        audit_status,
    )
    .await;
    tracing::warn!(reason = failure.reason(), "Remote SSO callback failed");
    sso_redirect(active, Some(failure))
}

/// Public OIDC callback. Every outcome is a redirect to the origin of the
/// configured redirect URL, never to the request's Host header. Provider error
/// text, codes, tokens, nonces and verifiers are never reflected or logged.
async fn sso_callback(
    State(state): State<Arc<RemoteState>>,
    ConnectInfo(peer): ConnectInfo<PeerAddr>,
    Query(query): Query<SsoCallbackQuery>,
) -> Response {
    let Some(active) = state.sso.as_ref() else {
        return ApiError::not_found("Not found.").into_response();
    };
    if !state.sso_callback_allowed(peer.0.ip()) {
        return ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many sign-in attempts. Wait a minute and try again.",
        )
        .into_response();
    }
    let _ = RemoteSsoQueries::prune_expired(&state.pool).await;

    // A provider error is a user-facing authorization failure. Consume any
    // state that is present so it cannot be replayed, then redirect with a
    // fixed reason and never forward error_description.
    if query.error.is_some() {
        let attempt = match query.state.as_deref() {
            Some(value) if !value.is_empty() && value.len() <= 128 => {
                RemoteSsoQueries::consume(&state.pool, &hash_secret(value))
                    .await
                    .ok()
                    .flatten()
            }
            _ => None,
        };
        return sso_failed(
            &state,
            active,
            attempt.as_ref(),
            sso::SsoFailure::Provider,
            401,
        )
        .await;
    }

    let Some(state_value) = query.state.as_deref().filter(|value| !value.is_empty()) else {
        return sso_failed(&state, active, None, sso::SsoFailure::State, 401).await;
    };
    if state_value.len() > 128 {
        return sso_failed(&state, active, None, sso::SsoFailure::State, 401).await;
    }
    let attempt = match RemoteSsoQueries::consume(&state.pool, &hash_secret(state_value)).await {
        Ok(Some(attempt)) => attempt,
        Ok(None) => {
            return sso_failed(&state, active, None, sso::SsoFailure::State, 401).await;
        }
        Err(error) => {
            tracing::warn!(%error, "Remote SSO attempt lookup failed");
            return sso_failed(&state, active, None, sso::SsoFailure::Exchange, 500).await;
        }
    };
    let Some(code) = query.code.as_deref().filter(|value| !value.is_empty()) else {
        return sso_failed(
            &state,
            active,
            Some(&attempt),
            sso::SsoFailure::Exchange,
            500,
        )
        .await;
    };
    let id_token = match sso::exchange_code(
        &active.client,
        &attempt.token_endpoint,
        &attempt.client_id,
        &attempt.redirect_url,
        &attempt.verifier,
        code,
    )
    .await
    {
        Ok(token) => token,
        Err(failure) => {
            return sso_failed(
                &state,
                active,
                Some(&attempt),
                failure,
                failure.audit_status(),
            )
            .await
        }
    };
    let identity = match sso::verify_id_token(
        &active.client,
        &attempt.jwks_uri,
        &id_token,
        &attempt.issuer,
        &attempt.client_id,
        &attempt.nonce,
        &attempt.username_claim,
    )
    .await
    {
        Ok(identity) => identity,
        Err(failure) => {
            return sso_failed(
                &state,
                active,
                Some(&attempt),
                failure,
                failure.audit_status(),
            )
            .await
        }
    };
    let Some(authenticator) = state.authenticator.clone() else {
        return sso_failed(&state, active, Some(&attempt), sso::SsoFailure::Config, 500).await;
    };
    let account = match authenticator.account_by_identity(&identity).await {
        Ok(Some(account)) => account,
        Ok(None) => {
            return sso_failed(
                &state,
                active,
                Some(&attempt),
                sso::SsoFailure::Account,
                401,
            )
            .await
        }
        Err(error) => {
            tracing::warn!(%error, "Remote SSO account lookup failed");
            return sso_failed(
                &state,
                active,
                Some(&attempt),
                sso::SsoFailure::Account,
                500,
            )
            .await;
        }
    };
    let bound = match RemoteAccessQueries::bind_account(
        &state.pool,
        &attempt.device_id,
        &account.id,
        &account.username,
        &account.role,
    )
    .await
    {
        Ok(bound) => bound,
        Err(error) => {
            tracing::warn!(%error, "Remote SSO account binding failed");
            return sso_failed(
                &state,
                active,
                Some(&attempt),
                sso::SsoFailure::Account,
                500,
            )
            .await;
        }
    };
    if !bound {
        // The device was revoked between start and callback.
        return sso_failed(
            &state,
            active,
            Some(&attempt),
            sso::SsoFailure::Account,
            401,
        )
        .await;
    }
    state.cache_account(&attempt.device_id, &account);
    let device = RemoteAccessQueries::device_by_id(&state.pool, &attempt.device_id)
        .await
        .ok()
        .flatten();
    let role = device
        .as_ref()
        .map(|device| RemoteRole::parse(&device.role).capped_by(&account.role));
    let _ = RemoteAccessQueries::audit(
        &state.pool,
        Some(&attempt.device_id),
        device.as_ref().map(|device| device.name.as_str()),
        role.map(|role| role.as_str()),
        Some(&account.username),
        "SSO",
        "/api/sso/callback",
        200,
    )
    .await;
    sso_redirect(active, None)
}

async fn status(
    State(state): State<Arc<RemoteState>>,
    Extension(auth): Extension<AuthDevice>,
) -> ApiResult<Json<serde_json::Value>> {
    let bots: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bots")
        .fetch_one(&state.pool)
        .await
        .map_err(|error| ApiError::internal("count bots", error))?;
    let active_runs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM runs WHERE state NOT IN ('completed','failed','cancelled')",
    )
    .fetch_one(&state.pool)
    .await
    .map_err(|error| ApiError::internal("count runs", error))?;
    let pending_approvals: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM approvals WHERE status = 'pending'")
            .fetch_one(&state.pool)
            .await
            .map_err(|error| ApiError::internal("count approvals", error))?;
    let pending_questions: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM questions WHERE status = 'pending'")
            .fetch_one(&state.pool)
            .await
            .map_err(|error| ApiError::internal("count questions", error))?;
    let account = auth
        .account
        .as_ref()
        .map(|account| json!({ "id": account.id, "username": account.username }));
    Ok(Json(json!({
        "device": { "id": auth.id, "name": auth.name, "role": auth.role },
        "account": account,
        "login_required": auth.login_required,
        "sso_enabled": state.sso.is_some(),
        "bots": bots,
        "active_runs": active_runs,
        "pending_approvals": pending_approvals,
        "pending_questions": pending_questions,
    })))
}

async fn bots(State(state): State<Arc<RemoteState>>) -> ApiResult<Json<serde_json::Value>> {
    let bots = ravenbot_db::queries::BotQueries::list(&state.pool)
        .await
        .map_err(|error| ApiError::internal("list bots", error))?;
    Ok(Json(json!({
        "bots": bots.into_iter().map(|bot| json!({
            "id": bot.id,
            "name": bot.name,
            "description": bot.description,
            "avatar_url": bot.avatar_url,
        })).collect::<Vec<_>>()
    })))
}

#[derive(Debug, Deserialize)]
struct ThreadsQuery {
    bot_id: Uuid,
}

async fn threads(
    State(state): State<Arc<RemoteState>>,
    Query(query): Query<ThreadsQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let threads = ThreadQueries::list_visible_by_bot(&state.pool, query.bot_id)
        .await
        .map_err(|error| ApiError::internal("list threads", error))?;
    Ok(Json(json!({
        "threads": threads.into_iter().map(|thread| json!({
            "id": thread.id,
            "title": thread.title,
            "updated_at": thread.updated_at,
        })).collect::<Vec<_>>()
    })))
}

#[derive(Debug, Deserialize)]
struct MessagesQuery {
    limit: Option<usize>,
}

async fn messages(
    State(state): State<Arc<RemoteState>>,
    Path(thread_id): Path<Uuid>,
    Query(query): Query<MessagesQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    if ThreadQueries::get(&state.pool, thread_id)
        .await
        .map_err(|error| ApiError::internal("get thread", error))?
        .is_none()
    {
        return Err(ApiError::not_found("Conversation not found."));
    }
    let page = MessageQueries::list_page(
        &state.pool,
        thread_id,
        None,
        query.limit.unwrap_or(50).clamp(1, 100),
    )
    .await
    .map_err(|error| ApiError::internal("list messages", error))?;
    Ok(Json(json!({
        "messages": page.messages.iter().map(message_json).collect::<Vec<_>>(),
        "has_more": page.has_more,
    })))
}

fn message_json(message: &ravenbot_core::Message) -> serde_json::Value {
    let (text, tools) = match &message.content {
        ravenbot_core::MessageContent::Text { text, tools, .. } => (
            text.clone(),
            tools
                .iter()
                .map(|tool| {
                    json!({
                        "name": tool.name,
                        "ok": !tool.is_error,
                        "cost": tool.cost,
                    })
                })
                .collect::<Vec<_>>(),
        ),
        ravenbot_core::MessageContent::Checklist { text, items } => (
            format!(
                "{}{}",
                text.as_deref().unwrap_or_default(),
                items
                    .iter()
                    .map(|item| format!(
                        "\n{} {}",
                        match item.status {
                            ravenbot_core::ChecklistStatus::Completed => "✓",
                            ravenbot_core::ChecklistStatus::Failed => "✗",
                            _ => "○",
                        },
                        item.label
                    ))
                    .collect::<String>()
            ),
            Vec::new(),
        ),
        _ => (String::new(), Vec::new()),
    };
    json!({
        "id": message.id,
        "role": match message.role {
            ravenbot_core::MessageRole::User => "user",
            ravenbot_core::MessageRole::Assistant => "assistant",
            ravenbot_core::MessageRole::System => "system",
            ravenbot_core::MessageRole::Tool => "tool",
        },
        "text": text,
        "sender_name": message.sender_name,
        "reply_to_id": message.reply_to_id,
        "created_at": message.created_at,
        "tools": tools,
    })
}

#[derive(Debug, Deserialize)]
struct SendBody {
    content: String,
    reply_to_id: Option<Uuid>,
}

async fn send_message(
    State(state): State<Arc<RemoteState>>,
    Path(thread_id): Path<Uuid>,
    Json(body): Json<SendBody>,
) -> ApiResult<Response> {
    let content = body.content.trim().to_string();
    if content.is_empty() || content.chars().count() > MAX_MESSAGE_CHARS {
        return Err(ApiError::bad(format!(
            "A message must be 1–{MAX_MESSAGE_CHARS} characters."
        )));
    }
    let thread = ThreadQueries::get(&state.pool, thread_id)
        .await
        .map_err(|error| ApiError::internal("get thread", error))?
        .ok_or_else(|| ApiError::not_found("Conversation not found."))?;
    let active: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM runs WHERE thread_id = ? AND state NOT IN ('completed','failed','cancelled')",
    )
    .bind(thread_id.to_string())
    .fetch_one(&state.pool)
    .await
    .map_err(|error| ApiError::internal("check active run", error))?;
    if active > 0 {
        return Err(ApiError::conflict(
            "This conversation is already running. Wait for it to finish or stop it first.",
        ));
    }
    let mut message = ravenbot_core::Message::user(thread_id, &content);
    if let Some(reply_to_id) = body.reply_to_id {
        let exists: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE id = ? AND thread_id = ?")
                .bind(reply_to_id.to_string())
                .bind(thread_id.to_string())
                .fetch_one(&state.pool)
                .await
                .map_err(|error| ApiError::internal("check reply target", error))?;
        if exists == 0 {
            return Err(ApiError::bad(
                "The quoted message is not part of this conversation.",
            ));
        }
        message.reply_to_id = Some(reply_to_id);
    }
    MessageQueries::insert(&state.pool, &message)
        .await
        .map_err(|error| ApiError::internal("insert message", error))?;

    let mut run = ravenbot_core::Run::new(thread.bot_id, thread_id);
    RunQueries::insert(&state.pool, &run)
        .await
        .map_err(|error| ApiError::internal("insert run", error))?;
    let run_id = run.id;
    let runtime = state.runtime.clone();
    tokio::spawn(async move {
        if let Err(error) = runtime.execute_run(&mut run).await {
            tracing::warn!(run_id = %run_id, %error, "remote-triggered run failed");
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "run_id": run_id,
            "message_id": message.id,
        })),
    )
        .into_response())
}

async fn attention(State(state): State<Arc<RemoteState>>) -> ApiResult<Json<serde_json::Value>> {
    let approvals = ApprovalQueries::list_pending_all(&state.pool, 50)
        .await
        .map_err(|error| ApiError::internal("list approvals", error))?;
    let questions = QuestionQueries::list_pending_all(&state.pool, 50)
        .await
        .map_err(|error| ApiError::internal("list questions", error))?;
    Ok(Json(json!({
        "approvals": approvals.iter().map(approval_json).collect::<Vec<_>>(),
        "questions": questions.iter().map(question_json).collect::<Vec<_>>(),
    })))
}

fn approval_json(approval: &ravenbot_core::ApprovalRequest) -> serde_json::Value {
    json!({
        "id": approval.id,
        "thread_id": approval.thread_id,
        "run_id": approval.run_id,
        "tool_name": approval.tool_name,
        "tool_label": approval.tool_label,
        "risk": approval.risk,
        "arguments": approval.arguments,
        "created_at": approval.created_at,
    })
}

fn question_json(question: &ravenbot_core::QuestionRequest) -> serde_json::Value {
    json!({
        "id": question.id,
        "thread_id": question.thread_id,
        "run_id": question.run_id,
        "header": question.header,
        "question": question.question,
        "options": question.options,
        "allow_custom": question.allow_custom,
        "kind": question.kind.as_str(),
        "payload": question.payload,
        "created_at": question.created_at,
    })
}

async fn approvals(State(state): State<Arc<RemoteState>>) -> ApiResult<Json<serde_json::Value>> {
    let approvals = ApprovalQueries::list_pending_all(&state.pool, 100)
        .await
        .map_err(|error| ApiError::internal("list approvals", error))?;
    Ok(Json(json!({
        "approvals": approvals.iter().map(approval_json).collect::<Vec<_>>()
    })))
}

#[derive(Debug, Deserialize)]
struct DecideBody {
    allow: bool,
    #[serde(default)]
    note: Option<String>,
}

async fn decide_approval(
    State(state): State<Arc<RemoteState>>,
    Path(approval_id): Path<Uuid>,
    Json(body): Json<DecideBody>,
) -> ApiResult<Json<serde_json::Value>> {
    let request = ApprovalQueries::get(&state.pool, approval_id)
        .await
        .map_err(|error| ApiError::internal("load approval", error))?;
    let decided =
        ApprovalQueries::decide(&state.pool, approval_id, body.allow, body.note.as_deref())
            .await
            .map_err(|error| ApiError::internal("decide approval", error))?;
    if !decided {
        return Err(ApiError::conflict("That approval is no longer pending."));
    }
    if let Some(request) = request {
        state
            .runtime
            .audit_approval_decision(&request, body.allow, "remote", body.note.as_deref())
            .await;
    }
    Ok(Json(json!({ "ok": true, "allowed": body.allow })))
}

async fn questions(State(state): State<Arc<RemoteState>>) -> ApiResult<Json<serde_json::Value>> {
    let questions = QuestionQueries::list_pending_all(&state.pool, 100)
        .await
        .map_err(|error| ApiError::internal("list questions", error))?;
    Ok(Json(json!({
        "questions": questions.iter().map(question_json).collect::<Vec<_>>()
    })))
}

#[derive(Debug, Deserialize)]
struct AnswerBody {
    answer: String,
}

async fn answer_question(
    State(state): State<Arc<RemoteState>>,
    Path(question_id): Path<Uuid>,
    Json(body): Json<AnswerBody>,
) -> ApiResult<Json<serde_json::Value>> {
    let answer = body.answer.trim();
    if answer.is_empty() || answer.len() > MAX_ANSWER_BYTES {
        return Err(ApiError::bad("An answer must be 1–64 KB."));
    }
    let answered = QuestionQueries::answer(&state.pool, question_id, answer)
        .await
        .map_err(|error| ApiError::internal("answer question", error))?;
    if !answered {
        return Err(ApiError::conflict("That question is no longer pending."));
    }
    Ok(Json(json!({ "ok": true })))
}

async fn run_status(
    State(state): State<Arc<RemoteState>>,
    Path(run_id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let run = RunQueries::get(&state.pool, run_id)
        .await
        .map_err(|error| ApiError::internal("get run", error))?
        .ok_or_else(|| ApiError::not_found("Run not found."))?;
    Ok(Json(json!({
        "id": run.id,
        "state": format!("{:?}", run.state).to_lowercase(),
        "outcome": run.outcome,
        "tokens": run.tokens_consumed,
        "cost": run.cost_estimate,
    })))
}

async fn cancel_run(
    State(state): State<Arc<RemoteState>>,
    Path(run_id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let run = RunQueries::get(&state.pool, run_id)
        .await
        .map_err(|error| ApiError::internal("get run", error))?
        .ok_or_else(|| ApiError::not_found("Run not found."))?;
    if matches!(
        run.state,
        ravenbot_core::RunState::Completed
            | ravenbot_core::RunState::Failed
            | ravenbot_core::RunState::Cancelled
    ) {
        return Err(ApiError::conflict("That run has already finished."));
    }
    state.runtime.request_cancel(run_id);
    Ok(Json(json!({ "ok": true, "requested": true })))
}

/// A device can always revoke itself, regardless of role. The mobile page
/// calls this when the user taps "Forget device"; clearing localStorage alone
/// would leave the token valid on the host.
async fn revoke_self(
    State(state): State<Arc<RemoteState>>,
    Extension(auth): Extension<AuthDevice>,
) -> ApiResult<Json<serde_json::Value>> {
    let id = Uuid::parse_str(&auth.id).map_err(|_| ApiError::unauthorized())?;
    RemoteAccessQueries::revoke_device(&state.pool, id)
        .await
        .map_err(|error| ApiError::internal("revoke device", error))?;
    state.clear_cached_account(&auth.id);
    Ok(Json(json!({ "ok": true })))
}

async fn remote_audit(
    State(state): State<Arc<RemoteState>>,
    Extension(auth): Extension<AuthDevice>,
) -> ApiResult<Json<serde_json::Value>> {
    if !auth.role.can_admin() {
        return Err(ApiError::forbidden());
    }
    let entries = RemoteAccessQueries::recent_audit(&state.pool, 100)
        .await
        .map_err(|error| ApiError::internal("remote audit", error))?;
    Ok(Json(json!({ "entries": entries })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request as HttpRequest;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    async fn state() -> Arc<RemoteState> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        ravenbot_db::migrations::run(&pool)
            .await
            .expect("migrations");
        let db = ravenbot_db::Database::from_pool(pool.clone());
        Arc::new(RemoteState::new(pool, Arc::new(Runtime::new(db))))
    }

    /// A deterministic stand-in for the account registry. `None` simulates a
    /// disabled, deleted or grant-revoked account.
    struct StubAuthenticator {
        password: String,
        account: Mutex<Option<RemoteAccount>>,
        fail_revalidation: Mutex<bool>,
        revalidations: std::sync::atomic::AtomicUsize,
    }

    impl StubAuthenticator {
        fn new(account: RemoteAccount, password: &str) -> Self {
            Self {
                password: password.to_string(),
                account: Mutex::new(Some(account)),
                fail_revalidation: Mutex::new(false),
                revalidations: std::sync::atomic::AtomicUsize::new(0),
            }
        }

        fn set_account(&self, account: Option<RemoteAccount>) {
            *self.account.lock().expect("account lock") = account;
        }

        fn fail_revalidation(&self) {
            *self.fail_revalidation.lock().expect("revalidation lock") = true;
        }

        fn revalidations(&self) -> usize {
            self.revalidations.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl AccountAuthenticator for StubAuthenticator {
        async fn authenticate(
            &self,
            username: &str,
            password: &str,
        ) -> Result<Option<RemoteAccount>, String> {
            let account = self.account.lock().expect("account lock").clone();
            Ok(account.filter(|account| {
                account.username.eq_ignore_ascii_case(username.trim()) && password == self.password
            }))
        }

        async fn revalidate(&self, account_id: &str) -> Result<Option<RemoteAccount>, String> {
            self.revalidations
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if *self.fail_revalidation.lock().expect("revalidation lock") {
                return Err("registry unavailable".to_string());
            }
            Ok(self
                .account
                .lock()
                .expect("account lock")
                .clone()
                .filter(|account| account.id == account_id))
        }

        async fn account_by_identity(
            &self,
            identity: &str,
        ) -> Result<Option<RemoteAccount>, String> {
            Ok(self
                .account
                .lock()
                .expect("account lock")
                .clone()
                .filter(|account| account.username.eq_ignore_ascii_case(identity.trim())))
        }
    }

    /// A workspace-scoped SCIM provisioner stand-in. The real implementation
    /// lives in the desktop crate; these tests cover HTTP shape, auth, filters
    /// and audit, not the account registry.
    struct StubProvisioner {
        owner: String,
        users: Mutex<Vec<ScimUser>>,
        next_id: std::sync::atomic::AtomicUsize,
    }

    impl StubProvisioner {
        fn new(owner: &str) -> Self {
            Self {
                owner: owner.to_string(),
                users: Mutex::new(Vec::new()),
                next_id: std::sync::atomic::AtomicUsize::new(1),
            }
        }

        fn users(&self) -> std::sync::MutexGuard<'_, Vec<ScimUser>> {
            self.users.lock().expect("users lock")
        }

        fn next_id(&self) -> String {
            format!(
                "scim-{}",
                self.next_id
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            )
        }
    }

    #[async_trait::async_trait]
    impl AccountProvisioner for StubProvisioner {
        async fn list_users(&self) -> Result<Vec<ScimUser>, String> {
            Ok(self.users().clone())
        }

        async fn get_user(&self, id: &str) -> Result<Option<ScimUser>, String> {
            Ok(self.users().iter().find(|user| user.id == id).cloned())
        }

        async fn find_user(
            &self,
            field: ScimFilterField,
            value: &str,
        ) -> Result<Option<ScimUser>, String> {
            Ok(self
                .users()
                .iter()
                .find(|user| match field {
                    ScimFilterField::Id => user.id == value,
                    ScimFilterField::UserName => user.user_name.eq_ignore_ascii_case(value),
                    ScimFilterField::ExternalId => user.external_id.as_deref() == Some(value),
                })
                .cloned())
        }

        async fn create_user(
            &self,
            user_name: &str,
            external_id: Option<&str>,
            active: bool,
            _role: &str,
        ) -> Result<ScimUser, ProvisionError> {
            if user_name.eq_ignore_ascii_case(&self.owner) {
                return Err(ProvisionError::Forbidden("owner".to_string()));
            }
            let mut users = self.users();
            if users
                .iter()
                .any(|user| user.user_name.eq_ignore_ascii_case(user_name))
                || external_id.is_some_and(|external_id| {
                    users
                        .iter()
                        .any(|user| user.external_id.as_deref() == Some(external_id))
                })
            {
                return Err(ProvisionError::Conflict("duplicate".to_string()));
            }
            let user = ScimUser {
                id: self.next_id(),
                user_name: user_name.to_string(),
                external_id: external_id.map(str::to_string),
                active,
                created_at: "2026-01-01T00:00:00Z".to_string(),
            };
            users.push(user.clone());
            Ok(user)
        }

        async fn set_active(&self, id: &str, active: bool) -> Result<ScimUser, ProvisionError> {
            if id == "owner-id" {
                return Err(ProvisionError::Forbidden("owner".to_string()));
            }
            let mut users = self.users();
            let user = users
                .iter_mut()
                .find(|user| user.id == id)
                .ok_or_else(|| ProvisionError::Invalid("missing".to_string()))?;
            user.active = active;
            Ok(user.clone())
        }

        async fn rename(&self, id: &str, user_name: &str) -> Result<ScimUser, ProvisionError> {
            if user_name.eq_ignore_ascii_case(&self.owner) {
                return Err(ProvisionError::Forbidden("owner".to_string()));
            }
            let mut users = self.users();
            if users
                .iter()
                .any(|user| user.id != id && user.user_name.eq_ignore_ascii_case(user_name))
            {
                return Err(ProvisionError::Conflict("duplicate".to_string()));
            }
            let user = users
                .iter_mut()
                .find(|user| user.id == id)
                .ok_or_else(|| ProvisionError::Invalid("missing".to_string()))?;
            user.user_name = user_name.to_string();
            Ok(user.clone())
        }
    }

    async fn scim_state() -> (Arc<RemoteState>, Arc<StubProvisioner>, String) {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        ravenbot_db::migrations::run(&pool)
            .await
            .expect("migrations");
        let db = ravenbot_db::Database::from_pool(pool.clone());
        let token = generate_scim_token();
        let config = ScimConfig::new(&hash_secret(&token), "viewer").expect("scim config");
        let provisioner = Arc::new(StubProvisioner::new("owner"));
        let state = Arc::new(RemoteState::new_with_services(
            pool,
            Arc::new(Runtime::new(db)),
            None,
            None,
            Some(config),
            Some(provisioner.clone()),
        ));
        (state, provisioner, token)
    }

    fn scim_request(
        method: &str,
        uri: &str,
        token: Option<&str>,
        body: Option<serde_json::Value>,
    ) -> HttpRequest<Body> {
        let mut request = HttpRequest::builder()
            .method(method)
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/scim+json");
        if let Some(token) = token {
            request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        let body = match body {
            Some(body) => Body::from(body.to_string()),
            None => Body::empty(),
        };
        let mut request = request.body(body).unwrap();
        request
            .extensions_mut()
            .insert(ConnectInfo(PeerAddr(SocketAddr::from((
                [127, 0, 0, 1],
                52000,
            )))));
        request
    }

    async fn state_with_authenticator(
        authenticator: Arc<dyn AccountAuthenticator>,
    ) -> Arc<RemoteState> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        ravenbot_db::migrations::run(&pool)
            .await
            .expect("migrations");
        let db = ravenbot_db::Database::from_pool(pool.clone());
        Arc::new(RemoteState::new_with_authenticator(
            pool,
            Arc::new(Runtime::new(db)),
            Some(authenticator),
            None,
        ))
    }

    async fn test_device(state: &RemoteState, name: &str, role: &str) -> String {
        let token = generate_token();
        RemoteAccessQueries::create_device(&state.pool, name, role, &hash_secret(&token))
            .await
            .expect("device");
        token
    }

    fn get_with_token(uri: &str, token: &str) -> HttpRequest<Body> {
        HttpRequest::builder()
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap()
    }

    fn login_request(token: &str, username: &str, password: &str) -> HttpRequest<Body> {
        let mut request = HttpRequest::builder()
            .method("POST")
            .uri("/api/login")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::from(
                json!({ "username": username, "password": password }).to_string(),
            ))
            .unwrap();
        request
            .extensions_mut()
            .insert(ConnectInfo(PeerAddr(SocketAddr::from((
                [127, 0, 0, 1],
                51000,
            )))));
        request
    }

    async fn body_json(response: Response) -> serde_json::Value {
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        serde_json::from_slice(&bytes).expect("json")
    }

    async fn seeded_thread(state: &RemoteState) -> (Uuid, Uuid) {
        let bot = ravenbot_core::Bot::new("Share bot", "public share test");
        ravenbot_db::queries::BotQueries::insert(&state.pool, &bot)
            .await
            .expect("bot");
        let thread = ravenbot_core::Thread::new(bot.id, "Quarterly plan <draft>");
        ThreadQueries::create(&state.pool, &thread)
            .await
            .expect("thread");
        let first = ravenbot_core::Message::user(thread.id, "Hello <script>alert(1)</script>");
        MessageQueries::insert(&state.pool, &first)
            .await
            .expect("user message");
        let mut answer = ravenbot_core::Message::assistant(thread.id, "The plan is ready.");
        answer.sender_name = Some("Raven".to_string());
        MessageQueries::insert(&state.pool, &answer)
            .await
            .expect("assistant message");
        (bot.id, thread.id)
    }

    async fn share_app() -> (Arc<RemoteState>, String, Uuid, Uuid) {
        let state = state().await;
        let (_, thread_id) = seeded_thread(&state).await;
        let mut tool = ravenbot_core::Message::assistant(thread_id, "internal trace");
        tool.content = ravenbot_core::MessageContent::ToolCall {
            tool_name: "secret-tool".to_string(),
            arguments: json!({"token": "tool-secret"}),
        };
        MessageQueries::insert(&state.pool, &tool)
            .await
            .expect("tool message");
        let token = generate_token();
        let expires = (Utc::now() + chrono::Duration::hours(2)).to_rfc3339();
        let share =
            ThreadShareQueries::create(&state.pool, thread_id, &hash_secret(&token), &expires)
                .await
                .expect("share");
        (state, token, share.id, thread_id)
    }

    #[tokio::test]
    async fn a_public_share_renders_only_the_snapshot_and_escapes_content() {
        let (state, token, share_id, _) = share_app().await;
        let app = router(state.clone());
        let response = app
            .oneshot(
                HttpRequest::builder()
                    .method("GET")
                    .uri(format!("/share/{token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("share response");
        assert_eq!(response.status(), StatusCode::OK);
        assert!(response
            .headers()
            .get(header::HeaderName::from_static("x-robots-tag"))
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.contains("noindex")));
        let body = body_text(response).await;
        assert!(body.contains("Quarterly plan &lt;draft&gt;"), "{body}");
        assert!(body.contains("Hello &lt;script&gt;"), "{body}");
        assert!(body.contains("The plan is ready."), "{body}");
        assert!(!body.contains("<script>alert"), "{body}");
        assert!(!body.contains("tool-secret"), "{body}");
        assert!(body.contains("Read-only"), "{body}");

        let stored: (i64, String) =
            sqlx::query_as("SELECT view_count, token_hash FROM thread_shares WHERE id = ?")
                .bind(share_id.to_string())
                .fetch_one(&state.pool)
                .await
                .expect("view count");
        assert_eq!(stored.0, 1);
        assert_ne!(stored.1, token, "the usable token is never persisted");
        assert_eq!(stored.1, hash_secret(&token));
    }

    #[tokio::test]
    async fn future_messages_and_tool_content_never_enter_a_share() {
        let (state, token, _, thread_id) = share_app().await;
        let app = router(state.clone());
        // The thread was seeded before the share; append after the snapshot
        // boundary and confirm it is absent.
        let mut future = ravenbot_core::Message::assistant(thread_id, "SECRET-FUTURE");
        future.created_at = Utc::now() + chrono::Duration::hours(1);
        MessageQueries::insert(&state.pool, &future)
            .await
            .expect("future message");
        let body = body_text(
            app.oneshot(
                HttpRequest::builder()
                    .method("GET")
                    .uri(format!("/share/{token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("share"),
        )
        .await;
        assert!(!body.contains("SECRET-FUTURE"), "{body}");
        assert!(!body.contains("tool-secret"), "{body}");
        assert!(!body.contains("secret-tool"), "{body}");
    }

    #[tokio::test]
    async fn expired_revoked_and_unknown_share_tokens_are_404() {
        let state = state().await;
        let app = router(state.clone());
        let missing = app
            .clone()
            .oneshot(
                HttpRequest::builder()
                    .method("GET")
                    .uri("/share/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("unknown");
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);

        let (_, thread_id) = seeded_thread(&state).await;
        let expired_token = generate_token();
        ThreadShareQueries::create(
            &state.pool,
            thread_id,
            &hash_secret(&expired_token),
            &(Utc::now() - chrono::Duration::minutes(1)).to_rfc3339(),
        )
        .await
        .expect("expired share");
        let expired = app
            .clone()
            .oneshot(
                HttpRequest::builder()
                    .method("GET")
                    .uri(format!("/share/{expired_token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("expired");
        assert_eq!(expired.status(), StatusCode::NOT_FOUND);

        let active_token = generate_token();
        let active = ThreadShareQueries::create(
            &state.pool,
            thread_id,
            &hash_secret(&active_token),
            &(Utc::now() + chrono::Duration::hours(1)).to_rfc3339(),
        )
        .await
        .expect("active share");
        ThreadShareQueries::revoke(&state.pool, active.id)
            .await
            .expect("revoke");
        let revoked = app
            .oneshot(
                HttpRequest::builder()
                    .method("GET")
                    .uri(format!("/share/{active_token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("revoked");
        assert_eq!(revoked.status(), StatusCode::NOT_FOUND);
    }

    async fn body_text(response: Response) -> String {
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        String::from_utf8(bytes.to_vec()).expect("utf8")
    }

    #[test]
    fn scim_config_accepts_only_a_full_hash_and_viewer_or_operator() {
        let hash = hash_secret(&generate_scim_token());
        assert_eq!(generate_scim_token().len(), 64);
        assert_eq!(
            ScimConfig::new(&hash, "OPERATOR")
                .expect("valid")
                .default_role,
            "operator"
        );
        assert_eq!(
            ScimConfig::new(&hash, "")
                .expect("default role")
                .default_role,
            "viewer"
        );
        assert!(ScimConfig::new("short", "viewer").is_err());
        assert!(ScimConfig::new(&hash, "owner").is_err());
        assert!(ScimConfig::new(&hash, "admin").is_err());
    }

    #[test]
    fn roles_are_ordered_by_capability() {
        assert!(!RemoteRole::Viewer.can_write());
        assert!(RemoteRole::Operator.can_write());
        assert!(!RemoteRole::Operator.can_admin());
        assert!(RemoteRole::Admin.can_write());
        assert!(RemoteRole::Admin.can_admin());
        assert_eq!(RemoteRole::parse("ADMIN"), RemoteRole::Admin);
        assert_eq!(RemoteRole::parse("nonsense"), RemoteRole::Viewer);
        assert_eq!(RemoteRole::from_workspace("owner"), RemoteRole::Admin);
        assert_eq!(RemoteRole::from_workspace("operator"), RemoteRole::Operator);
        assert_eq!(RemoteRole::from_workspace("viewer"), RemoteRole::Viewer);
        assert_eq!(RemoteRole::from_workspace("root"), RemoteRole::Viewer);
        assert_eq!(RemoteRole::Admin.capped_by("viewer"), RemoteRole::Viewer);
        assert_eq!(
            RemoteRole::Admin.capped_by("operator"),
            RemoteRole::Operator
        );
        assert_eq!(RemoteRole::Viewer.capped_by("owner"), RemoteRole::Viewer);
        assert_eq!(
            RemoteRole::Operator.capped_by("owner"),
            RemoteRole::Operator
        );
        assert_eq!(RemoteRole::Operator.capped_by("viewer"), RemoteRole::Viewer);
    }

    #[tokio::test]
    async fn pairing_and_sign_in_rate_limits_use_separate_counters() {
        let state = state().await;
        let login_ip = IpAddr::from([127, 0, 0, 2]);
        for _ in 0..MAX_LOGIN_ATTEMPTS_PER_MINUTE {
            assert!(state.login_allowed(login_ip));
        }
        assert!(!state.login_allowed(login_ip));
        assert!(
            state.pairing_allowed(login_ip),
            "a sign-in flood must not consume pairing attempts"
        );

        let pairing_ip = IpAddr::from([127, 0, 0, 3]);
        for _ in 0..MAX_PAIRING_ATTEMPTS_PER_MINUTE {
            assert!(state.pairing_allowed(pairing_ip));
        }
        assert!(!state.pairing_allowed(pairing_ip));
        assert!(
            state.login_allowed(pairing_ip),
            "a pairing flood must not lock out sign-in"
        );
    }

    #[test]
    fn pairing_codes_are_six_digits_and_tokens_are_high_entropy() {
        let code = generate_pairing_code();
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|ch| ch.is_ascii_digit()));
        let token = generate_token();
        assert_eq!(token.len(), 64);
        assert!(token.chars().all(|ch| ch.is_ascii_hexdigit()));
        assert_ne!(token, generate_token());
        assert_eq!(hash_secret("abc").len(), 64);
    }

    #[tokio::test]
    async fn a_pairing_code_creates_one_device_and_then_fails() {
        let state = state().await;
        let pairing = create_pairing(&state.pool, RemoteRole::Operator, Duration::from_secs(120))
            .await
            .expect("pairing");
        let app = router(state.clone());
        let mut request = HttpRequest::builder()
            .method("POST")
            .uri("/api/pair")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({ "code": pairing.code, "device_name": "Test phone" }).to_string(),
            ))
            .unwrap();
        request
            .extensions_mut()
            .insert(ConnectInfo(PeerAddr(SocketAddr::from((
                [127, 0, 0, 1],
                50000,
            )))));
        let response = app.clone().oneshot(request).await.expect("pair response");
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_json(response).await;
        assert_eq!(body["role"], "operator");
        assert_eq!(body["account_login"], false);
        let token = body["token"].as_str().expect("token");
        assert_eq!(token.len(), 64);
        assert!(body["device_id"].as_str().is_some());

        // The database stores only the hash, never the usable token.
        let stored: String = sqlx::query_scalar("SELECT token_hash FROM remote_devices LIMIT 1")
            .fetch_one(&state.pool)
            .await
            .expect("stored token hash");
        assert_ne!(stored, token);
        assert_eq!(stored, hash_secret(token));

        let mut replay = HttpRequest::builder()
            .method("POST")
            .uri("/api/pair")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({ "code": pairing.code, "device_name": "Second phone" }).to_string(),
            ))
            .unwrap();
        replay
            .extensions_mut()
            .insert(ConnectInfo(PeerAddr(SocketAddr::from((
                [127, 0, 0, 1],
                50000,
            )))));
        let response = app.oneshot(replay).await.expect("replay response");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn pairing_attempts_are_rate_limited_per_ip() {
        let state = state().await;
        let app = router(state);
        for attempt in 0..5 {
            let mut request = HttpRequest::builder()
                .method("POST")
                .uri("/api/pair")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "code": "000000", "device_name": format!("Phone {attempt}") })
                        .to_string(),
                ))
                .unwrap();
            request
                .extensions_mut()
                .insert(ConnectInfo(PeerAddr(SocketAddr::from((
                    [127, 0, 0, 1],
                    50100,
                )))));
            let response = app.clone().oneshot(request).await.expect("response");
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }
        let mut request = HttpRequest::builder()
            .method("POST")
            .uri("/api/pair")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({ "code": "000000", "device_name": "Phone 6" }).to_string(),
            ))
            .unwrap();
        request
            .extensions_mut()
            .insert(ConnectInfo(PeerAddr(SocketAddr::from((
                [127, 0, 0, 1],
                50100,
            )))));
        let response = app.oneshot(request).await.expect("rate-limited response");
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn protected_routes_require_a_token() {
        let state = state().await;
        let app = router(state);
        let response = app
            .clone()
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let response = app
            .oneshot(
                HttpRequest::builder()
                    .method("POST")
                    .uri("/api/sso/start")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("sso start response");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn self_signed_certificates_are_stable_and_fingerprinted() {
        let dir = std::env::temp_dir().join(format!("ravenbot-remote-tls-{}", Uuid::new_v4()));
        let (cert, key, fingerprint) = ensure_self_signed(&dir, false).expect("cert");
        let (cert_again, key_again, fingerprint_again) =
            ensure_self_signed(&dir, false).expect("cert again");
        assert_eq!(
            cert, cert_again,
            "the certificate must not rotate on restart"
        );
        assert_eq!(key, key_again);
        assert_eq!(fingerprint, fingerprint_again);
        assert_eq!(fingerprint.len(), 64);
        assert!(fingerprint.chars().all(|ch| ch.is_ascii_hexdigit()));
        assert!(std::fs::read_to_string(dir.join("sans.txt"))
            .expect("sans")
            .contains("127.0.0.1"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn the_tls_listener_serves_https() {
        let state = state().await;
        let dir = std::env::temp_dir().join(format!("ravenbot-remote-tls-{}", Uuid::new_v4()));
        let server = start(
            state.pool.clone(),
            state.runtime.clone(),
            RemoteConfig {
                port: 0,
                allow_lan: false,
                tls: true,
                tls_dir: Some(dir.clone()),
                sso: None,
                scim: None,
            },
        )
        .await
        .expect("tls listener");
        assert!(server.url().starts_with("https://"));
        assert!(server.tls());
        assert_eq!(server.fingerprint().map(str::len), Some(64));

        // The test trusts the transport, not a public CA. A browser shows the
        // standard self-signed warning and the user accepts it.
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .expect("client");
        let health = client
            .get(format!("{}/api/health", server.url()))
            .send()
            .await
            .expect("https health");
        assert_eq!(health.status(), 200);
        assert_eq!(
            health.json::<serde_json::Value>().await.unwrap()["ok"],
            true
        );
        server.shutdown().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn the_real_listener_serves_health_and_the_mobile_page() {
        let state = state().await;
        let server = start(
            state.pool.clone(),
            state.runtime.clone(),
            RemoteConfig {
                port: 0,
                allow_lan: false,
                tls: false,
                tls_dir: None,
                sso: None,
                scim: None,
            },
        )
        .await
        .expect("listener");
        let base = format!("http://127.0.0.1:{}", server.addr().port());
        let client = reqwest::Client::new();
        let health = client
            .get(format!("{base}/api/health"))
            .send()
            .await
            .expect("health request");
        assert_eq!(health.status(), 200);
        assert_eq!(
            health.json::<serde_json::Value>().await.unwrap()["ok"],
            true
        );

        let page = client
            .get(format!("{base}/"))
            .send()
            .await
            .expect("page request");
        assert_eq!(page.status(), 200);
        assert!(page
            .headers()
            .get("content-security-policy")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.contains("default-src 'self'")));
        let page = page.text().await.unwrap();
        assert!(page.contains("Pair this device"));
        assert!(page.contains("Sign in with SSO"));
        assert!(page.contains("login-username") && page.contains("login-password"));

        // A loopback listener is not reachable through a LAN bind assumption;
        // the URL still names the actual port.
        assert_eq!(
            server.url(),
            format!("http://127.0.0.1:{}", server.addr().port())
        );
        server.shutdown().await;
    }

    #[tokio::test]
    async fn a_viewer_reads_conversations_but_cannot_read_the_audit() {
        let state = state().await;
        let bot = ravenbot_core::Bot::new("Billing", "invoices");
        ravenbot_db::queries::BotQueries::insert(&state.pool, &bot)
            .await
            .expect("bot");
        let thread = ravenbot_core::Thread::new(bot.id, "Invoices");
        ThreadQueries::create(&state.pool, &thread)
            .await
            .expect("thread");
        let message = ravenbot_core::Message::user(thread.id, "hello from the host");
        MessageQueries::insert(&state.pool, &message)
            .await
            .expect("message");

        let token = generate_token();
        RemoteAccessQueries::create_device(
            &state.pool,
            "Viewer phone",
            "viewer",
            &hash_secret(&token),
        )
        .await
        .expect("device");
        let app = router(state.clone());
        let get = |uri: &str| {
            HttpRequest::builder()
                .uri(uri)
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap()
        };

        let response = app.clone().oneshot(get("/api/bots")).await.expect("bots");
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_json(response).await;
        assert_eq!(body["bots"][0]["name"], "Billing");

        let response = app
            .clone()
            .oneshot(get(&format!("/api/threads?bot_id={}", bot.id)))
            .await
            .expect("threads");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body_json(response).await["threads"][0]["title"], "Invoices");

        let response = app
            .clone()
            .oneshot(get(&format!("/api/threads/{}/messages", thread.id)))
            .await
            .expect("messages");
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_json(response).await;
        assert_eq!(body["messages"][0]["text"], "hello from the host");

        // The audit is admin-only, even for a valid viewer token.
        let response = app
            .clone()
            .oneshot(get("/api/remote/audit"))
            .await
            .expect("audit");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        let admin = generate_token();
        RemoteAccessQueries::create_device(
            &state.pool,
            "Admin phone",
            "admin",
            &hash_secret(&admin),
        )
        .await
        .expect("admin");
        let response = router(state)
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/remote/audit")
                    .header(header::AUTHORIZATION, format!("Bearer {admin}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("admin audit");
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn a_device_can_revoke_itself_and_its_token_stops_working() {
        let state = state().await;
        let token = generate_token();
        RemoteAccessQueries::create_device(
            &state.pool,
            "Lost phone",
            "viewer",
            &hash_secret(&token),
        )
        .await
        .expect("device");
        let app = router(state.clone());
        let revoke = HttpRequest::builder()
            .method("POST")
            .uri("/api/device/revoke")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(revoke).await.expect("revoke");
        assert_eq!(response.status(), StatusCode::OK);

        let after = HttpRequest::builder()
            .uri("/api/status")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(after).await.expect("after revoke");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn an_operator_can_send_a_message_and_start_a_run() {
        let state = state().await;
        let bot = ravenbot_core::Bot::new("Remote", "remote send");
        ravenbot_db::queries::BotQueries::insert(&state.pool, &bot)
            .await
            .expect("bot");
        let thread = ravenbot_core::Thread::new(bot.id, "Remote thread");
        ThreadQueries::create(&state.pool, &thread)
            .await
            .expect("thread");
        let token = generate_token();
        RemoteAccessQueries::create_device(
            &state.pool,
            "Operator phone",
            "operator",
            &hash_secret(&token),
        )
        .await
        .expect("device");

        let request = HttpRequest::builder()
            .method("POST")
            .uri(format!("/api/threads/{}/messages", thread.id))
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::from(
                json!({ "content": "hello from the phone", "reply_to_id": null }).to_string(),
            ))
            .unwrap();
        let response = router(state.clone())
            .oneshot(request)
            .await
            .expect("send response");
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = body_json(response).await;
        let run_id = Uuid::parse_str(body["run_id"].as_str().expect("run id")).expect("uuid");

        let messages = MessageQueries::list_by_thread(&state.pool, thread.id)
            .await
            .expect("messages");
        assert!(messages.iter().any(|message| matches!(
            &message.content,
            ravenbot_core::MessageContent::Text { text, .. } if text == "hello from the phone"
        )));
        assert!(RunQueries::get(&state.pool, run_id)
            .await
            .expect("run lookup")
            .is_some());
    }

    #[tokio::test]
    async fn a_viewer_cannot_post_and_an_operator_can() {
        let state = state().await;
        let app = router(state.clone());
        let token = generate_token();
        RemoteAccessQueries::create_device(
            &state.pool,
            "Viewer phone",
            "viewer",
            &hash_secret(&token),
        )
        .await
        .expect("device");

        let body = json!({ "content": "hello", "reply_to_id": null }).to_string();
        let request = HttpRequest::builder()
            .method("POST")
            .uri(format!("/api/threads/{}/messages", Uuid::new_v4()))
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::from(body.clone()))
            .unwrap();
        let response = app.oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        // The same call with an operator token reaches the handler, which
        // correctly reports the missing thread rather than denying the role.
        let app = router(state.clone());
        let operator = generate_token();
        RemoteAccessQueries::create_device(
            &state.pool,
            "Operator phone",
            "operator",
            &hash_secret(&operator),
        )
        .await
        .expect("device");
        let request = HttpRequest::builder()
            .method("POST")
            .uri(format!("/api/threads/{}/messages", Uuid::new_v4()))
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, format!("Bearer {operator}"))
            .body(Body::from(body))
            .unwrap();
        let response = app.oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn a_paired_device_must_sign_in_before_using_the_api() {
        let stub = Arc::new(StubAuthenticator::new(
            RemoteAccount {
                id: "account-alice".to_string(),
                username: "alice".to_string(),
                role: "owner".to_string(),
            },
            "correct horse battery",
        ));
        let state = state_with_authenticator(stub).await;
        let app = router(state.clone());

        let pairing = create_pairing(&state.pool, RemoteRole::Admin, Duration::from_secs(120))
            .await
            .expect("pairing");
        let mut request = HttpRequest::builder()
            .method("POST")
            .uri("/api/pair")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({ "code": pairing.code, "device_name": "Alice phone" }).to_string(),
            ))
            .unwrap();
        request
            .extensions_mut()
            .insert(ConnectInfo(PeerAddr(SocketAddr::from((
                [127, 0, 0, 1],
                51001,
            )))));
        let response = app.clone().oneshot(request).await.expect("pair response");
        assert_eq!(response.status(), StatusCode::OK);
        let paired = body_json(response).await;
        assert_eq!(paired["account_login"], true);
        let token = paired["token"].as_str().expect("token").to_string();

        // The device token alone is not enough while sign-in is enabled.
        let response = app
            .clone()
            .oneshot(get_with_token("/api/bots", &token))
            .await
            .expect("unbound bots");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(body_json(response).await["code"], "login_required");

        // Status stays reachable so the page can discover the sign-in step.
        let response = app
            .clone()
            .oneshot(get_with_token("/api/status", &token))
            .await
            .expect("unbound status");
        assert_eq!(response.status(), StatusCode::OK);
        let status = body_json(response).await;
        assert_eq!(status["login_required"], true);
        assert!(status["account"].is_null());
        assert_eq!(status["device"]["role"], "admin");

        let response = app
            .clone()
            .oneshot(login_request(&token, "alice", "wrong password"))
            .await
            .expect("wrong password");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let body = body_json(response).await;
        assert_eq!(body["error"], "Invalid username or password.");
        assert_eq!(body["code"], "invalid_credentials");

        let response = app
            .clone()
            .oneshot(login_request(&token, "alice", "correct horse battery"))
            .await
            .expect("login");
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_json(response).await;
        assert_eq!(body["account"]["username"], "alice");
        assert_eq!(body["role"], "admin");

        let response = app
            .clone()
            .oneshot(get_with_token("/api/bots", &token))
            .await
            .expect("bound bots");
        assert_eq!(response.status(), StatusCode::OK);

        let response = app
            .clone()
            .oneshot(get_with_token("/api/status", &token))
            .await
            .expect("bound status");
        let status = body_json(response).await;
        assert_eq!(status["login_required"], false);
        assert_eq!(status["account"]["username"], "alice");
        assert_eq!(status["device"]["role"], "admin");

        let audit = RemoteAccessQueries::recent_audit(&state.pool, 20)
            .await
            .expect("audit");
        let sign_in = audit
            .iter()
            .find(|entry| entry.path == "/api/login" && entry.status == 200)
            .expect("sign-in audit row");
        assert_eq!(sign_in.account_name.as_deref(), Some("alice"));
        assert_eq!(sign_in.role.as_deref(), Some("admin"));
    }

    #[tokio::test]
    async fn the_account_grant_caps_the_device_role() {
        let stub = Arc::new(StubAuthenticator::new(
            RemoteAccount {
                id: "account-alice".to_string(),
                username: "alice".to_string(),
                role: "owner".to_string(),
            },
            "correct horse battery",
        ));
        let state = state_with_authenticator(stub.clone()).await;
        let app = router(state.clone());

        // Device viewer + account owner: the account cannot raise the pair.
        let viewer = test_device(&state, "Viewer phone", "viewer").await;
        let response = app
            .clone()
            .oneshot(login_request(&viewer, "alice", "correct horse battery"))
            .await
            .expect("viewer login");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body_json(response).await["role"], "viewer");
        let status = body_json(
            app.clone()
                .oneshot(get_with_token("/api/status", &viewer))
                .await
                .expect("viewer status"),
        )
        .await;
        assert_eq!(status["device"]["role"], "viewer");
        let write = HttpRequest::builder()
            .method("POST")
            .uri(format!("/api/threads/{}/messages", Uuid::new_v4()))
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, format!("Bearer {viewer}"))
            .body(Body::from(
                json!({ "content": "hello", "reply_to_id": null }).to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(write).await.expect("viewer write");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        // Device admin + account operator: the admin-only audit is denied.
        stub.set_account(Some(RemoteAccount {
            id: "account-alice".to_string(),
            username: "alice".to_string(),
            role: "operator".to_string(),
        }));
        let admin = test_device(&state, "Admin phone", "admin").await;
        let response = app
            .clone()
            .oneshot(login_request(&admin, "alice", "correct horse battery"))
            .await
            .expect("admin login");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body_json(response).await["role"], "operator");
        let response = app
            .clone()
            .oneshot(get_with_token("/api/remote/audit", &admin))
            .await
            .expect("admin audit");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        // Device operator + account owner: the device still holds the ceiling.
        stub.set_account(Some(RemoteAccount {
            id: "account-alice".to_string(),
            username: "alice".to_string(),
            role: "owner".to_string(),
        }));
        let operator = test_device(&state, "Operator phone", "operator").await;
        let response = app
            .clone()
            .oneshot(login_request(&operator, "alice", "correct horse battery"))
            .await
            .expect("operator login");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body_json(response).await["role"], "operator");
        let write = HttpRequest::builder()
            .method("POST")
            .uri(format!("/api/threads/{}/messages", Uuid::new_v4()))
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, format!("Bearer {operator}"))
            .body(Body::from(
                json!({ "content": "hello", "reply_to_id": null }).to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(write).await.expect("operator write");
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "an operator clears the role gate and reaches the handler"
        );
    }

    #[tokio::test]
    async fn a_revoked_account_grant_fails_closed_and_clears_the_binding() {
        let stub = Arc::new(StubAuthenticator::new(
            RemoteAccount {
                id: "account-alice".to_string(),
                username: "alice".to_string(),
                role: "operator".to_string(),
            },
            "correct horse battery",
        ));
        let state = state_with_authenticator(stub.clone()).await;
        let app = router(state.clone());
        let token = test_device(&state, "Phone", "operator").await;
        let response = app
            .clone()
            .oneshot(login_request(&token, "alice", "correct horse battery"))
            .await
            .expect("login");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(stub.revalidations(), 0, "a fresh login seeds the cache");

        stub.set_account(None);
        state.clear_all_cached_accounts();
        let response = app
            .clone()
            .oneshot(get_with_token("/api/status", &token))
            .await
            .expect("revoked status");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(body_json(response).await["code"], "account_revoked");
        assert!(stub.revalidations() > 0);

        let device = RemoteAccessQueries::device_for_token(&state.pool, &hash_secret(&token))
            .await
            .expect("device lookup")
            .expect("device");
        assert!(
            device.account_id.is_none(),
            "the dead binding must be cleared"
        );
        let response = app
            .clone()
            .oneshot(get_with_token("/api/status", &token))
            .await
            .expect("unbound status");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body_json(response).await["login_required"], true);

        // A backend failure during revalidation is also fail-closed.
        stub.set_account(Some(RemoteAccount {
            id: "account-alice".to_string(),
            username: "alice".to_string(),
            role: "operator".to_string(),
        }));
        let response = app
            .clone()
            .oneshot(login_request(&token, "alice", "correct horse battery"))
            .await
            .expect("second login");
        assert_eq!(response.status(), StatusCode::OK);
        stub.fail_revalidation();
        state.clear_all_cached_accounts();
        let response = app
            .oneshot(get_with_token("/api/status", &token))
            .await
            .expect("failed revalidation");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(body_json(response).await["code"], "account_revoked");
    }

    async fn insert_sso_attempt(
        pool: &SqlitePool,
        device_id: &str,
        state_value: &str,
        ttl_secs: i64,
    ) -> bool {
        RemoteSsoQueries::create(
            pool,
            &NewRemoteSsoAttempt {
                state_hash: &hash_secret(state_value),
                device_id,
                nonce: "nonce",
                verifier: "verifier",
                issuer: "https://idp.example.com",
                client_id: "client",
                redirect_url: "https://bot.example.com/api/sso/callback",
                token_endpoint: "https://idp.example.com/token",
                jwks_uri: "https://idp.example.com/jwks",
                username_claim: "preferred_username",
                ttl_secs,
            },
            sso::MAX_ATTEMPTS_PER_DEVICE,
            sso::MAX_GLOBAL_ATTEMPTS,
        )
        .await
        .expect("insert attempt")
    }

    #[tokio::test]
    async fn sso_attempts_are_single_use_and_expired_rows_are_pruned() {
        let state = state().await;
        assert!(insert_sso_attempt(&state.pool, "device-a", "state-one", 600).await);
        let consumed = RemoteSsoQueries::consume(&state.pool, &hash_secret("state-one"))
            .await
            .expect("consume")
            .expect("attempt");
        assert_eq!(consumed.device_id, "device-a");
        assert!(
            RemoteSsoQueries::consume(&state.pool, &hash_secret("state-one"))
                .await
                .expect("replay")
                .is_none(),
            "a state is single-use"
        );

        assert!(insert_sso_attempt(&state.pool, "device-a", "state-expired", -1).await);
        assert!(
            RemoteSsoQueries::consume(&state.pool, &hash_secret("state-expired"))
                .await
                .expect("expired")
                .is_none(),
            "an expired state must not be consumable"
        );
        RemoteSsoQueries::prune_expired(&state.pool)
            .await
            .expect("prune");
        let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM remote_sso_attempts")
            .fetch_one(&state.pool)
            .await
            .expect("count");
        assert_eq!(remaining, 0);
    }

    #[tokio::test]
    async fn sso_attempts_are_capped_per_device() {
        let state = state().await;
        for index in 0..sso::MAX_ATTEMPTS_PER_DEVICE {
            assert!(
                insert_sso_attempt(&state.pool, "device-a", &format!("state-{index}"), 600).await,
                "attempt {index}"
            );
        }
        assert!(
            !insert_sso_attempt(&state.pool, "device-a", "state-over", 600).await,
            "a sixth outstanding attempt must be rejected"
        );
        assert!(
            insert_sso_attempt(&state.pool, "device-b", "state-b", 600).await,
            "one device's cap must not block another"
        );
    }

    #[tokio::test]
    async fn account_sign_in_is_unavailable_without_an_authenticator() {
        let state = state().await;
        let token = test_device(&state, "Phone", "operator").await;
        let response = router(state)
            .oneshot(login_request(&token, "alice", "correct horse battery"))
            .await
            .expect("login");
        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert!(body_json(response).await["error"]
            .as_str()
            .is_some_and(|message| message.contains("not enabled")));
    }

    #[tokio::test]
    async fn scim_requires_its_own_bearer_token_and_is_404_when_disabled() {
        let (state, provisioner, token) = scim_state().await;
        let app = router(state.clone());

        let response = app
            .clone()
            .oneshot(scim_request("GET", "/scim/v2/Users", None, None))
            .await
            .expect("missing token");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            response
                .headers()
                .get(header::WWW_AUTHENTICATE)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer")
        );
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("application/scim+json")
        );
        assert_eq!(body_json(response).await["status"], "401");

        let response = app
            .clone()
            .oneshot(scim_request(
                "GET",
                "/scim/v2/Users",
                Some("not-the-token"),
                None,
            ))
            .await
            .expect("wrong token");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        // A paired device token is not a SCIM token, even with the same value
        // shape.
        let device = test_device(&state, "Phone", "admin").await;
        let response = app
            .oneshot(scim_request("GET", "/scim/v2/Users", Some(&device), None))
            .await
            .expect("device token");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let disabled = Arc::new(RemoteState::new_with_services(
            state.pool.clone(),
            state.runtime.clone(),
            None,
            None,
            None,
            Some(provisioner),
        ));
        let response = router(disabled)
            .oneshot(scim_request("GET", "/scim/v2/Users", Some(&token), None))
            .await
            .expect("disabled");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn scim_discovery_reports_capabilities_without_a_provisioner() {
        let (base_state, _, token) = scim_state().await;
        // Discovery is static: a configured, authenticated listener must answer
        // even when no AccountProvisioner is attached.
        let state = Arc::new(RemoteState::new_with_services(
            base_state.pool.clone(),
            base_state.runtime.clone(),
            None,
            None,
            base_state.scim.clone(),
            None,
        ));
        let app = router(state.clone());

        let response = app
            .clone()
            .oneshot(scim_request(
                "GET",
                "/scim/v2/ServiceProviderConfig",
                Some(&token),
                None,
            ))
            .await
            .expect("service provider config");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("application/scim+json")
        );
        let body = body_json(response).await;
        assert_eq!(
            body["schemas"][0],
            "urn:ietf:params:scim:schemas:core:2.0:ServiceProviderConfig"
        );
        assert_eq!(body["patch"]["supported"], true);
        assert_eq!(body["bulk"]["supported"], false);
        assert_eq!(body["bulk"]["maxOperations"], 0);
        assert_eq!(body["bulk"]["maxPayloadSize"], 0);
        assert_eq!(body["filter"]["supported"], true);
        assert_eq!(body["filter"]["maxResults"], SCIM_MAX_RESULTS);
        assert_eq!(body["changePassword"]["supported"], false);
        assert_eq!(body["sort"]["supported"], false);
        assert_eq!(body["etag"]["supported"], false);
        assert_eq!(body["authenticationSchemes"][0]["type"], "oauthbearertoken");
        assert_eq!(body["authenticationSchemes"][0]["primary"], true);
        assert_eq!(body["authenticationSchemes"].as_array().unwrap().len(), 1);

        for uri in ["/scim/v2/ResourceTypes", "/scim/v2/ResourceTypes/"] {
            let response = app
                .clone()
                .oneshot(scim_request("GET", uri, Some(&token), None))
                .await
                .expect("resource types");
            assert_eq!(response.status(), StatusCode::OK, "{uri}");
            let body = body_json(response).await;
            assert_eq!(
                body["schemas"][0],
                "urn:ietf:params:scim:schemas:core:2.0:ListResponse"
            );
            assert_eq!(body["totalResults"], 1);
            assert_eq!(body["itemsPerPage"], 1);
            let user = &body["Resources"][0];
            assert_eq!(
                user["schemas"][0],
                "urn:ietf:params:scim:schemas:core:2.0:ResourceType"
            );
            assert_eq!(user["id"], "User");
            assert_eq!(user["name"], "User");
            assert_eq!(user["endpoint"], "/scim/v2/Users");
            assert_eq!(user["schema"], "urn:ietf:params:scim:schemas:core:2.0:User");
            assert!(user.get("schemaExtensions").is_none());
        }

        // The same discovery routes still require the SCIM token.
        for uri in ["/scim/v2/ServiceProviderConfig", "/scim/v2/ResourceTypes"] {
            let response = app
                .clone()
                .oneshot(scim_request("GET", uri, Some("wrong-token"), None))
                .await
                .expect("wrong token");
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{uri}");
        }

        let disabled = Arc::new(RemoteState::new_with_services(
            state.pool.clone(),
            state.runtime.clone(),
            None,
            None,
            None,
            None,
        ));
        for uri in ["/scim/v2/ServiceProviderConfig", "/scim/v2/ResourceTypes"] {
            let response = router(disabled.clone())
                .oneshot(scim_request("GET", uri, Some(&token), None))
                .await
                .expect("disabled");
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{uri}");
        }
    }

    #[tokio::test]
    async fn scim_rate_limit_returns_429() {
        let (state, _, token) = scim_state().await;
        let ip = IpAddr::from([127, 0, 0, 1]);
        for _ in 0..MAX_SCIM_REQUESTS_PER_MINUTE {
            assert!(state.scim_allowed(ip));
        }
        let response = router(state)
            .oneshot(scim_request("GET", "/scim/v2/Users", Some(&token), None))
            .await
            .expect("rate limited");
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn scim_lists_filters_paginates_and_accepts_the_trailing_slash() {
        let (state, provisioner, token) = scim_state().await;
        let alice = provisioner
            .create_user("alice", Some("ext-a"), true, "viewer")
            .await
            .expect("alice");
        provisioner
            .create_user("bob", Some("ext-b"), true, "operator")
            .await
            .expect("bob");
        let app = router(state);

        let response = app
            .clone()
            .oneshot(scim_request("GET", "/scim/v2/Users", Some(&token), None))
            .await
            .expect("list");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("application/scim+json")
        );
        let body = body_json(response).await;
        assert_eq!(body["totalResults"], 2);
        assert_eq!(body["startIndex"], 1);
        assert_eq!(body["itemsPerPage"], 2);
        assert_eq!(body["Resources"][0]["userName"], "alice");
        assert_eq!(
            body["Resources"][0]["schemas"][0],
            "urn:ietf:params:scim:schemas:core:2.0:User"
        );
        assert!(body["Resources"][0]["meta"]["created"].is_string());
        assert!(body["Resources"][0].get("location").is_none());

        let response = app
            .clone()
            .oneshot(scim_request(
                "GET",
                "/scim/v2/Users?filter=userName%20EQ%20%22ALICE%22",
                Some(&token),
                None,
            ))
            .await
            .expect("filter userName");
        let body = body_json(response).await;
        assert_eq!(body["totalResults"], 1);
        assert_eq!(body["Resources"][0]["id"], alice.id);

        let response = app
            .clone()
            .oneshot(scim_request(
                "GET",
                "/scim/v2/Users?filter=externalId%20eq%20%22ext-b%22",
                Some(&token),
                None,
            ))
            .await
            .expect("filter externalId");
        assert_eq!(body_json(response).await["Resources"][0]["userName"], "bob");

        let response = app
            .clone()
            .oneshot(scim_request(
                "GET",
                &format!("/scim/v2/Users?filter=id%20eq%20%22{}%22", alice.id),
                Some(&token),
                None,
            ))
            .await
            .expect("filter id");
        assert_eq!(body_json(response).await["totalResults"], 1);

        let response = app
            .clone()
            .oneshot(scim_request(
                "GET",
                "/scim/v2/Users?startIndex=2&count=1",
                Some(&token),
                None,
            ))
            .await
            .expect("pagination");
        let body = body_json(response).await;
        assert_eq!(body["totalResults"], 2);
        assert_eq!(body["startIndex"], 2);
        assert_eq!(body["itemsPerPage"], 1);
        assert_eq!(body["Resources"][0]["userName"], "bob");

        let response = app
            .clone()
            .oneshot(scim_request("GET", "/scim/v2/Users/", Some(&token), None))
            .await
            .expect("trailing slash");
        assert_eq!(response.status(), StatusCode::OK);

        let response = app
            .oneshot(scim_request(
                "GET",
                "/scim/v2/Users?filter=displayName%20eq%20%22x%22",
                Some(&token),
                None,
            ))
            .await
            .expect("bad filter");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(body_json(response).await["scimType"], "invalidFilter");
    }

    #[tokio::test]
    async fn scim_create_returns_201_and_maps_conflicts_owner_and_inactive() {
        let (state, _, token) = scim_state().await;
        let app = router(state);

        let response = app
            .clone()
            .oneshot(scim_request(
                "POST",
                "/scim/v2/Users",
                Some(&token),
                Some(json!({
                    "schemas": ["urn:ietf:params:scim:schemas:core:2.0:User"],
                    "userName": "dave",
                    "externalId": "ext-dave",
                })),
            ))
            .await
            .expect("create");
        assert_eq!(response.status(), StatusCode::CREATED);
        let location = response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .expect("location")
            .to_string();
        assert!(location.starts_with("/scim/v2/Users/"));
        let body = body_json(response).await;
        assert_eq!(body["userName"], "dave");
        assert_eq!(body["externalId"], "ext-dave");
        assert_eq!(body["active"], true);
        assert_eq!(body["meta"]["resourceType"], "User");

        let response = app
            .clone()
            .oneshot(scim_request(
                "POST",
                "/scim/v2/Users",
                Some(&token),
                Some(json!({ "userName": "DAVE" })),
            ))
            .await
            .expect("duplicate");
        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert_eq!(body_json(response).await["scimType"], "uniqueness");

        let response = app
            .clone()
            .oneshot(scim_request(
                "POST",
                "/scim/v2/Users",
                Some(&token),
                Some(json!({ "userName": "owner" })),
            ))
            .await
            .expect("owner");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(body_json(response).await["scimType"], "mutability");

        let response = app
            .clone()
            .oneshot(scim_request(
                "POST",
                "/scim/v2/Users",
                Some(&token),
                Some(json!({ "userName": "erin", "active": false })),
            ))
            .await
            .expect("inactive create");
        assert_eq!(response.status(), StatusCode::CREATED);
        assert_eq!(body_json(response).await["active"], false);

        let response = app
            .oneshot(scim_request(
                "POST",
                "/scim/v2/Users",
                Some(&token),
                Some(json!({ "externalId": "missing-name" })),
            ))
            .await
            .expect("missing username");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(body_json(response).await["scimType"], "invalidValue");
    }

    #[tokio::test]
    async fn scim_get_patch_put_and_soft_delete() {
        let (state, provisioner, token) = scim_state().await;
        let bob = provisioner
            .create_user("bob", Some("ext-bob"), true, "viewer")
            .await
            .expect("bob");
        let app = router(state);

        let response = app
            .clone()
            .oneshot(scim_request(
                "GET",
                &format!("/scim/v2/Users/{}", bob.id),
                Some(&token),
                None,
            ))
            .await
            .expect("get");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body_json(response).await["userName"], "bob");

        let response = app
            .clone()
            .oneshot(scim_request(
                "PATCH",
                &format!("/scim/v2/Users/{}", bob.id),
                Some(&token),
                Some(json!({
                    "schemas": ["urn:ietf:params:scim:api:messages:2.0:PatchOp"],
                    "Operations": [{ "op": "replace", "path": "active", "value": false }],
                })),
            ))
            .await
            .expect("patch active");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body_json(response).await["active"], false);

        // The no-path object form and an add for active are both tolerated.
        let response = app
            .clone()
            .oneshot(scim_request(
                "PATCH",
                &format!("/scim/v2/Users/{}", bob.id),
                Some(&token),
                Some(json!({
                    "Operations": [{ "op": "add", "value": { "active": true } }],
                })),
            ))
            .await
            .expect("add active");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body_json(response).await["active"], true);

        let response = app
            .clone()
            .oneshot(scim_request(
                "PATCH",
                &format!("/scim/v2/Users/{}", bob.id),
                Some(&token),
                Some(json!({
                    "Operations": [{ "op": "replace", "value": { "userName": "robert" } }],
                })),
            ))
            .await
            .expect("rename patch");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body_json(response).await["userName"], "robert");

        let response = app
            .clone()
            .oneshot(scim_request(
                "PATCH",
                &format!("/scim/v2/Users/{}", bob.id),
                Some(&token),
                Some(json!({
                    "Operations": [{ "op": "replace", "path": "displayName", "value": "x" }],
                })),
            ))
            .await
            .expect("bad patch");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(body_json(response).await["scimType"], "invalidPath");

        let response = app
            .clone()
            .oneshot(scim_request(
                "PUT",
                &format!("/scim/v2/Users/{}", bob.id),
                Some(&token),
                Some(json!({
                    "userName": "bobby",
                    "active": true,
                    "displayName": "ignored",
                })),
            ))
            .await
            .expect("put");
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_json(response).await;
        assert_eq!(body["userName"], "bobby");
        assert_eq!(body["active"], true);

        let response = app
            .clone()
            .oneshot(scim_request(
                "DELETE",
                &format!("/scim/v2/Users/{}", bob.id),
                Some(&token),
                None,
            ))
            .await
            .expect("delete");
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let response = app
            .clone()
            .oneshot(scim_request(
                "DELETE",
                &format!("/scim/v2/Users/{}", bob.id),
                Some(&token),
                None,
            ))
            .await
            .expect("delete again");
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let response = app
            .oneshot(scim_request(
                "GET",
                &format!("/scim/v2/Users/{}", bob.id),
                Some(&token),
                None,
            ))
            .await
            .expect("get deleted");
        assert_eq!(body_json(response).await["active"], false);
    }

    #[tokio::test]
    async fn scim_audits_mutations_and_auth_failures_and_updates_last_used() {
        let (state, _, token) = scim_state().await;
        let app = router(state.clone());

        let response = app
            .clone()
            .oneshot(scim_request(
                "POST",
                "/scim/v2/Users",
                Some(&token),
                Some(json!({ "userName": "dave" })),
            ))
            .await
            .expect("create");
        assert_eq!(response.status(), StatusCode::CREATED);
        let user_id = body_json(response).await["id"]
            .as_str()
            .unwrap()
            .to_string();

        let last_used =
            ravenbot_db::queries::AppSettingsQueries::get(&state.pool, "remote_scim_last_used_at")
                .await
                .expect("last used")
                .expect("last used value");
        assert!(!last_used.is_empty());

        let audit = RemoteAccessQueries::recent_audit(&state.pool, 20)
            .await
            .expect("audit");
        let created = audit
            .iter()
            .find(|entry| entry.method == "SCIM POST" && entry.status == 201)
            .expect("create audit");
        assert_eq!(created.device_name.as_deref(), Some("SCIM"));
        assert_eq!(created.account_name.as_deref(), Some("dave"));
        assert_eq!(created.path, "/scim/v2/Users");

        // A read is not audited.
        let before = audit.len();
        let response = app
            .clone()
            .oneshot(scim_request("GET", "/scim/v2/Users", Some(&token), None))
            .await
            .expect("read");
        assert_eq!(response.status(), StatusCode::OK);
        let audit_after = RemoteAccessQueries::recent_audit(&state.pool, 20)
            .await
            .expect("audit after read");
        assert_eq!(audit_after.len(), before);

        // A bad token writes an authentication failure row.
        let response = app
            .clone()
            .oneshot(scim_request("GET", "/scim/v2/Users", Some("wrong"), None))
            .await
            .expect("bad token");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let audit = RemoteAccessQueries::recent_audit(&state.pool, 20)
            .await
            .expect("audit after failure");
        assert!(audit
            .iter()
            .any(|entry| entry.device_name.as_deref() == Some("SCIM") && entry.status == 401));

        let _ = user_id;
    }

    #[tokio::test]
    async fn an_invalid_or_unbacked_scim_config_never_stops_the_listener() {
        let (state, provisioner, token) = scim_state().await;

        // An invalid saved config disables SCIM but the listener still serves
        // the device surface.
        let server = start_with_services(
            state.pool.clone(),
            state.runtime.clone(),
            RemoteConfig {
                port: 0,
                allow_lan: false,
                tls: false,
                tls_dir: None,
                sso: None,
                scim: Some(ScimConfig {
                    token_hash: "not-a-hash".to_string(),
                    default_role: "viewer".to_string(),
                }),
            },
            None,
            Some(provisioner.clone()),
        )
        .await
        .expect("listener");
        let base = format!("http://127.0.0.1:{}", server.addr().port());
        let client = reqwest::Client::new();
        assert_eq!(
            client
                .get(format!("{base}/api/health"))
                .send()
                .await
                .expect("health")
                .status(),
            200
        );
        assert_eq!(
            client
                .get(format!("{base}/scim/v2/Users"))
                .bearer_auth(&token)
                .send()
                .await
                .expect("scim")
                .status(),
            404
        );
        server.shutdown().await;

        // A valid config without a provisioner is also disabled, not mounted
        // as a route that returns 500.
        let config = ScimConfig::new(&hash_secret(&token), "viewer").expect("config");
        let server = start_with_services(
            state.pool.clone(),
            state.runtime.clone(),
            RemoteConfig {
                port: 0,
                allow_lan: false,
                tls: false,
                tls_dir: None,
                sso: None,
                scim: Some(config),
            },
            None,
            None,
        )
        .await
        .expect("listener");
        let base = format!("http://127.0.0.1:{}", server.addr().port());
        assert_eq!(
            client
                .get(format!("{base}/scim/v2/Users"))
                .bearer_auth(&token)
                .send()
                .await
                .expect("scim without provisioner")
                .status(),
            404
        );
        server.shutdown().await;
    }

    #[tokio::test]
    async fn scim_does_not_open_the_device_api_or_leak_into_status() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        ravenbot_db::migrations::run(&pool)
            .await
            .expect("migrations");
        let db = ravenbot_db::Database::from_pool(pool.clone());
        let token = generate_scim_token();
        let config = ScimConfig::new(&hash_secret(&token), "viewer").expect("scim");
        let provisioner = Arc::new(StubProvisioner::new("owner"));
        let authenticator = Arc::new(StubAuthenticator::new(
            RemoteAccount {
                id: "account-alice".to_string(),
                username: "alice".to_string(),
                role: "owner".to_string(),
            },
            "correct horse battery",
        ));
        let state = Arc::new(RemoteState::new_with_services(
            pool,
            Arc::new(Runtime::new(db)),
            Some(authenticator),
            None,
            Some(config),
            Some(provisioner),
        ));

        let device = test_device(&state, "Phone", "admin").await;
        let app = router(state);

        // Status is reachable before the device is bound and must stay free of
        // SCIM configuration.
        let response = app
            .clone()
            .oneshot(get_with_token("/api/status", &device))
            .await
            .expect("device status");
        assert_eq!(response.status(), StatusCode::OK);
        let status = body_json(response).await;
        assert!(status.get("scim").is_none());
        assert!(status.get("sso").is_none());

        let response = app
            .clone()
            .oneshot(login_request(&device, "alice", "correct horse battery"))
            .await
            .expect("device login");
        assert_eq!(response.status(), StatusCode::OK);

        let response = app
            .clone()
            .oneshot(get_with_token("/api/bots", &device))
            .await
            .expect("device api");
        assert_eq!(response.status(), StatusCode::OK);

        let response = app
            .oneshot(scim_request("GET", "/scim/v2/Users", Some(&device), None))
            .await
            .expect("scim with device token");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
