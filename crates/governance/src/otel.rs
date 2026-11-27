//! Opt-in enterprise export over OTLP/HTTP JSON logs.
//!
//! This is deliberately a *metadata* stream, not a transcript. It exports the
//! same allow-listed audit projection the viewer shows minus free text that can
//! carry user content (thread titles, approval notes and reasons), plus run
//! lifecycle rows without their checkpoint, result or error text.
//!
//! Delivery is cursor-based: a successful batch advances `audit_log.rowid` and
//! `(runs.updated_at, runs.id)` only after the collector accepts it. A crash
//! between the two cursor writes can re-send a batch (duplicates); it cannot
//! skip one. The trade-off is stated in `docs/ENTERPRISE-EXPORT.md`.

use crate::audit::{AuditLogRow, AuditLogger};
use chrono::{DateTime, SecondsFormat, Utc};
use ravenbot_db::queries::AppSettingsQueries;
use ravenbot_db::secrets;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::BTreeMap;
use std::time::Duration;
use uuid::Uuid;

pub const KEY_ENABLED: &str = "otel_export_enabled";
pub const KEY_ENDPOINT: &str = "otel_export_endpoint";
pub const KEY_SERVICE_NAME: &str = "otel_export_service_name";
pub const KEY_INTERVAL_SECS: &str = "otel_export_interval_secs";
pub const KEY_BATCH_SIZE: &str = "otel_export_batch_size";
pub const KEY_AUDIT_CURSOR: &str = "otel_export_audit_cursor";
pub const KEY_RUN_CURSOR: &str = "otel_export_run_cursor";
pub const KEY_LAST_ATTEMPT: &str = "otel_export_last_attempt_at";
pub const KEY_LAST_SUCCESS: &str = "otel_export_last_success_at";
pub const KEY_LAST_ERROR: &str = "otel_export_last_error";
pub const KEY_LAST_AUDIT_COUNT: &str = "otel_export_last_audit_count";
pub const KEY_LAST_RUN_COUNT: &str = "otel_export_last_run_count";
/// App-setting marker for the keychain entry. The value itself is never here.
pub const KEY_HEADERS_MARKER: &str = "otel_export_headers";

pub const DEFAULT_SERVICE_NAME: &str = "ravenbot";
pub const DEFAULT_INTERVAL_SECS: u32 = 60;
pub const DEFAULT_BATCH_SIZE: u32 = 200;
pub const MIN_INTERVAL_SECS: u32 = 10;
pub const MAX_INTERVAL_SECS: u32 = 3600;
pub const MIN_BATCH_SIZE: u32 = 1;
pub const MAX_BATCH_SIZE: u32 = 500;
pub const MAX_ENDPOINT_CHARS: usize = 2048;
pub const MAX_HEADERS: usize = 20;
pub const MAX_HEADER_VALUE_BYTES: usize = 4096;
const MAX_ERROR_CHARS: usize = 1000;
const MAX_RESPONSE_BODY_CHARS: usize = 512;
const REQUEST_TIMEOUT_SECS: u64 = 10;

/// Runtime export configuration. Deliberately not `Serialize` or `Debug`:
/// header values are credentials and must never cross the IPC boundary.
#[derive(Clone)]
pub struct OtelConfig {
    pub enabled: bool,
    pub endpoint: String,
    pub service_name: String,
    pub interval_secs: u32,
    pub batch_size: u32,
    pub headers: BTreeMap<String, String>,
}

/// Safe status for the Settings card. Header values are represented only by
/// `headers_configured`.
#[derive(Debug, Clone, Serialize)]
pub struct OtelStatus {
    pub enabled: bool,
    pub endpoint: String,
    pub service_name: String,
    pub interval_secs: u32,
    pub batch_size: u32,
    pub headers_configured: bool,
    pub audit_cursor: i64,
    pub run_cursor: Option<RunCursor>,
    pub last_attempt_at: Option<String>,
    pub last_success_at: Option<String>,
    pub last_error: Option<String>,
    pub last_audit_count: u64,
    pub last_run_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunCursor {
    pub updated_at: String,
    pub id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OtelExportReport {
    pub attempted: bool,
    pub audit_count: u64,
    pub run_count: u64,
    pub delivered: bool,
    pub endpoint: String,
    pub status_code: Option<u16>,
    pub error: Option<String>,
    pub next_retry_in_secs: Option<u64>,
}

struct RunExportRow {
    id: String,
    bot_id: String,
    thread_id: String,
    state: String,
    outcome: Option<String>,
    tokens: u64,
    cost_estimate: f64,
    model_cost: f64,
    tool_cost: f64,
    created_at: String,
    updated_at: String,
    completed_at: Option<String>,
    actor_name: Option<String>,
    actor_role: Option<String>,
}

/// Validate and normalize a collector endpoint.
///
/// A bare host becomes `…/v1/logs`; a path already ending in `/v1/logs` is
/// kept. Plain HTTP is accepted only for the three documented loopback hosts.
pub fn normalize_endpoint(raw: &str) -> Result<String, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("Choose a collector endpoint.".to_string());
    }
    if raw.chars().count() > MAX_ENDPOINT_CHARS {
        return Err("The collector endpoint is longer than 2048 characters.".to_string());
    }
    let mut url = reqwest::Url::parse(raw)
        .map_err(|_| "The collector endpoint is not a valid URL.".to_string())?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err("The collector endpoint must not include a username or password.".to_string());
    }
    match url.scheme() {
        "https" => {}
        "http" if is_loopback_host(url.host_str()) => {}
        "http" => {
            return Err(
                "Use HTTPS for a remote collector. Plain HTTP is allowed only for localhost, 127.0.0.1 or ::1."
                    .to_string(),
            )
        }
        _ => return Err("The collector endpoint must use http or https.".to_string()),
    }
    if url.path().is_empty() || url.path() == "/" {
        url.set_path("/v1/logs");
    }
    url.set_fragment(None);
    let normalized = url.to_string();
    if normalized.chars().count() > MAX_ENDPOINT_CHARS {
        return Err("The collector endpoint is longer than 2048 characters.".to_string());
    }
    Ok(normalized)
}

