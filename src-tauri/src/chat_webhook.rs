//! Loopback receiver for signed chat-provider webhooks.
//!
//! WhatsApp and Microsoft Teams both deliver over HTTPS webhooks. The listener
//! binds loopback only; exposing it is an explicit reverse-proxy or tunnel
//! decision. Provider-specific signature and token checks live in the provider
//! modules (`whatsapp` and `teams`).

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use ravenbot_db::Database;
use ravenbot_runtime::Runtime;

const WEBHOOK_BIND_ENV: &str = "RAVENBOT_CHAT_WEBHOOK_BIND";
const WEBHOOK_PORT_ENV: &str = "RAVENBOT_CHAT_WEBHOOK_PORT";
const DEFAULT_WEBHOOK_PORT: u16 = 8801;

static ACTIVE_WEBHOOK_BASE: std::sync::OnceLock<String> = std::sync::OnceLock::new();

fn webhook_bind() -> String {
    std::env::var(WEBHOOK_BIND_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "127.0.0.1".to_string())
}

fn webhook_port() -> u16 {
    std::env::var(WEBHOOK_PORT_ENV)
        .ok()
        .and_then(|value| value.trim().parse::<u16>().ok())
        .unwrap_or(DEFAULT_WEBHOOK_PORT)
}

pub(crate) fn local_base() -> String {
    if let Some(url) = ACTIVE_WEBHOOK_BASE.get() {
        return url.clone();
    }
    let bind = webhook_bind();
    let host = if bind == "0.0.0.0" || bind == "::" {
        "127.0.0.1"
    } else {
        &bind
    };
    format!("http://{host}:{}", webhook_port())
}

pub fn local_url() -> String {
    format!("{}/whatsapp", local_base())
}

pub fn local_teams_url() -> String {
    format!("{}/teams", local_base())
}

#[derive(Clone)]
pub(crate) struct ChatWebhookState {
    pub(crate) db: Database,
    pub(crate) runtime: Arc<Runtime>,
    pub(crate) events: crate::HostEvents,
    pub(crate) client: reqwest::Client,
    pub(crate) whatsapp_api_base: String,
    pub(crate) teams: Arc<crate::teams::TeamsService>,
}

#[derive(Debug, Clone)]
pub struct ChatWebhookConfig {
    pub bind: String,
    pub port: u16,
    /// Override for tests. Production uses `RAVENBOT_WHATSAPP_API_BASE` or the
    /// Graph API default.
    pub whatsapp_api_base: Option<String>,
    /// Override for tests. Production uses `RAVENBOT_TEAMS_TOKEN_URL` or the
    /// Bot Framework token endpoint.
    pub teams_token_url: Option<String>,
    /// Override for tests. Production uses `RAVENBOT_TEAMS_OPENID_URL` or the
    /// Bot Framework OpenID metadata endpoint.
    pub teams_openid_url: Option<String>,
}

impl Default for ChatWebhookConfig {
    fn default() -> Self {
        Self {
            bind: webhook_bind(),
            port: webhook_port(),
            whatsapp_api_base: None,
            teams_token_url: None,
            teams_openid_url: None,
        }
    }
}

/// The loopback listener for signed chat webhooks. Keep it alive for the life
/// of the desktop app or daemon; dropping it aborts the accept loop.
pub(crate) struct ChatWebhookServer {
    addr: SocketAddr,
    task: tokio::task::JoinHandle<()>,
}

impl ChatWebhookServer {
    pub(crate) fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub(crate) fn local_urls(&self) -> (String, String) {
        (
            format!("http://{}/whatsapp", self.addr()),
            format!("http://{}/teams", self.addr()),
        )
    }

    pub(crate) async fn start(
        db: Database,
        runtime: Arc<Runtime>,
        events: crate::HostEvents,
        config: ChatWebhookConfig,
    ) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|error| error.to_string())?;
        let teams = Arc::new(crate::teams::TeamsService::new(
            client.clone(),
            config.teams_token_url,
            config.teams_openid_url,
        ));
        let state = ChatWebhookState {
            db,
            runtime,
            events,
            client,
            whatsapp_api_base: config
                .whatsapp_api_base
                .unwrap_or_else(crate::whatsapp::api_base),
            teams,
        };
        let app = Router::new()
            .route("/health", get(|| async { "ok" }))
            .route("/whatsapp", get(whatsapp_verify).post(whatsapp_receive))
            .route("/teams", post(teams_receive))
            .layer(DefaultBodyLimit::max(1024 * 1024))
            .with_state(state);
        let bind = format!("{}:{}", config.bind, config.port);
        let listener = tokio::net::TcpListener::bind(&bind)
            .await
            .map_err(|error| format!("failed to bind chat webhook at {bind}: {error}"))?;
        let addr = listener
            .local_addr()
            .map_err(|error| format!("no local address: {error}"))?;
        let _ = ACTIVE_WEBHOOK_BASE.set(format!("http://{addr}"));
        let task = tokio::spawn(async move {
            if let Err(error) = axum::serve(listener, app).await {
                tracing::warn!(%error, "Chat webhook receiver stopped");
            }
        });
        Ok(Self { addr, task })
    }
}

impl Drop for ChatWebhookServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn whatsapp_verify(
    State(state): State<ChatWebhookState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let Some(expected) = crate::whatsapp::read_verify_token(&state.db).await else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "WhatsApp is not configured",
        )
            .into_response();
    };
    if params.get("hub.mode").map(String::as_str) != Some("subscribe") {
        return (StatusCode::FORBIDDEN, "invalid mode").into_response();
    }
    let Some(received) = params.get("hub.verify_token") else {
        return (StatusCode::FORBIDDEN, "missing verify token").into_response();
    };
    if !crate::whatsapp::constant_time_eq(received, &expected) {
        return (StatusCode::FORBIDDEN, "invalid verify token").into_response();
    }
    let Some(challenge) = params.get("hub.challenge") else {
        return (StatusCode::BAD_REQUEST, "missing challenge").into_response();
    };
    (StatusCode::OK, challenge.clone()).into_response()
}

async fn whatsapp_receive(
    State(state): State<ChatWebhookState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !crate::whatsapp::enabled(&state.db).await {
        return (StatusCode::OK, "ignored").into_response();
    }
    let Some(secret) = crate::whatsapp::read_app_secret(&state.db).await else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "WhatsApp app secret is not configured",
        )
            .into_response();
    };
    let signature = headers
        .get("x-hub-signature-256")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    if !crate::whatsapp::verify_signature(&secret, &body, signature) {
        return (StatusCode::FORBIDDEN, "invalid signature").into_response();
    }
    let update: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid JSON").into_response(),
    };
    let db = state.db.clone();
    let runtime = state.runtime.clone();
    let events = state.events.clone();
    let client = state.client.clone();
    let base = state.whatsapp_api_base.clone();
    tokio::spawn(async move {
        if let Err(error) = crate::whatsapp::process_update_with_base(
            &client, &db, &runtime, &events, &update, &base,
        )
        .await
        {
            tracing::warn!(%error, "WhatsApp webhook processing failed");
        }
    });
    (StatusCode::OK, "EVENT_RECEIVED").into_response()
}

async fn teams_receive(
    State(state): State<ChatWebhookState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    match crate::teams::receive(&state, &headers, &body).await {
        Ok((status, message)) => (status, message).into_response(),
        Err(error) => {
            tracing::warn!(%error, "Teams webhook processing failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "webhook error").into_response()
        }
    }
}
