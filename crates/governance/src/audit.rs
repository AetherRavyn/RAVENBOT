//! Audit logging for tracking all bot actions

use chrono::{DateTime, SecondsFormat, Utc};
use ravenbot_core::{AuditEntry, AuditEventType, AUDIT_EVENT_KINDS};
use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};
use uuid::Uuid;

/// Filters for the global audit view. Times are RFC 3339 strings; a missing
/// bound means "no bound". `limit` is clamped to 500 to keep the IPC payload
/// bounded even if a caller asks for everything.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct AuditQuery {
    pub bot_id: Option<String>,
    pub kinds: Vec<String>,
    pub search: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// The most entries one signed export may contain. The export fails rather
/// than silently dropping rows when the filter matches more.
pub const MAX_AUDIT_EXPORT_ENTRIES: usize = 50_000;

/// One audit entry projected for display. Deliberately flat and free of raw
/// tool arguments: the viewer is an activity record, not a secret browser.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogRow {
    pub id: String,
    pub bot_id: String,
    pub bot_name: Option<String>,
    pub run_id: Option<String>,
    pub thread_id: Option<String>,
    pub thread_title: Option<String>,
    pub kind: String,
    pub timestamp: String,
    pub tool_name: Option<String>,
    pub path: Option<String>,
    pub url: Option<String>,
    pub method: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub tokens_in: Option<u64>,
    pub tokens_out: Option<u64>,
    pub cost: Option<f64>,
    pub size: Option<u64>,
    pub permission: Option<String>,
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_scope: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub office_id: Option<String>,
}

/// A page of global audit entries plus the total matching the same filters.
#[derive(Debug, Clone, Serialize)]
pub struct AuditPage {
    pub entries: Vec<AuditLogRow>,
    pub total: u64,
    pub limit: u32,
    pub offset: u32,
}

pub const AUDIT_EXPORT_FORMAT: &str = "ravenbot-audit-export";
pub const AUDIT_EXPORT_VERSION: u32 = 1;

/// The filters that produced an export. Kept with the signed payload so a
/// reader can tell what the export does and does not cover.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuditExportFilters {
    pub bot_id: Option<String>,
    pub kinds: Vec<String>,
    pub search: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
}

/// The signed portion of an audit export. The signature covers exactly
/// `serde_json::to_vec(self)`; verification re-serializes the parsed struct,
/// the same contract used by signed bot bundles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditExportPayload {
    pub format: String,
    pub version: u32,
    pub exported_at: String,
    pub filters: AuditExportFilters,
    pub count: u64,
    pub entries: Vec<AuditLogRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditExportSignature {
    pub algorithm: String,
    pub public_key: String,
    pub signature: String,
}

/// A self-contained audit export document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditExport {
    pub payload: AuditExportPayload,
    pub signature: AuditExportSignature,
}

struct NormalizedAuditQuery {
    bot_id: Option<String>,
    kinds: Vec<&'static str>,
    search: Option<String>,
    since: Option<String>,
    until: Option<String>,
    limit: i64,
    offset: i64,
}

impl NormalizedAuditQuery {
    fn new(query: &AuditQuery) -> Result<Self, String> {
        if query.kinds.len() > 32 {
            return Err("Too many audit event kinds were requested.".to_string());
        }
        let bot_id = match query.bot_id.as_deref().map(str::trim) {
            None | Some("") => None,
            Some(value) => {
                if value.len() > 64 {
                    return Err("The audit bot filter is not a valid UUID.".to_string());
                }
                Some(
                    Uuid::parse_str(value)
                        .map_err(|_| "The audit bot filter is not a valid UUID.".to_string())?
                        .to_string(),
                )
            }
        };

        let mut kinds = Vec::new();
        for value in &query.kinds {
            let value = value.trim();
            if value.is_empty() {
                continue;
            }
            let kind = AUDIT_EVENT_KINDS
                .iter()
                .find(|kind| kind.eq_ignore_ascii_case(value))
                .copied()
                .ok_or_else(|| format!("Unknown audit event kind: {value}"))?;
            if !kinds.contains(&kind) {
                kinds.push(kind);
            }
        }

        let search = query
            .search
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| {
                if value.len() > 800 || value.chars().count() > 200 {
                    Err("The audit search is longer than 200 characters.".to_string())
                } else {
                    Ok(value.to_lowercase())
                }
            })
            .transpose()?;