fn is_loopback_host(host: Option<&str>) -> bool {
    matches!(host, Some("127.0.0.1") | Some("localhost") | Some("[::1]"))
}

pub fn validate_service_name(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Give the export service a name.".to_string());
    }
    if trimmed.chars().count() > 64 || trimmed.chars().any(char::is_control) {
        return Err("The export service name must be 1–64 printable characters.".to_string());
    }
    Ok(trimmed.to_string())
}

pub fn validate_interval_secs(value: u32) -> Result<(), String> {
    if !(MIN_INTERVAL_SECS..=MAX_INTERVAL_SECS).contains(&value) {
        return Err(format!(
            "The export interval must be between {MIN_INTERVAL_SECS} and {MAX_INTERVAL_SECS} seconds."
        ));
    }
    Ok(())
}

pub fn validate_batch_size(value: u32) -> Result<(), String> {
    if !(MIN_BATCH_SIZE..=MAX_BATCH_SIZE).contains(&value) {
        return Err(format!(
            "The export batch size must be between {MIN_BATCH_SIZE} and {MAX_BATCH_SIZE}."
        ));
    }
    Ok(())
}

/// Parse the write-only header JSON object from Settings.
///
/// Values are validated but never echoed in an error message.
pub fn parse_headers(raw: &str) -> Result<BTreeMap<String, String>, String> {
    if raw.trim().is_empty() {
        return Ok(BTreeMap::new());
    }
    let value: serde_json::Value = serde_json::from_str(raw)
        .map_err(|error| format!("The export headers are not valid JSON: {error}"))?;
    let object = value.as_object().ok_or_else(|| {
        "The export headers must be a JSON object of name/value strings.".to_string()
    })?;
    if object.len() > MAX_HEADERS {
        return Err(format!("At most {MAX_HEADERS} export headers are allowed."));
    }
    let mut headers = BTreeMap::new();
    for (name, value) in object {
        let name = name.trim();
        if !valid_header_name(name) {
            return Err(
                "An export header name is invalid; use only letters, digits and hyphens."
                    .to_string(),
            );
        }
        let value = value
            .as_str()
            .ok_or_else(|| "Every export header value must be a string.".to_string())?;
        if value.len() > MAX_HEADER_VALUE_BYTES {
            return Err("An export header value is larger than 4 KiB.".to_string());
        }
        if value.contains('\r') || value.contains('\n') {
            return Err("An export header value must not contain a line break.".to_string());
        }
        headers.insert(name.to_string(), value.to_string());
    }
    Ok(headers)
}

fn valid_header_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

async fn setting(pool: &SqlitePool, key: &str) -> Result<Option<String>, String> {
    AppSettingsQueries::get(pool, key)
        .await
        .map_err(|error| error.to_string())
}

async fn setting_or_default(pool: &SqlitePool, key: &str, default: &str) -> Result<String, String> {
    Ok(setting(pool, key)
        .await?
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| default.to_string()))
}

fn parsed_u32(raw: Option<String>, default: u32, min: u32, max: u32) -> u32 {
    raw.and_then(|value| value.trim().parse::<u32>().ok())
        .map(|value| value.clamp(min, max))
        .unwrap_or(default)
}

fn parsed_bool(raw: Option<String>) -> bool {
    matches!(
        raw.as_deref().map(str::trim),
        Some("1") | Some("true") | Some("TRUE")
    )
}

async fn headers_configured(pool: &SqlitePool) -> Result<bool, String> {
    let marker = setting(pool, KEY_HEADERS_MARKER).await?;
    if marker.as_deref() == Some(ravenbot_db::secrets::KEYCHAIN_MARKER) {
        return Ok(true);
    }
    Ok(secrets::get_otel_export_headers()
        .await
        .map(|raw| {
            let trimmed = raw.trim();
            !trimmed.is_empty() && trimmed != "{}"
        })
        .unwrap_or(false))
}

