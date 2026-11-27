//! Loopback OTLP collector tests for the enterprise export plane.
//!
//! These use an in-process axum server on an ephemeral port. No external
//! network is touched, and the collector records the exact request bodies so
//! the payload and cursor semantics are evidence rather than assumptions.

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use ravenbot_core::{AuditEntry, AuditEventType, Bot, Run, RunOutcome, Thread};
use ravenbot_db::queries::{BotQueries, RunQueries, ThreadQueries};
use ravenbot_db::Database;
use ravenbot_governance::audit::AuditLogger;
use ravenbot_governance::otel::{load_status, run_export, send_test, OtelConfig, KEY_AUDIT_CURSOR};
use serde_json::Value;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Clone)]
struct Recorded {
    authorization: Option<String>,
    api_key: Option<String>,
    body: Value,
}

#[derive(Clone)]
struct CollectorState {
    requests: Arc<Mutex<Vec<Recorded>>>,
    status: Arc<AtomicU16>,
    response_body: Arc<Mutex<String>>,
}

struct Collector {
    addr: SocketAddr,
    state: CollectorState,
    handle: tokio::task::JoinHandle<()>,
}

impl Collector {
    async fn start() -> Self {
        let state = CollectorState {
            requests: Arc::new(Mutex::new(Vec::new())),
            status: Arc::new(AtomicU16::new(200)),
            response_body: Arc::new(Mutex::new(String::new())),
        };
        let app = Router::new()
            .route("/v1/logs", post(collect))
            .with_state(state.clone());
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("ephemeral collector port");
        let addr = listener.local_addr().expect("collector address");
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Self {
            addr,
            state,
            handle,
        }
    }

    fn endpoint(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn requests(&self) -> Vec<Recorded> {
        self.state.requests.lock().expect("requests lock").clone()
    }

    fn fail_with(&self, status: u16, body: &str) {
        self.state.status.store(status, Ordering::SeqCst);
        *self.state.response_body.lock().expect("response lock") = body.to_string();
    }

    fn succeed(&self) {
        self.state.status.store(200, Ordering::SeqCst);
        self.state
            .response_body
            .lock()
            .expect("response lock")
            .clear();
    }
}

impl Drop for Collector {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

async fn collect(
    State(state): State<CollectorState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let read = |name: &str| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string)
    };
    state
        .requests
        .lock()
        .expect("requests lock")
        .push(Recorded {
            authorization: read("authorization"),
            api_key: read("x-api-key"),
            body,
        });
    let status = StatusCode::from_u16(state.status.load(Ordering::SeqCst))
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let body = state.response_body.lock().expect("response lock").clone();
    (status, Json(serde_json::json!({ "detail": body })))
}

struct TestDb {
    db: Option<Database>,
    dir: PathBuf,
}

impl TestDb {
    async fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("ravenbot-otel-{name}-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("temp export dir");
        let db = Database::new(dir.join("otel.db"))
            .await
            .expect("otel test database");
        Self { db: Some(db), dir }
    }

    fn pool(&self) -> &sqlx::SqlitePool {
        self.db.as_ref().expect("open database").pool()
    }
}

impl Drop for TestDb {
    fn drop(&mut self) {
        let _ = self.db.take();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn config(collector: &Collector) -> OtelConfig {
    OtelConfig {
        enabled: true,
        endpoint: collector.endpoint(),
        service_name: "ravenbot-test".to_string(),
        interval_secs: 60,
        batch_size: 200,
        headers: BTreeMap::from([(
            "Authorization".to_string(),
            "Bearer header-secret".to_string(),
        )]),
    }
}

async fn seed_audit(
    pool: &sqlx::SqlitePool,
    bot_id: Uuid,
    thread_id: Uuid,
    run_id: Option<Uuid>,
    event: AuditEventType,
) -> Uuid {
    let entry = AuditEntry {
        id: Uuid::new_v4(),
        bot_id,
        run_id,
        thread_id: Some(thread_id),
        event,
        timestamp: chrono::Utc::now(),
    };
    AuditLogger::new(pool.clone())
        .log(&entry)
        .await
        .expect("audit row");
    entry.id
}

fn scopes(body: &Value) -> &Vec<Value> {
    body["resourceLogs"][0]["scopeLogs"]
        .as_array()
        .expect("scope logs")
}

fn records<'a>(body: &'a Value, scope_name: &str) -> &'a Vec<Value> {
    scopes(body)
        .iter()
        .find(|scope| scope["scope"]["name"] == scope_name)
        .and_then(|scope| scope["logRecords"].as_array())
        .unwrap_or_else(|| panic!("missing scope {scope_name}"))
}

