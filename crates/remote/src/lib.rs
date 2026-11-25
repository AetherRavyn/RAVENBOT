//! Authenticated remote access for RAVENBOT.
//!
//! This is deliberately a small, local-first surface:
//!
//! * loopback by default; LAN binding is an explicit opt-in with a warning;
//! * pairing codes are one-time, short-lived and rate-limited;
//! * tokens are random, shown once, and stored only as SHA-256 hashes;
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
    ApprovalQueries, MessageQueries, QuestionQueries, RemoteAccessQueries, RunQueries,
    ThreadQueries, ThreadShareQueries,
};
use ravenbot_runtime::Runtime;

pub const DEFAULT_PORT: u16 = 8790;
const MAX_PAIRING_ATTEMPTS_PER_MINUTE: usize = 5;
const MAX_GLOBAL_PAIRING_ATTEMPTS_PER_MINUTE: usize = 30;
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
}

impl Default for RemoteConfig {
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            allow_lan: false,
            tls: false,
            tls_dir: None,
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

#[derive(Debug, Clone, Serialize)]
pub struct PairedDevice {
    pub token: String,
    pub device_id: Uuid,
    pub name: String,
    pub role: RemoteRole,
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
    // Bound the pairing and audit tables before the listener accepts traffic.
    if let Err(error) = RemoteAccessQueries::prune(&pool).await {
        tracing::warn!(%error, "Could not prune remote access tables");
    }
    let port = config.normalized_port();
    let bind = format!("{}:{}", config.bind_addr(), port);
    let state = Arc::new(RemoteState::new(pool, runtime));
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

pub struct RemoteState {
    pool: SqlitePool,
    runtime: Arc<Runtime>,
    pairing_attempts: Mutex<HashMap<IpAddr, VecDeque<Instant>>>,
    global_pairing_attempts: Mutex<VecDeque<Instant>>,
    last_touch: Mutex<HashMap<String, Instant>>,
}

impl RemoteState {
    fn new(pool: SqlitePool, runtime: Arc<Runtime>) -> Self {
        Self {
            pool,
            runtime,
            pairing_attempts: Mutex::new(HashMap::new()),
            global_pairing_attempts: Mutex::new(VecDeque::new()),
            last_touch: Mutex::new(HashMap::new()),
        }
    }

    fn pairing_allowed(&self, ip: IpAddr) -> bool {
        let now = Instant::now();
        let mut global = self.global_pairing_attempts.lock().expect("pairing lock");
        while global
            .front()
            .is_some_and(|at| now.duration_since(*at) > Duration::from_secs(60))
        {
            global.pop_front();
        }
        if global.len() >= MAX_GLOBAL_PAIRING_ATTEMPTS_PER_MINUTE {
            return false;
        }
        let mut by_ip = self.pairing_attempts.lock().expect("pairing lock");
        let attempts = by_ip.entry(ip).or_default();
        while attempts
            .front()
            .is_some_and(|at| now.duration_since(*at) > Duration::from_secs(60))
        {
            attempts.pop_front();
        }
        if attempts.len() >= MAX_PAIRING_ATTEMPTS_PER_MINUTE {
            return false;
        }
        attempts.push_back(now);
        global.push_back(now);
        true
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

pub fn router(state: Arc<RemoteState>) -> Router {
    let public = Router::new()
        .route("/", get(index))
        .route("/app.css", get(app_css))
        .route("/app.js", get(app_js))
        .route("/api/health", get(health))
        .route("/api/pair", post(pair))
        .route("/share/{token}", get(share_page));

    let protected = Router::new()
        .route("/api/status", get(status))
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
}

impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
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
        (self.status, Json(json!({ "error": self.message }))).into_response()
    }
}

type ApiResult<T> = Result<T, ApiError>;

#[derive(Debug, Clone)]
struct AuthDevice {
    id: String,
    name: String,
    role: RemoteRole,
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
    let role = RemoteRole::parse(&device.role);
    let method = request.method().clone();
    // A device may revoke itself even as a viewer. Every other mutation needs
    // the operator role.
    let self_revoke = request.uri().path() == "/api/device/revoke";
    if method != Method::GET && method != Method::HEAD && !self_revoke && !role.can_write() {
        return ApiError::forbidden().into_response();
    }
    let auth = AuthDevice {
        id: device.id.clone(),
        name: device.name.clone(),
        role,
    };
    request.extensions_mut().insert(auth.clone());
    let path = request.uri().path().to_string();
    let response = next.run(request).await;
    let status = response.status().as_u16() as i64;
    // Reads are polled; only mutations and failures are persisted, so polling
    // does not turn into a write every three seconds.
    if method != Method::GET || status >= 400 {
        let _ = RemoteAccessQueries::audit(
            &state.pool,
            Some(&auth.id),
            Some(&auth.name),
            Some(auth.role.as_str()),
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
    }))
}

// ─────────────────────────────────────────────────────────────────────────────
// Authenticated endpoints
// ─────────────────────────────────────────────────────────────────────────────

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
    Ok(Json(json!({
        "device": { "id": auth.id, "name": auth.name, "role": auth.role },
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
    fn roles_are_ordered_by_capability() {
        assert!(!RemoteRole::Viewer.can_write());
        assert!(RemoteRole::Operator.can_write());
        assert!(!RemoteRole::Operator.can_admin());
        assert!(RemoteRole::Admin.can_write());
        assert!(RemoteRole::Admin.can_admin());
        assert_eq!(RemoteRole::parse("ADMIN"), RemoteRole::Admin);
        assert_eq!(RemoteRole::parse("nonsense"), RemoteRole::Viewer);
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
        let response = router(state)
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("response");
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
        assert!(page.text().await.unwrap().contains("Pair this device"));

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
}