/// Read the saved configuration, including keychain-backed headers.
pub async fn load_config(pool: &SqlitePool) -> Result<OtelConfig, String> {
    let enabled = parsed_bool(setting(pool, KEY_ENABLED).await?);
    let endpoint = setting_or_default(pool, KEY_ENDPOINT, "").await?;
    let service_name = validate_service_name(
        &setting(pool, KEY_SERVICE_NAME)
            .await?
            .unwrap_or_else(|| DEFAULT_SERVICE_NAME.to_string()),
    )
    .unwrap_or_else(|_| DEFAULT_SERVICE_NAME.to_string());
    let interval_secs = parsed_u32(
        setting(pool, KEY_INTERVAL_SECS).await?,
        DEFAULT_INTERVAL_SECS,
        MIN_INTERVAL_SECS,
        MAX_INTERVAL_SECS,
    );
    let batch_size = parsed_u32(
        setting(pool, KEY_BATCH_SIZE).await?,
        DEFAULT_BATCH_SIZE,
        MIN_BATCH_SIZE,
        MAX_BATCH_SIZE,
    );
    let headers = match secrets::get_otel_export_headers().await {
        Some(raw) if !raw.trim().is_empty() => parse_headers(&raw)?,
        _ => BTreeMap::new(),
    };
    Ok(OtelConfig {
        enabled,
        endpoint,
        service_name,
        interval_secs,
        batch_size,
        headers,
    })
}

/// Read the safe status shown in Settings. Never includes header values.
pub async fn load_status(pool: &SqlitePool) -> Result<OtelStatus, String> {
    let config = load_config(pool).await?;
    let audit_cursor = setting(pool, KEY_AUDIT_CURSOR)
        .await?
        .and_then(|value| value.trim().parse::<i64>().ok())
        .unwrap_or(0)
        .max(0);
    let run_cursor = setting(pool, KEY_RUN_CURSOR)
        .await?
        .and_then(|value| serde_json::from_str::<RunCursor>(&value).ok());
    let last_audit_count = setting(pool, KEY_LAST_AUDIT_COUNT)
        .await?
        .and_then(|value| value.trim().parse::<i64>().ok())
        .unwrap_or(0)
        .max(0) as u64;
    let last_run_count = setting(pool, KEY_LAST_RUN_COUNT)
        .await?
        .and_then(|value| value.trim().parse::<i64>().ok())
        .unwrap_or(0)
        .max(0) as u64;
    Ok(OtelStatus {
        enabled: config.enabled,
        endpoint: config.endpoint,
        service_name: config.service_name,
        interval_secs: config.interval_secs,
        batch_size: config.batch_size,
        headers_configured: headers_configured(pool).await?,
        audit_cursor,
        run_cursor,
        last_attempt_at: setting(pool, KEY_LAST_ATTEMPT)
            .await?
            .filter(|value| !value.trim().is_empty()),
        last_success_at: setting(pool, KEY_LAST_SUCCESS)
            .await?
            .filter(|value| !value.trim().is_empty()),
        last_error: setting(pool, KEY_LAST_ERROR)
            .await?
            .filter(|value| !value.trim().is_empty()),
        last_audit_count,
        last_run_count,
    })
}

/// Whether the configured interval has elapsed since the last attempt.
pub fn export_due(interval_secs: u32, last_attempt_at: Option<&str>, now: DateTime<Utc>) -> bool {
    let Some(last_attempt_at) = last_attempt_at else {
        return true;
    };
    let Ok(last_attempt_at) = DateTime::parse_from_rfc3339(last_attempt_at) else {
        return true;
    };
    now.signed_duration_since(last_attempt_at.with_timezone(&Utc))
        .num_seconds()
        >= i64::from(interval_secs.clamp(MIN_INTERVAL_SECS, MAX_INTERVAL_SECS))
}

fn now_string() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, false)
}

fn clip_error(error: &str) -> String {
    error.chars().take(MAX_ERROR_CHARS).collect()
}

/// Persist an attempted export that failed before a report could be built.
/// The loop calls this when the saved configuration itself is unreadable, so a
/// broken setup is retried on the configured cadence instead of every tick.
pub async fn record_failure(pool: &SqlitePool, error: &str) {
    let now = now_string();
    let _ = AppSettingsQueries::set(pool, KEY_LAST_ATTEMPT, &now).await;
    let _ = AppSettingsQueries::set(pool, KEY_LAST_ERROR, &clip_error(error)).await;
}

async fn record_attempt(pool: &SqlitePool) {
    let _ = AppSettingsQueries::set(pool, KEY_LAST_ATTEMPT, &now_string()).await;
}