        let since = canonical_time(query.since.as_deref(), "start")?;
        let until = canonical_time(query.until.as_deref(), "end")?;
        if let (Some(since), Some(until)) = (&since, &until) {
            if since > until {
                return Err("The audit start time is after the end time.".to_string());
            }
        }

        Ok(Self {
            bot_id,
            kinds,
            search,
            since,
            until,
            limit: i64::from(query.limit.unwrap_or(100).clamp(1, 500)),
            offset: i64::from(query.offset.unwrap_or(0).min(1_000_000)),
        })
    }
}

fn canonical_time(value: Option<&str>, label: &str) -> Result<Option<String>, String> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if value.len() > 64 {
        return Err(format!(
            "The audit {label} time is not an RFC 3339 timestamp."
        ));
    }
    let parsed = DateTime::parse_from_rfc3339(value)
        .map_err(|_| format!("The audit {label} time is not an RFC 3339 timestamp."))?;
    Ok(Some(
        parsed
            .with_timezone(&Utc)
            .to_rfc3339_opts(SecondsFormat::Millis, false),
    ))
}

const SEARCH_FIELDS: &[&str] = &[
    "COALESCE(b.name, '')",
    "a.bot_id",
    "COALESCE(a.run_id, '')",
    "COALESCE(a.thread_id, '')",
    "COALESCE(t.title, '')",
    "a.event_kind",
    "COALESCE(CASE WHEN json_valid(a.event) = 1 THEN json_extract(a.event, '$.ToolCall.tool_name') END, '')",
    "COALESCE(CASE WHEN json_valid(a.event) = 1 THEN json_extract(a.event, '$.FileRead.path') END, '')",
    "COALESCE(CASE WHEN json_valid(a.event) = 1 THEN json_extract(a.event, '$.FileWrite.path') END, '')",
    "COALESCE(CASE WHEN json_valid(a.event) = 1 THEN json_extract(a.event, '$.NetworkRequest.url') END, '')",
    "COALESCE(CASE WHEN json_valid(a.event) = 1 THEN json_extract(a.event, '$.NetworkRequest.method') END, '')",
    "COALESCE(CASE WHEN json_valid(a.event) = 1 THEN json_extract(a.event, '$.ModelCall.provider') END, '')",
    "COALESCE(CASE WHEN json_valid(a.event) = 1 THEN json_extract(a.event, '$.ModelCall.model') END, '')",
    "COALESCE(CASE WHEN json_valid(a.event) = 1 THEN json_extract(a.event, '$.PermissionGranted.permission') END, '')",
    "COALESCE(CASE WHEN json_valid(a.event) = 1 THEN json_extract(a.event, '$.PermissionDenied.permission') END, '')",
    "COALESCE(CASE WHEN json_valid(a.event) = 1 THEN json_extract(a.event, '$.PermissionDenied.reason') END, '')",
    "COALESCE(CASE WHEN json_valid(a.event) = 1 THEN json_extract(a.event, '$.ApprovalDecision.tool_name') END, '')",
    "COALESCE(CASE WHEN json_valid(a.event) = 1 THEN json_extract(a.event, '$.ApprovalDecision.source') END, '')",
    "COALESCE(CASE WHEN json_valid(a.event) = 1 THEN json_extract(a.event, '$.ApprovalDecision.note') END, '')",
];

fn push_clause(qb: &mut QueryBuilder<Sqlite>, first: &mut bool, clause: &str) {
    if *first {
        qb.push(" WHERE ");
        *first = false;
    } else {
        qb.push(" AND ");
    }
    qb.push(clause);
}

