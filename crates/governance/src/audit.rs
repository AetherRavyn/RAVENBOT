//! Audit logging for tracking all bot actions

use ravenbot_core::{AuditEntry, AuditEventType};
use sqlx::SqlitePool;
use uuid::Uuid;

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
        let event_json = serde_json::to_string(&entry.event)
            .map_err(|e| e.to_string())?;

        sqlx::query(
            "INSERT INTO audit_log (id, bot_id, run_id, thread_id, event, timestamp)
             VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind(entry.id.to_string())
        .bind(entry.bot_id.to_string())
        .bind(entry.run_id.map(|id| id.to_string()))
        .bind(entry.thread_id.map(|id| id.to_string()))
        .bind(&event_json)
        .bind(entry.timestamp.to_rfc3339())
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

    /// Get audit log entries for a bot
    pub async fn get_entries(
        &self,
        bot_id: Uuid,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<AuditEntry>, String> {
        let rows: Vec<(String, String, Option<String>, Option<String>, String, String)> = sqlx::query_as(
            "SELECT id, bot_id, run_id, thread_id, event, timestamp
             FROM audit_log
             WHERE bot_id = ?
             ORDER BY timestamp DESC
             LIMIT ? OFFSET ?"
        )
        .bind(bot_id.to_string())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        let entries = rows.iter()
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
