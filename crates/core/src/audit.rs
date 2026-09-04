use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use uuid::Uuid;

/// Type of audit event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditEventType {
    /// Model call made
    ModelCall {
        provider: String,
        model: String,
        tokens_in: u64,
        tokens_out: u64,
        cost: f64,
    },
    /// Tool call executed
    ToolCall {
        tool_name: String,
        arguments: serde_json::Value,
    },
    /// Network request made
    NetworkRequest {
        url: String,
        method: String,
    },
    /// File read
    FileRead {
        path: String,
    },
    /// File write
    FileWrite {
        path: String,
        size: u64,
    },
    /// Permission granted
    PermissionGranted {
        permission: String,
    },
    /// Permission denied
    PermissionDenied {
        permission: String,
        reason: String,
    },
    /// Bot started
    BotStarted,
    /// Bot paused
    BotPaused,
    /// Bot stopped
    BotStopped,
}

/// An audit log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Unique identifier
    pub id: Uuid,
    /// Bot that performed the action
    pub bot_id: Uuid,
    /// Run ID (if part of a run)
    pub run_id: Option<Uuid>,
    /// Thread ID (if part of a thread)
    pub thread_id: Option<Uuid>,
    /// Event type
    pub event: AuditEventType,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

impl AuditEntry {
    /// Create a new audit entry
    pub fn new(bot_id: Uuid, event: AuditEventType) -> Self {
        Self {
            id: Uuid::new_v4(),
            bot_id,
            run_id: None,
            thread_id: None,
            event,
            timestamp: Utc::now(),
        }
    }

    /// Set run ID
    pub fn with_run_id(mut self, run_id: Uuid) -> Self {
        self.run_id = Some(run_id);
        self
    }

    /// Set thread ID
    pub fn with_thread_id(mut self, thread_id: Uuid) -> Self {
        self.thread_id = Some(thread_id);
        self
    }
}