fn apply_audit_filters(qb: &mut QueryBuilder<Sqlite>, filter: &NormalizedAuditQuery) {
    let mut first = true;

    if let Some(bot_id) = &filter.bot_id {
        push_clause(qb, &mut first, "a.bot_id = ");
        qb.push_bind(bot_id);
    }

    if !filter.kinds.is_empty() {
        push_clause(qb, &mut first, "a.event_kind IN (");
        let mut separated = qb.separated(", ");
        for kind in &filter.kinds {
            separated.push_bind(*kind);
        }
        separated.push_unseparated(")");
    }

    if let Some(search) = &filter.search {
        push_clause(qb, &mut first, "(");
        for (index, field) in SEARCH_FIELDS.iter().enumerate() {
            if index > 0 {
                qb.push(" OR ");
            }
            qb.push("instr(lower(");
            qb.push(*field);
            qb.push("), ");
            qb.push_bind(search);
            qb.push(") > 0");
        }
        qb.push(")");
    }

    if let Some(since) = &filter.since {
        push_clause(qb, &mut first, "a.timestamp >= ");
        qb.push_bind(since);
    }

    if let Some(until) = &filter.until {
        push_clause(qb, &mut first, "a.timestamp <= ");
        qb.push_bind(until);
    }
}

/// Audit logger for recording all bot actions
pub struct AuditLogger {
    pool: SqlitePool,
}

