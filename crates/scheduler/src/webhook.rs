//! Inbound webhook receiver for routine triggers.
//!
//! A deliberately minimal HTTP/1.1 server: it accepts `POST /hooks/<secret>`
//! and `GET /health` on **loopback only**. A valid secret (from the URL or an
//! `Authorization: Bearer <secret>` header) maps to an enabled routine, which
//! is dispatched through the same executor the cron scheduler uses. Nothing
//! else is exposed — no bot CRUD, no settings, no files.
//!
//! The receiver never parses or trusts the request body; it only reads enough
//! headers to route and authenticate, then responds immediately (202) and runs
//! the routine asynchronously.

use std::net::SocketAddr;

use ravenbot_core::Routine;
use ravenbot_db::queries::WebhookQueries;
use sqlx::SqlitePool;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::scheduler::RoutineExecutor;

/// Hard cap on a request's header block (protects against a client that never
/// sends a blank line).
const MAX_HEADER_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone)]
pub struct WebhookConfig {
    /// Bind address. Defaults to loopback so the receiver is not public.
    pub bind: String,
    /// Port; 0 means "let the OS choose" (useful for tests).
    pub port: u16,
}

impl Default for WebhookConfig {
    fn default() -> Self {
        let port = std::env::var("RAVENBOT_WEBHOOK_PORT")
            .ok()
            .and_then(|v| v.trim().parse::<u16>().ok())
            .unwrap_or(8800);
        Self {
            bind: std::env::var("RAVENBOT_WEBHOOK_BIND")
                .unwrap_or_else(|_| "127.0.0.1".to_string()),
            port,
        }
    }
}

/// A running webhook receiver. Dropping it aborts the accept loop.
pub struct WebhookServer {
    addr: SocketAddr,
    task: tokio::task::JoinHandle<()>,
}

impl WebhookServer {
    /// The actual bound address (resolves port 0 to the chosen port).
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Bind and start serving. Returns once the listener is bound.
    pub async fn start(
        pool: SqlitePool,
        executor: RoutineExecutor,
        config: WebhookConfig,
    ) -> Result<Self, String> {
        let bind = format!("{}:{}", config.bind, config.port);
        let listener = TcpListener::bind(&bind)
            .await
            .map_err(|e| format!("failed to bind webhook receiver at {}: {}", bind, e))?;
        let addr = listener
            .local_addr()
            .map_err(|e| format!("no local address: {}", e))?;

        let task = tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _peer)) => {
                        let pool = pool.clone();
                        let executor = executor.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(stream, pool, executor).await {
                                tracing::debug!(error = %e, "webhook connection error");
                            }
                        });
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "webhook accept failed");
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    }
                }
            }
        });

        Ok(Self { addr, task })
    }
}

impl Drop for WebhookServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn handle_connection(
    mut stream: TcpStream,
    pool: SqlitePool,
    executor: RoutineExecutor,
) -> Result<(), String> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    // Read until end of headers, bounded.
    loop {
        let n = stream
            .read(&mut chunk)
            .await
            .map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") || buf.len() > MAX_HEADER_BYTES {
            break;
        }
    }
    let text = String::from_utf8_lossy(&buf);
    let mut lines = text.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("");

    let mut authorization: Option<String> = None;
    for line in lines {
        if line.is_empty() {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            if key.trim().eq_ignore_ascii_case("authorization") {
                authorization = Some(value.trim().to_string());
            }
        }
    }

    // Health check — unauthenticated so an operator can verify reachability.
    if method == "GET" && path == "/health" {
        return respond(&mut stream, 200, "ok").await;
    }

    if method != "POST" {
        return respond(&mut stream, 405, "method not allowed").await;
    }

    // Extract the secret from the path (/hooks/<secret>) or the bearer header.
    let path_secret = path
        .strip_prefix("/hooks/")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let bearer = authorization
        .as_deref()
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let Some(secret) = path_secret.or(bearer) else {
        return respond(&mut stream, 401, "missing webhook secret").await;
    };

    let routine_id = match WebhookQueries::routine_for_secret(&pool, &secret).await {
        Ok(Some(id)) => id,
        Ok(None) => return respond(&mut stream, 404, "unknown or disabled webhook").await,
        Err(e) => {
            tracing::warn!(error = %e, "webhook lookup failed");
            return respond(&mut stream, 500, "lookup failed").await;
        }
    };

    // Load the routine and dispatch through the same executor cron uses.
    let routine = match ravenbot_scheduler_routine_get(&pool, routine_id).await {
        Ok(Some(r)) => r,
        Ok(None) => return respond(&mut stream, 404, "routine not found").await,
        Err(e) => {
            tracing::warn!(error = %e, "webhook routine load failed");
            return respond(&mut stream, 500, "routine load failed").await;
        }
    };

    tracing::info!(routine = %routine.name, "Webhook triggered routine");
    let executor = executor.clone();
    tokio::spawn(async move {
        if let Err(e) = executor(routine).await {
            tracing::warn!(error = %e, "webhook routine execution failed");
        }
    });

    respond(&mut stream, 202, "accepted").await
}