fn attr<'a>(record: &'a Value, key: &str) -> Option<&'a Value> {
    record["attributes"]
        .as_array()?
        .iter()
        .find(|attribute| attribute["key"] == key)
        .map(|attribute| &attribute["value"])
}

#[tokio::test]
async fn exports_audit_and_run_rows_once_and_advances_both_cursors() {
    let db = TestDb::new("once").await;
    let collector = Collector::start().await;
    let bot = Bot::new("Atlas", "enterprise export test");
    BotQueries::insert(db.pool(), &bot).await.expect("bot");
    let thread = Thread::new(bot.id, "Quarterly acquisition plan from the first message");
    ThreadQueries::create(db.pool(), &thread)
        .await
        .expect("thread");

    let mut run = Run::new(bot.id, thread.id);
    run.add_model_usage(42, 0.4);
    run.add_tool_usage(0.1);
    run.complete(RunOutcome::Failure {
        error: "secret failure text that must not leave".to_string(),
    });
    RunQueries::insert(db.pool(), &run).await.expect("run");
    RunQueries::set_actor(
        db.pool(),
        run.id,
        "actor-1",
        "alice",
        "operator",
        "workspace-1",
    )
    .await
    .expect("actor");

    let tool_audit = seed_audit(
        db.pool(),
        bot.id,
        thread.id,
        Some(run.id),
        AuditEventType::ToolCall {
            tool_name: "read_file".to_string(),
            arguments: serde_json::json!({
                "path": "docs/plan.md",
                "prompt": "private prompt text",
                "api_key": "sk-tool-secret",
            }),
        },
    )
    .await;
    let network_audit = seed_audit(
        db.pool(),
        bot.id,
        thread.id,
        Some(run.id),
        AuditEventType::NetworkRequest {
            url: "https://api.example.com/v1/items?token=super-secret&q=private#frag".to_string(),
            method: "GET".to_string(),
        },
    )
    .await;
    seed_audit(
        db.pool(),
        bot.id,
        thread.id,
        Some(run.id),
        AuditEventType::ApprovalDecision {
            approval_id: Uuid::new_v4().to_string(),
            tool_name: "shell_exec".to_string(),
            allowed: false,
            source: "desktop".to_string(),
            note: Some("approval note: the private acquisition plan".to_string()),
        },
    )
    .await;

    let report = run_export(db.pool(), config(&collector))
        .await
        .expect("first export");
    assert!(report.attempted);
    assert!(report.delivered);
    assert_eq!(report.audit_count, 3);
    assert_eq!(report.run_count, 1);
    assert_eq!(report.status_code, Some(200));
    assert!(report.error.is_none());

    let requests = collector.requests();
    assert_eq!(requests.len(), 1, "one batch means one HTTP request");
    assert_eq!(
        requests[0].authorization.as_deref(),
        Some("Bearer header-secret")
    );
    let body = &requests[0].body;
    assert_eq!(
        body["resourceLogs"][0]["resource"]["attributes"][0]["value"]["stringValue"],
        "ravenbot-test"
    );

    let audit_records = records(body, "ravenbot.audit");
    assert_eq!(audit_records.len(), 3);
    assert_eq!(audit_records[0]["body"]["stringValue"], "ToolCall");
    assert!(audit_records[0]["timeUnixNano"].is_string());
    assert_eq!(
        attr(&audit_records[0], "tool.name").unwrap()["stringValue"],
        "read_file"
    );
    assert_eq!(
        attr(&audit_records[0], "actor.name").unwrap()["stringValue"],
        "alice"
    );
    assert_eq!(
        attr(&audit_records[1], "http.url").unwrap()["stringValue"],
        "https://api.example.com/v1/items"
    );

    let run_records = records(body, "ravenbot.runs");
    assert_eq!(run_records.len(), 1);
    assert_eq!(run_records[0]["body"]["stringValue"], "run.failed");
    assert_eq!(
        attr(&run_records[0], "run.outcome").unwrap()["stringValue"],
        "Failure"
    );
    assert_eq!(
        attr(&run_records[0], "run.tokens").unwrap()["intValue"],
        "42"
    );
    assert_eq!(
        attr(&run_records[0], "run.cost_estimate").unwrap()["doubleValue"],
        0.5
    );

    let serialized = serde_json::to_string(body).unwrap();
    for private in [
        "Quarterly acquisition plan",
        "private prompt text",
        "sk-tool-secret",
        "super-secret",
        "token=",
        "#frag",
        "approval note: the private acquisition plan",
        "secret failure text",
        "checkpoint",
    ] {
        assert!(
            !serialized.contains(private),
            "payload leaked {private:?}: {serialized}"
        );
    }
    assert!(serialized.contains(&tool_audit.to_string()));
    assert!(serialized.contains(&network_audit.to_string()));

    let status = load_status(db.pool()).await.expect("status");
    let first_audit_cursor = status.audit_cursor;
    assert!(first_audit_cursor > 0);
    assert_eq!(
        status.run_cursor.as_ref().map(|cursor| cursor.id.clone()),
        Some(run.id.to_string())
    );
    assert_eq!(status.last_audit_count, 3);
    assert_eq!(status.last_run_count, 1);
    assert!(status.last_success_at.is_some());

    // A second export has no new rows and must not open a request.
    let second = run_export(db.pool(), config(&collector))
        .await
        .expect("second export");
    assert!(!second.attempted);
    assert_eq!(second.audit_count, 0);
    assert_eq!(second.run_count, 0);
    assert!(second.error.is_none());
    assert_eq!(collector.requests().len(), 1, "empty batch sends nothing");

    let unchanged = load_status(db.pool()).await.expect("status after empty");
    assert_eq!(unchanged.audit_cursor, first_audit_cursor);
    assert_eq!(unchanged.last_audit_count, 3);
}