impl AuditLogger {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Log an audit entry
    pub async fn log(&self, entry: &AuditEntry) -> Result<(), String> {
        let event_json = serde_json::to_string(&entry.event).map_err(|e| e.to_string())?;

        sqlx::query(
            "INSERT INTO audit_log (id, bot_id, run_id, thread_id, event, event_kind, timestamp)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(entry.id.to_string())
        .bind(entry.bot_id.to_string())
        .bind(entry.run_id.map(|id| id.to_string()))
        .bind(entry.thread_id.map(|id| id.to_string()))
        .bind(&event_json)
        .bind(entry.event.kind())
        .bind(
            entry
                .timestamp
                .to_rfc3339_opts(SecondsFormat::Millis, false),
        )
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        tracing::debug!(
            entry_id = %entry.id,
            bot_id = %entry.bot_id,
            "Audit entry logged"
        );

        Ok(())
    }

    /// Log a model call
    pub async fn log_model_call(
        &self,
        bot_id: Uuid,
        run_id: Option<Uuid>,
        thread_id: Option<Uuid>,
        provider: &str,
        model: &str,
        tokens_in: u64,
        tokens_out: u64,
        cost: f64,
    ) -> Result<(), String> {
        let entry = AuditEntry {
            id: Uuid::new_v4(),
            bot_id,
            run_id,
            thread_id,
            event: AuditEventType::ModelCall {
                provider: provider.to_string(),
                model: model.to_string(),
                tokens_in,
                tokens_out,
                cost,
            },
            timestamp: chrono::Utc::now(),
        };
        self.log(&entry).await
    }

    /// Log a tool call
    pub async fn log_tool_call(
        &self,
        bot_id: Uuid,
        run_id: Option<Uuid>,
        thread_id: Option<Uuid>,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<(), String> {
        let entry = AuditEntry {
            id: Uuid::new_v4(),
            bot_id,
            run_id,
            thread_id,
            event: AuditEventType::ToolCall {
                tool_name: tool_name.to_string(),
                arguments,
            },
            timestamp: chrono::Utc::now(),
        };
        self.log(&entry).await
    }

    /// Log a network request
    pub async fn log_network_request(
        &self,
        bot_id: Uuid,
        run_id: Option<Uuid>,
        thread_id: Option<Uuid>,
        url: &str,
        method: &str,
    ) -> Result<(), String> {
        let entry = AuditEntry {
            id: Uuid::new_v4(),
            bot_id,
            run_id,
            thread_id,
            event: AuditEventType::NetworkRequest {
                url: url.to_string(),
                method: method.to_string(),
            },
            timestamp: chrono::Utc::now(),
        };
        self.log(&entry).await
    }

    /// Log a file operation
    pub async fn log_file_read(
        &self,
        bot_id: Uuid,
        run_id: Option<Uuid>,
        thread_id: Option<Uuid>,
        path: &str,
    ) -> Result<(), String> {
        let entry = AuditEntry {
            id: Uuid::new_v4(),
            bot_id,
            run_id,
            thread_id,
            event: AuditEventType::FileRead {
                path: path.to_string(),
            },
            timestamp: chrono::Utc::now(),
        };
        self.log(&entry).await
    }

    /// Log a file write
    pub async fn log_file_write(
        &self,
        bot_id: Uuid,
        run_id: Option<Uuid>,
        thread_id: Option<Uuid>,
        path: &str,
        size: u64,
    ) -> Result<(), String> {
        let entry = AuditEntry {
            id: Uuid::new_v4(),
            bot_id,
            run_id,
            thread_id,
            event: AuditEventType::FileWrite {
                path: path.to_string(),
                size,
            },
            timestamp: chrono::Utc::now(),
        };
        self.log(&entry).await
    }

    /// Query the audit log across every bot.
    ///
    /// The raw event JSON can contain tool arguments, which may include
    /// credentials. This query deliberately projects only an allow-list of
    /// fields per event kind, never the arguments object.
    pub async fn query(&self, query: &AuditQuery) -> Result<AuditPage, String> {
        let filter = NormalizedAuditQuery::new(query)?;

        let mut count_qb = QueryBuilder::<Sqlite>::new(
            "SELECT COUNT(*) FROM audit_log a \
             LEFT JOIN bots b ON b.id = a.bot_id \
             LEFT JOIN threads t ON t.id = a.thread_id",
        );
        apply_audit_filters(&mut count_qb, &filter);
        let total: i64 = count_qb
            .build_query_scalar()
            .fetch_one(&self.pool)
            .await
            .map_err(|e| e.to_string())?;

        let entries = self
            .select_rows(&filter, filter.limit, filter.offset)
            .await?;

        Ok(AuditPage {
            entries,
            total: total as u64,
            limit: filter.limit as u32,
            offset: filter.offset as u32,
        })
    }

    /// Select every entry matching the filters, ignoring page/offset.
    ///
    /// The export is capped rather than silently truncated: a signed export
    /// that claims to be complete must never omit matching entries.
    pub async fn export(&self, query: &AuditQuery) -> Result<Vec<AuditLogRow>, String> {
        let filter = NormalizedAuditQuery::new(query)?;
        let rows = self
            .select_rows(&filter, MAX_AUDIT_EXPORT_ENTRIES as i64 + 1, 0)
            .await?;
        if rows.len() > MAX_AUDIT_EXPORT_ENTRIES {
            return Err(format!(
                "This export matches more than {MAX_AUDIT_EXPORT_ENTRIES} entries. Narrow the agent, event, time or search filters and try again."
            ));
        }
        Ok(rows)
    }

    /// Permanently delete audit entries older than `older_than_days`.
    ///
    /// This is an explicit user action. The caller shows the destructive
    /// confirmation; the limit prevents a malformed number from erasing the
    /// whole log by accident.
    pub async fn prune(&self, older_than_days: u32) -> Result<u64, String> {
        if !(1..=3650).contains(&older_than_days) {
            return Err("Audit retention must be between 1 and 3650 days.".to_string());
        }
        let cutoff = (Utc::now() - chrono::Duration::days(i64::from(older_than_days)))
            .to_rfc3339_opts(SecondsFormat::Millis, false);
        let result = sqlx::query("DELETE FROM audit_log WHERE timestamp < ?")
            .bind(cutoff)
            .execute(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
        Ok(result.rows_affected())
    }

    async fn select_rows(
        &self,
        filter: &NormalizedAuditQuery,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<AuditLogRow>, String> {
        let mut qb = QueryBuilder::<Sqlite>::new(
            "SELECT a.id, a.bot_id, b.name, a.run_id, a.thread_id, t.title, \
                    a.event_kind, a.event, a.timestamp \
             FROM audit_log a \
             LEFT JOIN bots b ON b.id = a.bot_id \
             LEFT JOIN threads t ON t.id = a.thread_id",
        );
        apply_audit_filters(&mut qb, filter);
        qb.push(" ORDER BY a.timestamp DESC, a.rowid DESC LIMIT ");
        qb.push_bind(limit);
        qb.push(" OFFSET ");
        qb.push_bind(offset);

        let rows: Vec<(
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            String,
            String,
            String,
        )> = qb
            .build_query_as()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(rows
            .into_iter()
            .map(
                |(
                    id,
                    bot_id,
                    bot_name,
                    run_id,
                    thread_id,
                    thread_title,
                    stored_kind,
                    event_json,
                    timestamp,
                )| {
                    let event: Option<AuditEventType> = serde_json::from_str(&event_json).ok();
                    let kind = event
                        .as_ref()
                        .map(|event| event.kind().to_string())
                        .filter(|kind| !kind.is_empty())
                        .unwrap_or(stored_kind);
                    let mut row = AuditLogRow {
                        id,
                        bot_id,
                        bot_name,
                        run_id,
                        thread_id,
                        thread_title,
                        kind,
                        timestamp,
                        tool_name: None,
                        path: None,
                        url: None,
                        method: None,
                        provider: None,
                        model: None,
                        tokens_in: None,
                        tokens_out: None,
                        cost: None,
                        size: None,
                        permission: None,
                        reason: None,
                        memory_scope: None,
                        memory_action: None,
                        memory_id: None,
                        category: None,
                        office_id: None,
                    };
                    if let Some(event) = event {
                        match event {
                            AuditEventType::ModelCall {
                                provider,
                                model,
                                tokens_in,
                                tokens_out,
                                cost,
                            } => {
                                row.provider = Some(provider);
                                row.model = Some(model);
                                row.tokens_in = Some(tokens_in);
                                row.tokens_out = Some(tokens_out);
                                row.cost = Some(cost);
                            }
                            AuditEventType::ToolCall { tool_name, .. } => {
                                // Arguments are intentionally dropped: they can
                                // contain file contents, prompts or secrets.
                                row.tool_name = Some(tool_name);
                            }
                            AuditEventType::NetworkRequest { url, method } => {
                                row.url = Some(url);
                                row.method = Some(method);
                            }
                            AuditEventType::FileRead { path } => row.path = Some(path),
                            AuditEventType::FileWrite { path, size } => {
                                row.path = Some(path);
                                row.size = Some(size);
                            }
                            AuditEventType::PermissionGranted { permission } => {
                                row.permission = Some(permission);
                            }
                            AuditEventType::PermissionDenied { permission, reason } => {
                                row.permission = Some(permission);
                                row.reason = Some(reason);
                            }
                            AuditEventType::ApprovalDecision {
                                tool_name,
                                allowed,
                                source,
                                note,
                                ..
                            } => {
                                row.tool_name = Some(tool_name);
                                row.permission =
                                    Some(if allowed { "approved" } else { "denied" }.to_string());
                                row.reason = Some(match note.filter(|v| !v.trim().is_empty()) {
                                    Some(note) => format!("{source}: {note}"),
                                    None => source,
                                });
                            }
                            AuditEventType::BotStarted
                            | AuditEventType::BotPaused
                            | AuditEventType::BotStopped => {}
                            AuditEventType::MemoryChanged {
                                scope,
                                action,
                                memory_id,
                                category,
                                office_id,
                            } => {
                                row.memory_scope = Some(scope);
                                row.memory_action = Some(action);
                                row.memory_id = Some(memory_id);
                                row.category = Some(category).filter(|value| !value.is_empty());
                                row.office_id = office_id;
                            }
                            AuditEventType::MemoryEmbeddingsUpdated {
                                provider,
                                model,
                                base_url,
                                reindexed,
                            } => {
                                row.provider = Some(provider);
                                row.model = Some(model);
                                row.url = Some(base_url);
                                row.size = Some(reindexed);
                            }
                            AuditEventType::WorkspaceAccess {
                                actor,
                                workspace,
                                role,
                            } => {
                                row.permission = Some(format!("workspace:{role}"));
                                row.reason = Some(format!("{actor} opened {workspace}"));
                            }
                        }
                    }
                    row
                },
            )
            .collect())
    }

    /// Get audit log entries for a bot
    pub async fn get_entries(
        &self,
        bot_id: Uuid,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<AuditEntry>, String> {
        let rows: Vec<(
            String,
            String,
            Option<String>,
            Option<String>,
            String,
            String,
        )> = sqlx::query_as(
            "SELECT id, bot_id, run_id, thread_id, event, timestamp
             FROM audit_log
             WHERE bot_id = ?
             ORDER BY timestamp DESC
             LIMIT ? OFFSET ?",
        )
        .bind(bot_id.to_string())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        let entries = rows
            .iter()
            .filter_map(|row| {
                let event: AuditEventType = serde_json::from_str(&row.4).ok()?;

                Some(AuditEntry {
                    id: Uuid::parse_str(&row.0).ok()?,
                    bot_id: Uuid::parse_str(&row.1).ok()?,
                    run_id: row.2.as_ref().and_then(|s| Uuid::parse_str(s).ok()),
                    thread_id: row.3.as_ref().and_then(|s| Uuid::parse_str(s).ok()),
                    event,
                    timestamp: chrono::DateTime::parse_from_rfc3339(&row.5)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now()),
                })
            })
            .collect();

        Ok(entries)
    }

    /// Get audit log count for a bot
    pub async fn count(&self, bot_id: Uuid) -> Result<u64, String> {
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_log WHERE bot_id = ?")
            .bind(bot_id.to_string())
            .fetch_one(&self.pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(row.0 as u64)
    }
}

#[cfg(test)]
mod query_tests {
    use super::*;
    use chrono::Duration;
    use ravenbot_core::{Bot, Thread};
    use ravenbot_db::queries::{BotQueries, ThreadQueries};
    use ravenbot_db::Database;
    use std::path::PathBuf;

    struct TestDb {
        db: Option<Database>,
        dir: PathBuf,
    }

    impl TestDb {
        async fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("ravenbot-audit-{}", Uuid::new_v4()));
            std::fs::create_dir_all(&dir).expect("temp audit dir");
            let db = Database::new(dir.join("audit.db"))
                .await
                .expect("audit test database");
            Self { db: Some(db), dir }
        }

        fn pool(&self) -> &SqlitePool {
            self.db.as_ref().expect("open database").pool()
        }
    }

    impl Drop for TestDb {
        fn drop(&mut self) {
            let _ = self.db.take();
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn entry(
        bot_id: Uuid,
        thread_id: Option<Uuid>,
        event: AuditEventType,
        age: Duration,
    ) -> AuditEntry {
        AuditEntry {
            id: Uuid::new_v4(),
            bot_id,
            run_id: None,
            thread_id,
            event,
            timestamp: Utc::now() - age,
        }
    }

    #[tokio::test]
    async fn global_query_orders_filters_paginates_and_redacts_arguments() {
        let test = TestDb::new().await;
        let aster = Bot::new("Aster", "audit test");
        let birch = Bot::new("Birch", "other audit test");
        BotQueries::insert(test.pool(), &aster).await.unwrap();
        BotQueries::insert(test.pool(), &birch).await.unwrap();
        let thread = Thread::new(aster.id, "Release review");
        ThreadQueries::create(test.pool(), &thread).await.unwrap();

        let logger = AuditLogger::new(test.pool().clone());
        logger
            .log(&entry(
                aster.id,
                Some(thread.id),
                AuditEventType::ToolCall {
                    tool_name: "read_file".into(),
                    arguments: serde_json::json!({
                        "path": "src/lib.rs",
                        "api_key": "sk-secret-value",
                    }),
                },
                Duration::minutes(3),
            ))
            .await
            .unwrap();
        logger
            .log(&entry(
                aster.id,
                Some(thread.id),
                AuditEventType::ModelCall {
                    provider: "openai".into(),
                    model: "gpt-5".into(),
                    tokens_in: 120,
                    tokens_out: 30,
                    cost: 0.004,
                },
                Duration::minutes(2),
            ))
            .await
            .unwrap();
        logger
            .log(&entry(
                birch.id,
                None,
                AuditEventType::FileRead {
                    path: "docs/README.md".into(),
                },
                Duration::minutes(1),
            ))
            .await
            .unwrap();

        let page = logger
            .query(&AuditQuery::default())
            .await
            .expect("global audit page");
        assert_eq!(page.total, 3);
        assert_eq!(page.entries.len(), 3);
        assert_eq!(page.entries[0].kind, "FileRead");
        assert_eq!(page.entries[1].kind, "ModelCall");
        assert_eq!(page.entries[2].kind, "ToolCall");
        assert_eq!(page.entries[2].bot_name.as_deref(), Some("Aster"));
        assert_eq!(
            page.entries[2].thread_title.as_deref(),
            Some("Release review")
        );
        assert_eq!(page.entries[2].tool_name.as_deref(), Some("read_file"));
        assert_eq!(page.entries[1].tokens_in, Some(120));
        assert_eq!(page.entries[1].cost, Some(0.004));
        let serialized = serde_json::to_string(&page.entries).unwrap();
        assert!(!serialized.contains("sk-secret-value"));
        assert!(!serialized.contains("api_key"));

        let tools = logger
            .query(&AuditQuery {
                kinds: vec!["ToolCall".into()],
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(tools.total, 1);
        assert_eq!(tools.entries[0].tool_name.as_deref(), Some("read_file"));

        let by_bot = logger
            .query(&AuditQuery {
                bot_id: Some(birch.id.to_string()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(by_bot.total, 1);
        assert_eq!(by_bot.entries[0].kind, "FileRead");

        let by_thread_title = logger
            .query(&AuditQuery {
                search: Some("Release".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(by_thread_title.total, 2);

        let by_path = logger
            .query(&AuditQuery {
                search: Some("README".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(by_path.total, 1);
        assert_eq!(by_path.entries[0].path.as_deref(), Some("docs/README.md"));

        // Search must not reach into tool arguments, even when the value is
        // present in the stored JSON.
        let by_secret = logger
            .query(&AuditQuery {
                search: Some("sk-secret-value".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(by_secret.total, 0);

        let page_two = logger
            .query(&AuditQuery {
                limit: Some(1),
                offset: Some(1),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(page_two.total, 3);
        assert_eq!(page_two.entries.len(), 1);
        assert_eq!(page_two.entries[0].kind, "ModelCall");

        let now = Utc::now();
        let window = logger
            .query(&AuditQuery {
                since: Some((now - Duration::minutes(2) - Duration::seconds(30)).to_rfc3339()),
                until: Some((now - Duration::minutes(1) - Duration::seconds(30)).to_rfc3339()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(window.total, 1);
        assert_eq!(window.entries[0].kind, "ModelCall");
    }

    #[tokio::test]
    async fn export_ignores_pagination_and_prune_only_removes_old_rows() {
        let test = TestDb::new().await;
        let bot = Bot::new("Cedar", "export test");
        BotQueries::insert(test.pool(), &bot).await.unwrap();
        let logger = AuditLogger::new(test.pool().clone());
        logger
            .log(&entry(
                bot.id,
                None,
                AuditEventType::ToolCall {
                    tool_name: "old_tool".into(),
                    arguments: serde_json::json!({}),
                },
                Duration::days(10),
            ))
            .await
            .unwrap();
        logger
            .log(&entry(
                bot.id,
                None,
                AuditEventType::ToolCall {
                    tool_name: "new_tool".into(),
                    arguments: serde_json::json!({}),
                },
                Duration::days(1),
            ))
            .await
            .unwrap();

        let exported = logger
            .export(&AuditQuery {
                limit: Some(1),
                offset: Some(1),
                kinds: vec!["ToolCall".into()],
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(exported.len(), 2, "export must ignore the page size");
        assert_eq!(exported[0].tool_name.as_deref(), Some("new_tool"));
        assert_eq!(exported[1].tool_name.as_deref(), Some("old_tool"));

        assert!(logger.prune(0).await.is_err());
        assert!(logger.prune(3651).await.is_err());
        assert_eq!(logger.prune(5).await.unwrap(), 1);
        let remaining = logger.query(&AuditQuery::default()).await.unwrap();
        assert_eq!(remaining.total, 1);
        assert_eq!(remaining.entries[0].tool_name.as_deref(), Some("new_tool"));
    }

    #[tokio::test]
    async fn memory_changes_are_queryable_and_carry_no_content() {
        let test = TestDb::new().await;
        let bot = Bot::new("Memory Bot", "audit test");
        BotQueries::insert(test.pool(), &bot).await.unwrap();
        let logger = AuditLogger::new(test.pool().clone());
        logger
            .log(&entry(
                bot.id,
                None,
                AuditEventType::MemoryChanged {
                    scope: "office".into(),
                    action: "updated".into(),
                    memory_id: "memory-1".into(),
                    category: "rule".into(),
                    office_id: Some("office-1".into()),
                },
                Duration::minutes(1),
            ))
            .await
            .unwrap();

        let page = logger
            .query(&AuditQuery {
                kinds: vec!["MemoryChanged".into()],
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(page.total, 1);
        let row = &page.entries[0];
        assert_eq!(row.kind, "MemoryChanged");
        assert_eq!(row.memory_scope.as_deref(), Some("office"));
        assert_eq!(row.memory_action.as_deref(), Some("updated"));
        assert_eq!(row.memory_id.as_deref(), Some("memory-1"));
        assert_eq!(row.category.as_deref(), Some("rule"));
        assert_eq!(row.office_id.as_deref(), Some("office-1"));
        assert!(row.bot_name.as_deref() == Some("Memory Bot"));
    }

    #[tokio::test]
    async fn embedding_changes_are_queryable() {
        let test = TestDb::new().await;
        let logger = AuditLogger::new(test.pool().clone());
        logger
            .log(&entry(
                Uuid::nil(),
                None,
                AuditEventType::MemoryEmbeddingsUpdated {
                    provider: "ollama".into(),
                    model: "nomic-embed-text".into(),
                    base_url: "http://127.0.0.1:11434".into(),
                    reindexed: 42,
                },
                Duration::minutes(1),
            ))
            .await
            .unwrap();

        let page = logger
            .query(&AuditQuery {
                kinds: vec!["MemoryEmbeddingsUpdated".into()],
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(page.total, 1);
        let row = &page.entries[0];
        assert_eq!(row.kind, "MemoryEmbeddingsUpdated");
        assert_eq!(row.provider.as_deref(), Some("ollama"));
        assert_eq!(row.model.as_deref(), Some("nomic-embed-text"));
        assert_eq!(row.url.as_deref(), Some("http://127.0.0.1:11434"));
        assert_eq!(row.size, Some(42));
    }

    #[tokio::test]
    async fn global_query_rejects_bad_filters_and_clamps_the_page() {
        let logger = AuditLogger::new(
            sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .expect("in-memory pool"),
        );
        // The table is absent, but validation happens before any query.
        assert!(logger
            .query(&AuditQuery {
                bot_id: Some("not-a-uuid".into()),
                ..Default::default()
            })
            .await
            .unwrap_err()
            .contains("UUID"));
        assert!(logger
            .query(&AuditQuery {
                kinds: vec!["MadeUp".into()],
                ..Default::default()
            })
            .await
            .unwrap_err()
            .contains("Unknown audit event kind"));
        assert!(logger
            .query(&AuditQuery {
                since: Some("yesterday".into()),
                ..Default::default()
            })
            .await
            .unwrap_err()
            .contains("RFC 3339"));
        assert!(logger
            .query(&AuditQuery {
                since: Some("2026-01-02T00:00:00Z".into()),
                until: Some("2026-01-01T00:00:00Z".into()),
                ..Default::default()
            })
            .await
            .unwrap_err()
            .contains("after the end"));
    }

    #[tokio::test]
    async fn approval_decisions_project_the_tool_outcome_and_source() {
        let test = TestDb::new().await;
        let bot = Bot::new("Approver", "audit approval test");
        BotQueries::insert(test.pool(), &bot).await.unwrap();
        let logger = AuditLogger::new(test.pool().clone());
        logger
            .log(&entry(
                bot.id,
                None,
                AuditEventType::ApprovalDecision {
                    approval_id: Uuid::new_v4().to_string(),
                    tool_name: "shell_exec".into(),
                    allowed: false,
                    source: "desktop".into(),
                    note: Some("too risky".into()),
                },
                Duration::minutes(1),
            ))
            .await
            .unwrap();

        let page = logger
            .query(&AuditQuery {
                search: Some("shell_exec".into()),
                ..Default::default()
            })
            .await
            .expect("approval decision is searchable by tool");
        assert_eq!(page.total, 1);
        let row = &page.entries[0];
        assert_eq!(row.kind, "ApprovalDecision");
        assert_eq!(row.tool_name.as_deref(), Some("shell_exec"));
        assert_eq!(row.permission.as_deref(), Some("denied"));
        assert_eq!(row.reason.as_deref(), Some("desktop: too risky"));
    }
}