async fn respond(stream: &mut TcpStream, status: u16, body: &str) -> Result<(), String> {
    let reason = match status {
        200 => "OK",
        202 => "Accepted",
        401 => "Unauthorized",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Error",
    };
    let response = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status,
        reason,
        body.len(),
        body
    );
    stream
        .write_all(response.as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    let _ = stream.shutdown().await;
    Ok(())
}

/// Load a routine using the scheduler's own column mapping.
async fn ravenbot_scheduler_routine_get(
    pool: &SqlitePool,
    routine_id: uuid::Uuid,
) -> Result<Option<Routine>, String> {
    crate::routine::RoutineManager::new(pool.clone())
        .get(routine_id)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use ravenbot_core::Routine;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    async fn temp_db() -> ravenbot_db::Database {
        let path = std::path::PathBuf::from(std::env::temp_dir())
            .join(format!("ravenbot-webhook-{}.db", uuid::Uuid::new_v4()));
        ravenbot_db::Database::new(&path).await.expect("temp db")
    }

    async fn seed_routine(db: &ravenbot_db::Database, secret: &str) -> uuid::Uuid {
        let bot = ravenbot_core::Bot::new("WebhookBot", "test");
        ravenbot_db::queries::BotQueries::insert(db.pool(), &bot).await.unwrap();
        let routine = Routine::new(bot.id, "Hook routine", "0 9 * * *", "do work");
        sqlx::query(
            "INSERT INTO routines (id, bot_id, name, description, schedule, instruction, is_enabled, created_at, updated_at)
             VALUES (?, ?, ?, '', ?, ?, 1, datetime('now'), datetime('now'))",
        )
        .bind(routine.id.to_string())
        .bind(routine.bot_id.to_string())
        .bind(&routine.name)
        .bind(&routine.schedule)
        .bind(&routine.instruction)
        .execute(db.pool())
        .await
        .unwrap();
        WebhookQueries::set(db.pool(), routine.id, Some(secret), true).await.unwrap();
        routine.id
    }

    async fn post(addr: SocketAddr, path: &str, auth: Option<&str>) -> String {
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let auth_header = auth.map(|a| format!("Authorization: Bearer {}\r\n", a)).unwrap_or_default();
        let req = format!(
            "POST {} HTTP/1.1\r\nHost: localhost\r\n{}Content-Length: 0\r\nConnection: close\r\n\r\n",
            path, auth_header
        );
        stream.write_all(req.as_bytes()).await.unwrap();
        let mut resp = String::new();
        stream.read_to_string(&mut resp).await.unwrap();
        resp
    }

    #[tokio::test]
    async fn health_and_auth_paths_behave() {
        let db = temp_db().await;
        let secret = "s3cret-token";
        seed_routine(&db, secret).await;

        let calls = Arc::new(AtomicUsize::new(0));
        let calls_cb = calls.clone();
        let executor: RoutineExecutor = Arc::new(move |_routine: Routine| {
            let calls = calls_cb.clone();
            Box::pin(async move {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        });

        let server = WebhookServer::start(
            db.pool().clone(),
            executor,
            WebhookConfig { bind: "127.0.0.1".into(), port: 0 },
        )
        .await
        .expect("start server");
        let addr = server.addr();

        // Health is open.
        let mut s = TcpStream::connect(addr).await.unwrap();
        s.write_all(b"GET /health HTTP/1.1\r\nConnection: close\r\n\r\n").await.unwrap();
        let mut health = String::new();
        s.read_to_string(&mut health).await.unwrap();
        assert!(health.contains("200"), "health: {health}");

        // Unknown secret → 404.
        let unknown = post(addr, "/hooks/nope", None).await;
        assert!(unknown.contains("404"), "unknown secret: {unknown}");

        // Wrong method → 405.
        let mut s = TcpStream::connect(addr).await.unwrap();
        s.write_all(b"GET /hooks/x HTTP/1.1\r\nConnection: close\r\n\r\n").await.unwrap();
        let mut getresp = String::new();
        s.read_to_string(&mut getresp).await.unwrap();
        assert!(getresp.contains("405"), "get: {getresp}");

        // Missing secret → 401.
        let noauth = post(addr, "/hooks/", None).await;
        assert!(noauth.contains("401"), "no secret: {noauth}");

        // Correct secret in path → 202 and the executor ran.
        let ok = post(addr, &format!("/hooks/{}", secret), None).await;
        assert!(ok.contains("202"), "valid path secret: {ok}");

        // Correct secret via bearer header → 202 and executor ran again.
        let ok2 = post(addr, "/hooks/does-not-matter", Some(secret)).await;
        // Path secret is checked first, so this path is unknown → 404.
        assert!(ok2.contains("404"), "path secret precedence: {ok2}");
        let ok3 = post(addr, "/hooks/", Some(secret)).await;
        assert!(ok3.contains("202"), "bearer secret: {ok3}");

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert_eq!(calls.load(Ordering::SeqCst), 2, "two valid triggers");
    }
}