async fn persist_success(
    pool: &SqlitePool,
    audit_cursor: i64,
    run_cursor: &RunCursor,
    audit_count: u64,
    run_count: u64,
) -> Result<(), String> {
    AppSettingsQueries::set(pool, KEY_AUDIT_CURSOR, &audit_cursor.to_string())
        .await
        .map_err(|error| error.to_string())?;
    AppSettingsQueries::set(
        pool,
        KEY_RUN_CURSOR,
        &serde_json::to_string(run_cursor).map_err(|error| error.to_string())?,
    )
    .await
    .map_err(|error| error.to_string())?;
    AppSettingsQueries::set(pool, KEY_LAST_AUDIT_COUNT, &audit_count.to_string())
        .await
        .map_err(|error| error.to_string())?;
    AppSettingsQueries::set(pool, KEY_LAST_RUN_COUNT, &run_count.to_string())
        .await
        .map_err(|error| error.to_string())?;
    let now = now_string();
    AppSettingsQueries::set(pool, KEY_LAST_ATTEMPT, &now)
        .await
        .map_err(|error| error.to_string())?;
    AppSettingsQueries::set(pool, KEY_LAST_SUCCESS, &now)
        .await
        .map_err(|error| error.to_string())?;
    AppSettingsQueries::set(pool, KEY_LAST_ERROR, "")
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// Deliver every audit and run row after its cursor, then advance the cursors.
///
/// A transport or collector failure is returned inside the report (with the
/// cursors untouched) so the caller can show the clipped error. Configuration
/// or database failures return `Err` after recording an attempt.
pub async fn run_export(pool: &SqlitePool, config: OtelConfig) -> Result<OtelExportReport, String> {
    let endpoint = match normalize_endpoint(&config.endpoint) {
        Ok(endpoint) => endpoint,
        Err(error) => {
            record_failure(pool, &error).await;
            return Err(error);
        }
    };
    let batch_size = i64::from(config.batch_size.clamp(MIN_BATCH_SIZE, MAX_BATCH_SIZE));
    let audit_cursor = match setting(pool, KEY_AUDIT_CURSOR).await {
        Ok(value) => value
            .and_then(|value| value.trim().parse::<i64>().ok())
            .unwrap_or(0)
            .max(0),
        Err(error) => {
            let message = format!("Could not read the audit export cursor: {error}");
            record_failure(pool, &message).await;
            return Err(message);
        }
    };
    let run_cursor = match setting(pool, KEY_RUN_CURSOR).await {
        Ok(value) => value.and_then(|value| serde_json::from_str::<RunCursor>(&value).ok()),
        Err(error) => {
            let message = format!("Could not read the run export cursor: {error}");
            record_failure(pool, &message).await;
            return Err(message);
        }
    };

    let audit_rows = match AuditLogger::new(pool.clone())
        .rows_after_rowid(audit_cursor, batch_size)
        .await
    {
        Ok(rows) => rows,
        Err(error) => {
            let message = format!("Could not read the audit log for export: {error}");
            record_failure(pool, &message).await;
            return Err(message);
        }
    };
    let run_rows = match select_run_rows(pool, run_cursor.as_ref(), batch_size).await {
        Ok(rows) => rows,
        Err(error) => {
            let message = format!("Could not read runs for export: {error}");
            record_failure(pool, &message).await;
            return Err(message);
        }
    };

    let audit_count = audit_rows.len() as u64;
    let run_count = run_rows.len() as u64;
    if audit_rows.is_empty() && run_rows.is_empty() {
        // Nothing to send. Recording the attempt keeps the background loop on
        // the configured cadence, but no success or error is fabricated.
        record_attempt(pool).await;
        return Ok(OtelExportReport {
            attempted: false,
            audit_count: 0,
            run_count: 0,
            delivered: false,
            endpoint,
            status_code: None,
            error: None,
            next_retry_in_secs: None,
        });
    }

    let next_audit_cursor = audit_rows
        .last()
        .map(|(rowid, _)| *rowid)
        .unwrap_or(audit_cursor);
    let next_run_cursor = run_rows
        .last()
        .map(|row| RunCursor {
            updated_at: row.updated_at.clone(),
            id: row.id.clone(),
        })
        .or(run_cursor)
        .unwrap_or_else(|| RunCursor {
            updated_at: String::new(),
            id: String::new(),
        });
    let payload = build_payload(&config.service_name, &audit_rows, &run_rows);

    match deliver(&config, &endpoint, &payload).await {
        Ok(status_code) => {
            if let Err(error) = persist_success(
                pool,
                next_audit_cursor,
                &next_run_cursor,
                audit_count,
                run_count,
            )
            .await
            {
                let message = format!(
                    "The batch was delivered but its cursor could not be saved: {error}. The next export may duplicate it."
                );
                record_failure(pool, &message).await;
                return Err(message);
            }
            Ok(OtelExportReport {
                attempted: true,
                audit_count,
                run_count,
                delivered: true,
                endpoint,
                status_code: Some(status_code),
                error: None,
                next_retry_in_secs: None,
            })
        }
        Err((status_code, error)) => {
            record_failure(pool, &error).await;
            Ok(OtelExportReport {
                attempted: true,
                audit_count,
                run_count,
                delivered: false,
                endpoint,
                status_code,
                error: Some(error),
                next_retry_in_secs: Some(u64::from(
                    config
                        .interval_secs
                        .clamp(MIN_INTERVAL_SECS, MAX_INTERVAL_SECS),
                )),
            })
        }
    }
}

/// Send one synthetic log record to prove connectivity. Cursors are untouched.
pub async fn send_test(_pool: &SqlitePool, config: OtelConfig) -> Result<OtelExportReport, String> {
    let endpoint = normalize_endpoint(&config.endpoint)?;
    let payload = build_test_payload(&config.service_name);
    match deliver(&config, &endpoint, &payload).await {
        Ok(status_code) => Ok(OtelExportReport {
            attempted: true,
            audit_count: 1,
            run_count: 0,
            delivered: true,
            endpoint,
            status_code: Some(status_code),
            error: None,
            next_retry_in_secs: None,
        }),
        Err((status_code, error)) => Ok(OtelExportReport {
            attempted: true,
            audit_count: 1,
            run_count: 0,
            delivered: false,
            endpoint,
            status_code,
            error: Some(error),
            next_retry_in_secs: None,
        }),
    }
}

async fn select_run_rows(
    pool: &SqlitePool,
    cursor: Option<&RunCursor>,
    limit: i64,
) -> Result<Vec<RunExportRow>, String> {
    let (updated_at, id) = cursor
        .map(|cursor| (cursor.updated_at.as_str(), cursor.id.as_str()))
        .unwrap_or(("", ""));
    let rows: Vec<(
        String,
        String,
        String,
        String,
        Option<String>,
        i64,
        f64,
        f64,
        f64,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT r.id, r.bot_id, r.thread_id, r.state, r.outcome, r.tokens_consumed, \
                r.cost_estimate, r.model_cost, r.tool_cost, r.created_at, r.updated_at, \
                r.completed_at, ra.actor_name, ra.role \
         FROM runs r \
         LEFT JOIN run_actors ra ON ra.run_id = r.id \
         WHERE r.updated_at > ? OR (r.updated_at = ? AND r.id > ?) \
         ORDER BY r.updated_at ASC, r.id ASC LIMIT ?",
    )
    .bind(updated_at)
    .bind(updated_at)
    .bind(id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;

    Ok(rows
        .into_iter()
        .map(
            |(
                id,
                bot_id,
                thread_id,
                state,
                outcome,
                tokens,
                cost_estimate,
                model_cost,
                tool_cost,
                created_at,
                updated_at,
                completed_at,
                actor_name,
                actor_role,
            )| RunExportRow {
                id,
                bot_id,
                thread_id,
                state,
                outcome: outcome.as_deref().and_then(outcome_variant),
                tokens: tokens.max(0) as u64,
                cost_estimate,
                model_cost,
                tool_cost,
                created_at,
                updated_at,
                completed_at,
                actor_name,
                actor_role,
            },
        )
        .collect())
}

/// Keep only the serde variant name. The result, error or cancel reason stored
/// inside the outcome can contain user text and is never exported.
fn outcome_variant(raw: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(raw.trim()).ok()?;
    let object = value.as_object()?;
    ["Success", "PartialSuccess", "Failure", "Cancelled"]
        .iter()
        .find(|variant| object.contains_key(**variant))
        .map(|variant| (*variant).to_string())
}

fn unix_nano(raw: &str) -> Option<String> {
    DateTime::parse_from_rfc3339(raw)
        .ok()?
        .timestamp_nanos_opt()
        .map(|nanos| nanos.to_string())
}

fn str_attr(key: &str, value: impl Into<String>) -> serde_json::Value {
    serde_json::json!({
        "key": key,
        "value": { "stringValue": value.into() }
    })
}

fn int_attr(key: &str, value: i64) -> serde_json::Value {
    serde_json::json!({
        "key": key,
        "value": { "intValue": value.to_string() }
    })
}

fn double_attr(key: &str, value: f64) -> serde_json::Value {
    serde_json::json!({
        "key": key,
        "value": { "doubleValue": value }
    })
}

fn bool_attr(key: &str, value: bool) -> serde_json::Value {
    serde_json::json!({
        "key": key,
        "value": { "boolValue": value }
    })
}

/// Strip credentials that routinely live in a query string or fragment.
/// Unparseable URLs are omitted rather than exported raw.
fn sanitize_url(raw: &str) -> Option<String> {
    let url = reqwest::Url::parse(raw.trim()).ok()?;
    let host = url.host_str()?;
    let mut out = format!("{}://{host}", url.scheme());
    if let Some(port) = url.port() {
        out.push(':');
        out.push_str(&port.to_string());
    }
    let path = url.path();
    if !path.is_empty() {
        out.push_str(path);
    }
    Some(out)
}

fn audit_record(rowid: i64, row: &AuditLogRow) -> serde_json::Value {
    let mut attributes = vec![
        str_attr("audit.id", row.id.clone()),
        int_attr("audit.rowid", rowid),
        str_attr("audit.kind", row.kind.clone()),
        str_attr("bot.id", row.bot_id.clone()),
    ];
    if let Some(value) = &row.bot_name {
        attributes.push(str_attr("bot.name", value.clone()));
    }
    if let Some(value) = &row.run_id {
        attributes.push(str_attr("run.id", value.clone()));
    }
    if let Some(value) = &row.thread_id {
        attributes.push(str_attr("thread.id", value.clone()));
    }
    if let Some(value) = &row.tool_name {
        attributes.push(str_attr("tool.name", value.clone()));
    }
    if let Some(value) = &row.path {
        attributes.push(str_attr("file.path", value.clone()));
    }
    if let Some(value) = row.url.as_deref().and_then(sanitize_url) {
        attributes.push(str_attr("http.url", value));
    }
    if let Some(value) = &row.method {
        attributes.push(str_attr("http.method", value.clone()));
    }
    if let Some(value) = &row.provider {
        attributes.push(str_attr("model.provider", value.clone()));
    }
    if let Some(value) = &row.model {
        attributes.push(str_attr("model.name", value.clone()));
    }
    if let Some(value) = row.tokens_in {
        attributes.push(int_attr("model.tokens_in", value as i64));
    }
    if let Some(value) = row.tokens_out {
        attributes.push(int_attr("model.tokens_out", value as i64));
    }
    if let Some(value) = row.cost {
        attributes.push(double_attr("model.cost", value));
    }
    if let Some(value) = row.size {
        attributes.push(int_attr("file.size", value as i64));
    }
    if let Some(value) = &row.permission {
        attributes.push(str_attr("permission", value.clone()));
    }
    if let Some(value) = &row.actor_name {
        attributes.push(str_attr("actor.name", value.clone()));
    }
    if let Some(value) = &row.actor_role {
        attributes.push(str_attr("actor.role", value.clone()));
    }
    if let Some(value) = &row.memory_scope {
        attributes.push(str_attr("memory.scope", value.clone()));
    }
    if let Some(value) = &row.memory_action {
        attributes.push(str_attr("memory.action", value.clone()));
    }
    if let Some(value) = &row.memory_id {
        attributes.push(str_attr("memory.id", value.clone()));
    }
    if let Some(value) = &row.category {
        attributes.push(str_attr("memory.category", value.clone()));
    }
    if let Some(value) = &row.office_id {
        attributes.push(str_attr("office.id", value.clone()));
    }

    let mut record = serde_json::json!({
        "severityText": "INFO",
        "body": { "stringValue": row.kind }
    });
    if let Some(nanos) = unix_nano(&row.timestamp) {
        record["timeUnixNano"] = serde_json::Value::String(nanos);
    }
    record["attributes"] = serde_json::Value::Array(attributes);
    record
}

fn run_record(row: &RunExportRow) -> serde_json::Value {
    let mut attributes = vec![
        str_attr("run.id", row.id.clone()),
        str_attr("run.bot_id", row.bot_id.clone()),
        str_attr("run.thread_id", row.thread_id.clone()),
        str_attr("run.state", row.state.clone()),
        int_attr("run.tokens", row.tokens as i64),
        double_attr("run.cost_estimate", row.cost_estimate),
        double_attr("run.model_cost", row.model_cost),
        double_attr("run.tool_cost", row.tool_cost),
        str_attr("run.created_at", row.created_at.clone()),
        str_attr("run.updated_at", row.updated_at.clone()),
    ];
    if let Some(value) = &row.outcome {
        attributes.push(str_attr("run.outcome", value.clone()));
    }
    if let Some(value) = &row.completed_at {
        attributes.push(str_attr("run.completed_at", value.clone()));
    }
    if let Some(value) = &row.actor_name {
        attributes.push(str_attr("actor.name", value.clone()));
    }
    if let Some(value) = &row.actor_role {
        attributes.push(str_attr("actor.role", value.clone()));
    }

    let mut record = serde_json::json!({
        "severityText": "INFO",
        "body": { "stringValue": format!("run.{}", row.state) }
    });
    if let Some(nanos) = unix_nano(&row.updated_at) {
        record["timeUnixNano"] = serde_json::Value::String(nanos);
    }
    record["attributes"] = serde_json::Value::Array(attributes);
    record
}

fn build_payload(
    service_name: &str,
    audit_rows: &[(i64, AuditLogRow)],
    run_rows: &[RunExportRow],
) -> serde_json::Value {
    let audit_records: Vec<_> = audit_rows
        .iter()
        .map(|(rowid, row)| audit_record(*rowid, row))
        .collect();
    let run_records: Vec<_> = run_rows.iter().map(run_record).collect();
    serde_json::json!({
        "resourceLogs": [{
            "resource": {
                "attributes": [
                    str_attr("service.name", service_name),
                    str_attr("service.version", env!("CARGO_PKG_VERSION"))
                ]
            },
            "scopeLogs": [
                {
                    "scope": { "name": "ravenbot.audit" },
                    "logRecords": audit_records
                },
                {
                    "scope": { "name": "ravenbot.runs" },
                    "logRecords": run_records
                }
            ]
        }]
    })
}

fn build_test_payload(service_name: &str) -> serde_json::Value {
    let record = serde_json::json!({
        "timeUnixNano": Utc::now().timestamp_nanos_opt().map(|n| n.to_string()),
        "severityText": "INFO",
        "body": { "stringValue": "ravenbot.export.test" },
        "attributes": [
            str_attr("test.id", Uuid::new_v4().to_string()),
            bool_attr("test.synthetic", true),
            str_attr("test.service_name", service_name)
        ]
    });
    serde_json::json!({
        "resourceLogs": [{
            "resource": {
                "attributes": [
                    str_attr("service.name", service_name),
                    str_attr("service.version", env!("CARGO_PKG_VERSION"))
                ]
            },
            "scopeLogs": [{
                "scope": { "name": "ravenbot.export" },
                "logRecords": [record]
            }]
        }]
    })
}

/// Remove every configured header value from an error message. Response
/// bodies are attacker-controlled and some collectors echo request headers.
fn redact_headers(message: &str, headers: &BTreeMap<String, String>) -> String {
    let mut redacted = message.to_string();
    for value in headers.values().filter(|value| !value.is_empty()) {
        redacted = redacted.replace(value, "[redacted]");
    }
    redacted
}

async fn deliver(
    config: &OtelConfig,
    endpoint: &str,
    payload: &serde_json::Value,
) -> Result<u16, (Option<u16>, String)> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()
        .map_err(|error| {
            (
                None,
                redact_headers(
                    &format!("Could not prepare the export request: {error}"),
                    &config.headers,
                ),
            )
        })?;
    let mut request = client
        .post(endpoint)
        .header(reqwest::header::CONTENT_TYPE, "application/json");
    for (name, value) in &config.headers {
        let header_name = reqwest::header::HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| (None, "An export header name is invalid.".to_string()))?;
        let header_value = reqwest::header::HeaderValue::from_str(value)
            .map_err(|_| (None, "An export header value is invalid.".to_string()))?;
        request = request.header(header_name, header_value);
    }

    let response = match request.json(payload).send().await {
        Ok(response) => response,
        Err(error) => {
            return Err((
                None,
                redact_headers(
                    &format!("Could not reach the collector: {error}"),
                    &config.headers,
                ),
            ))
        }
    };
    let status = response.status();
    if status.is_success() {
        return Ok(status.as_u16());
    }
    let body = response.text().await.unwrap_or_default();
    let body: String = body.chars().take(MAX_RESPONSE_BODY_CHARS).collect();
    let message = if body.trim().is_empty() {
        format!("Collector returned HTTP {}.", status.as_u16())
    } else {
        format!("Collector returned HTTP {}: {body}", status.as_u16())
    };
    Err((
        Some(status.as_u16()),
        redact_headers(&message, &config.headers),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audit_row(kind: &str) -> AuditLogRow {
        AuditLogRow {
            id: "audit-1".to_string(),
            bot_id: "bot-1".to_string(),
            bot_name: Some("Atlas".to_string()),
            run_id: Some("run-1".to_string()),
            thread_id: Some("thread-1".to_string()),
            thread_title: Some("Quarterly acquisition plan".to_string()),
            kind: kind.to_string(),
            timestamp: "2026-01-02T03:04:05.678Z".to_string(),
            tool_name: Some("read_file".to_string()),
            path: Some("docs/plan.md".to_string()),
            url: Some("https://example.com/v1/items?token=sk-live#section".to_string()),
            method: Some("GET".to_string()),
            provider: Some("openai".to_string()),
            model: Some("gpt-5".to_string()),
            tokens_in: Some(120),
            tokens_out: Some(30),
            cost: Some(0.004),
            size: Some(4096),
            permission: Some("approved".to_string()),
            reason: Some("note from alice: ship it".to_string()),
            actor_name: Some("alice".to_string()),
            actor_role: Some("operator".to_string()),
            memory_scope: Some("office".to_string()),
            memory_action: Some("updated".to_string()),
            memory_id: Some("memory-1".to_string()),
            category: Some("rule".to_string()),
            office_id: Some("office-1".to_string()),
        }
    }

    fn run_row() -> RunExportRow {
        RunExportRow {
            id: "run-1".to_string(),
            bot_id: "bot-1".to_string(),
            thread_id: "thread-1".to_string(),
            state: "failed".to_string(),
            outcome: Some("Failure".to_string()),
            tokens: 42,
            cost_estimate: 0.5,
            model_cost: 0.4,
            tool_cost: 0.1,
            created_at: "2026-01-02T03:00:00Z".to_string(),
            updated_at: "2026-01-02T03:05:00Z".to_string(),
            completed_at: Some("2026-01-02T03:05:00Z".to_string()),
            actor_name: Some("alice".to_string()),
            actor_role: Some("operator".to_string()),
        }
    }

    #[test]
    fn endpoints_normalize_and_require_transport_security() {
        assert_eq!(
            normalize_endpoint("https://otel.example.com").unwrap(),
            "https://otel.example.com/v1/logs"
        );
        assert_eq!(
            normalize_endpoint("https://otel.example.com/").unwrap(),
            "https://otel.example.com/v1/logs"
        );
        assert_eq!(
            normalize_endpoint("https://otel.example.com/v1/logs").unwrap(),
            "https://otel.example.com/v1/logs"
        );
        assert_eq!(
            normalize_endpoint("https://otel.example.com/collector").unwrap(),
            "https://otel.example.com/collector"
        );
        assert!(normalize_endpoint("http://127.0.0.1:4318").is_ok());
        assert!(normalize_endpoint("http://localhost:4318/v1/logs").is_ok());
        assert!(normalize_endpoint("http://[::1]:4318").is_ok());
        assert!(normalize_endpoint("http://otel.example.com").is_err());
        assert!(normalize_endpoint("ftp://otel.example.com").is_err());
        assert!(normalize_endpoint("https://user:pass@otel.example.com").is_err());
        assert!(normalize_endpoint("  ").is_err());
        assert!(normalize_endpoint(&format!(
            "https://otel.example.com/{}",
            "a".repeat(MAX_ENDPOINT_CHARS)
        ))
        .is_err());
    }

    #[test]
    fn header_names_values_and_bounds_are_validated_without_echoing_values() {
        let parsed =
            parse_headers(r#"{"Authorization":"Bearer super-secret","X-Org":"acme"}"#).unwrap();
        assert_eq!(parsed.get("Authorization").unwrap(), "Bearer super-secret");
        assert!(parse_headers(r#"{"Bad Name":"x"}"#).is_err());
        assert!(parse_headers(r#"{"X-Test":"line\nbreak"}"#).is_err());
        assert!(parse_headers(r#"{"X-Test":42}"#).is_err());
        assert!(parse_headers("[1,2]").is_err());
        assert!(parse_headers(&format!(
            r#"{{"X-Test":"{}"}}"#,
            "a".repeat(MAX_HEADER_VALUE_BYTES + 1)
        ))
        .is_err());
        let too_many = (0..=MAX_HEADERS)
            .map(|index| format!("\"X-{index}\":\"v\""))
            .collect::<Vec<_>>()
            .join(",");
        let error = parse_headers(&format!("{{{too_many}}}")).unwrap_err();
        assert!(!error.contains("\"v\""));
    }

    #[test]
    fn payload_has_safe_audit_and_run_attributes_and_no_user_text() {
        let payload = build_payload("ravenbot-test", &[(7, audit_row("ToolCall"))], &[run_row()]);
        let serialized = serde_json::to_string(&payload).unwrap();

        assert!(serialized.contains("ravenbot.audit"));
        assert!(serialized.contains("ravenbot.runs"));
        assert!(serialized.contains("\"audit.rowid\""));
        assert!(serialized.contains("\"7\""));
        assert!(serialized.contains("\"tool.name\""));
        assert!(serialized.contains("\"permission\""));
        assert!(serialized.contains("\"memory.scope\""));
        assert!(serialized.contains("\"run.outcome\""));
        assert!(serialized.contains("https://example.com/v1/items"));

        // Never exported: thread titles, free-text reasons, URL queries or
        // fragments, outcome text, prompts, tool arguments and checkpoints.
        assert!(!serialized.contains("Quarterly acquisition plan"));
        assert!(!serialized.contains("note from alice"));
        assert!(!serialized.contains("token=sk-live"));
        assert!(!serialized.contains("#section"));
        assert!(!serialized.contains("checkpoint"));
        assert!(!serialized.contains("secret failure text"));
    }

    #[test]
    fn an_unparseable_timestamp_is_omitted_instead_of_fabricated() {
        let mut row = audit_row("ToolCall");
        row.timestamp = "yesterday".to_string();
        let payload = serde_json::to_value(audit_record(3, &row)).unwrap();
        assert!(payload.get("timeUnixNano").is_none());
        assert_eq!(payload["body"]["stringValue"], "ToolCall");
    }

    #[test]
    fn outcome_keeps_only_the_variant_name() {
        let raw = r#"{"Failure":{"error":"the private thing broke"}}"#;
        assert_eq!(outcome_variant(raw).as_deref(), Some("Failure"));
        assert_eq!(
            outcome_variant(r#"{"PartialSuccess":{"result":"secret","errors":["x"]}}"#).as_deref(),
            Some("PartialSuccess")
        );
        assert!(outcome_variant(r#"{"MadeUp":{}}"#).is_none());
        assert!(outcome_variant("not json").is_none());
    }

    #[test]
    fn url_sanitization_drops_credentials_query_and_fragment() {
        assert_eq!(
            sanitize_url("https://example.com/a/b?token=secret#frag").as_deref(),
            Some("https://example.com/a/b")
        );
        assert_eq!(
            sanitize_url("http://127.0.0.1:11434/api?x=1").as_deref(),
            Some("http://127.0.0.1:11434/api")
        );
        assert!(sanitize_url("not a url").is_none());
    }

    #[test]
    fn errors_redact_every_configured_header_value() {
        let headers = BTreeMap::from([
            (
                "Authorization".to_string(),
                "Bearer super-secret".to_string(),
            ),
            ("X-Org".to_string(), "acme".to_string()),
        ]);
        let message = "collector echoed Bearer super-secret for acme";
        let redacted = redact_headers(message, &headers);
        assert!(!redacted.contains("super-secret"));
        assert!(redacted.contains("[redacted]"));
    }

    #[test]
    fn cadence_is_due_on_first_run_and_after_the_interval() {
        let now = Utc::now();
        assert!(export_due(60, None, now));
        assert!(export_due(60, Some("not a timestamp"), now));
        assert!(!export_due(
            60,
            Some(&(now - chrono::Duration::seconds(30)).to_rfc3339()),
            now
        ));
        assert!(export_due(
            60,
            Some(&(now - chrono::Duration::seconds(61)).to_rfc3339()),
            now
        ));
    }

    #[test]
    fn synthetic_payload_is_labelled_and_has_no_audit_fields() {
        let payload = build_test_payload("ravenbot-test");
        let serialized = serde_json::to_string(&payload).unwrap();
        assert!(serialized.contains("ravenbot.export.test"));
        assert!(serialized.contains("\"test.synthetic\""));
        assert!(serialized.contains("\"boolValue\":true"));
        assert!(!serialized.contains("\"audit.id\""));
        assert!(!serialized.contains("ravenbot.audit"));
    }
}