#[tokio::test]
async fn batch_size_limits_each_request_and_advances_the_cursor() {
    let db = TestDb::new("batch").await;
    let collector = Collector::start().await;
    let bot = Bot::new("Batching", "enterprise export test");
    BotQueries::insert(db.pool(), &bot).await.expect("bot");
    let thread = Thread::new(bot.id, "Batch thread");
    ThreadQueries::create(db.pool(), &thread)
        .await
        .expect("thread");

    for index in 0..3 {
        seed_audit(
            db.pool(),
            bot.id,
            thread.id,
            None,
            AuditEventType::FileRead {
                path: format!("docs/{index}.md"),
            },
        )
        .await;
    }

    let mut limited = config(&collector);
    limited.batch_size = 2;
    let first = run_export(db.pool(), limited.clone())
        .await
        .expect("first limited export");
    assert_eq!(first.audit_count, 2);
    assert_eq!(
        records(&collector.requests()[0].body, "ravenbot.audit").len(),
        2
    );

    let cursor: String = sqlx::query_scalar("SELECT value FROM app_settings WHERE key = ?")
        .bind(KEY_AUDIT_CURSOR)
        .fetch_one(db.pool())
        .await
        .expect("cursor row");
    assert!(cursor.parse::<i64>().unwrap() > 0);

    let second = run_export(db.pool(), limited)
        .await
        .expect("second limited export");
    assert_eq!(second.audit_count, 1);
    assert_eq!(collector.requests().len(), 2);
    assert_eq!(
        records(&collector.requests()[1].body, "ravenbot.audit").len(),
        1
    );
}

#[tokio::test]
async fn a_failed_batch_keeps_the_cursor_and_retries_the_same_events_once() {
    let db = TestDb::new("retry").await;
    let collector = Collector::start().await;
    let bot = Bot::new("Retry", "enterprise export test");
    BotQueries::insert(db.pool(), &bot).await.expect("bot");
    let thread = Thread::new(bot.id, "Retry thread");
    ThreadQueries::create(db.pool(), &thread)
        .await
        .expect("thread");
    let audit_id = seed_audit(
        db.pool(),
        bot.id,
        thread.id,
        None,
        AuditEventType::FileWrite {
            path: "docs/result.md".to_string(),
            size: 512,
        },
    )
    .await;

    collector.fail_with(500, "collector is down");
    let failed = run_export(db.pool(), config(&collector))
        .await
        .expect("failed export returns a report");
    assert!(failed.attempted);
    assert!(!failed.delivered);
    assert_eq!(failed.status_code, Some(500));
    assert!(failed.error.as_deref().unwrap_or("").contains("HTTP 500"));
    assert_eq!(failed.audit_count, 1);

    let after_failure = load_status(db.pool()).await.expect("status");
    assert_eq!(after_failure.audit_cursor, 0, "failure must not advance");
    assert!(after_failure.last_error.is_some());
    assert_eq!(collector.requests().len(), 1);

    collector.succeed();
    let retried = run_export(db.pool(), config(&collector))
        .await
        .expect("retry export");
    assert!(retried.delivered);
    assert_eq!(retried.audit_count, 1);
    assert_eq!(collector.requests().len(), 2);
    let retried_body = serde_json::to_string(&collector.requests()[1].body).unwrap();
    assert_eq!(retried_body.matches(&audit_id.to_string()).count(), 1);

    let after_success = load_status(db.pool()).await.expect("status after retry");
    assert!(after_success.audit_cursor > 0);
    assert!(after_success.last_error.is_none());
    assert!(after_success.last_success_at.is_some());

    let empty = run_export(db.pool(), config(&collector))
        .await
        .expect("after success");
    assert!(!empty.attempted);
    assert_eq!(collector.requests().len(), 2);
}

#[tokio::test]
async fn header_values_are_sent_and_redacted_from_an_echoing_error_body() {
    let db = TestDb::new("headers").await;
    let collector = Collector::start().await;
    let bot = Bot::new("Header Bot", "enterprise export test");
    BotQueries::insert(db.pool(), &bot).await.expect("bot");
    let thread = Thread::new(bot.id, "Header thread");
    ThreadQueries::create(db.pool(), &thread)
        .await
        .expect("thread");
    seed_audit(
        db.pool(),
        bot.id,
        thread.id,
        None,
        AuditEventType::ToolCall {
            tool_name: "read_file".to_string(),
            arguments: serde_json::json!({}),
        },
    )
    .await;
    let mut configuration = config(&collector);
    configuration.headers =
        BTreeMap::from([("X-Api-Key".to_string(), "sk-header-secret".to_string())]);
    collector.fail_with(500, "you sent sk-header-secret");

    let report = run_export(db.pool(), configuration).await.expect("report");
    assert!(!report.delivered);
    let error = report.error.expect("error text");
    assert!(!error.contains("sk-header-secret"));
    assert!(error.contains("[redacted]"));
    assert_eq!(
        collector.requests()[0].api_key.as_deref(),
        Some("sk-header-secret")
    );
}

#[tokio::test]
async fn the_synthetic_test_log_is_labelled_and_does_not_move_cursors() {
    let db = TestDb::new("synthetic").await;
    let collector = Collector::start().await;
    let report = send_test(db.pool(), config(&collector))
        .await
        .expect("test connection");
    assert!(report.attempted);
    assert!(report.delivered);
    assert_eq!(report.audit_count, 1);

    let body = &collector.requests()[0].body;
    let test_records = records(body, "ravenbot.export");
    assert_eq!(test_records.len(), 1);
    assert_eq!(
        test_records[0]["body"]["stringValue"],
        "ravenbot.export.test"
    );
    assert_eq!(
        attr(&test_records[0], "test.synthetic").unwrap()["boolValue"],
        true
    );

    let status = load_status(db.pool()).await.expect("status");
    assert_eq!(status.audit_cursor, 0);
    assert!(status.run_cursor.is_none());
    assert!(status.last_success_at.is_none());
}
