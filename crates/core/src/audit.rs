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
    NetworkRequest { url: String, method: String },
    /// File read
    FileRead { path: String },
    /// File write
    FileWrite { path: String, size: u64 },
    /// Permission granted
    PermissionGranted { permission: String },
    /// Permission denied
    PermissionDenied { permission: String, reason: String },
    /// A decision on a tool that needed approval.
    ///
    /// `ApprovalRequested` alone says a gate existed; this row says what came
    /// out of it, who or what decided, and whether the tool was allowed to run.
    /// It is the difference between "the agent tried" and "a person let it".
    ApprovalDecision {
        approval_id: String,
        tool_name: String,
        allowed: bool,
        /// "desktop", "remote", "chat", "headless", "timeout", "cancelled",
        /// "paused", "kill_switch", "startup" or "missing".
        source: String,
        note: Option<String>,
    },
    /// Bot started
    BotStarted,
    /// Bot paused
    BotPaused,
    /// Bot stopped
    BotStopped,
    /// A memory was added, edited or deleted by the user or the app.
    MemoryChanged {
        /// "agent" for a bot fact, "office" for shared office knowledge.
        scope: String,
        /// "created", "updated" or "deleted".
        action: String,
        memory_id: String,
        /// Empty for agent facts.
        category: String,
        /// Present for office memories.
        office_id: Option<String>,
    },
    /// The memory embedding backend was changed or its index rebuilt.
    MemoryEmbeddingsUpdated {
        provider: String,
        model: String,
        base_url: String,
        reindexed: u64,
    },
    /// A local account opened a workspace, including a shared workspace.
    WorkspaceAccess {
        actor: String,
        workspace: String,
        role: String,
    },
}

impl AuditEventType {
    /// Stable name used for filtering. Serde's externally-tagged names are an
    /// implementation detail; this is the value the database indexes.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::ModelCall { .. } => "ModelCall",
            Self::ToolCall { .. } => "ToolCall",
            Self::NetworkRequest { .. } => "NetworkRequest",
            Self::FileRead { .. } => "FileRead",
            Self::FileWrite { .. } => "FileWrite",
            Self::PermissionGranted { .. } => "PermissionGranted",
            Self::PermissionDenied { .. } => "PermissionDenied",
            Self::ApprovalDecision { .. } => "ApprovalDecision",
            Self::BotStarted => "BotStarted",
            Self::BotPaused => "BotPaused",
            Self::BotStopped => "BotStopped",
            Self::MemoryChanged { .. } => "MemoryChanged",
            Self::MemoryEmbeddingsUpdated { .. } => "MemoryEmbeddingsUpdated",
            Self::WorkspaceAccess { .. } => "WorkspaceAccess",
        }
    }
}

/// Every event kind the audit view can filter by.
pub const AUDIT_EVENT_KINDS: &[&str] = &[
    "ModelCall",
    "ToolCall",
    "NetworkRequest",
    "FileRead",
    "FileWrite",
    "PermissionGranted",
    "PermissionDenied",
    "ApprovalDecision",
    "BotStarted",
    "BotPaused",
    "BotStopped",
    "MemoryChanged",
    "MemoryEmbeddingsUpdated",
    "WorkspaceAccess",
];

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_has_a_kind_in_the_filter_list() {
        let events = [
            AuditEventType::ModelCall {
                provider: "openai".into(),
                model: "gpt".into(),
                tokens_in: 1,
                tokens_out: 2,
                cost: 0.1,
            },
            AuditEventType::ToolCall {
                tool_name: "read_file".into(),
                arguments: serde_json::json!({}),
            },
            AuditEventType::NetworkRequest {
                url: "https://example.com".into(),
                method: "GET".into(),
            },
            AuditEventType::FileRead {
                path: "/tmp/x".into(),
            },
            AuditEventType::FileWrite {
                path: "/tmp/x".into(),
                size: 4,
            },
            AuditEventType::PermissionGranted {
                permission: "shell".into(),
            },
            AuditEventType::PermissionDenied {
                permission: "shell".into(),
                reason: "denied".into(),
            },
            AuditEventType::ApprovalDecision {
                approval_id: Uuid::new_v4().to_string(),
                tool_name: "shell_exec".into(),
                allowed: false,
                source: "desktop".into(),
                note: Some("looks unsafe".into()),
            },
            AuditEventType::BotStarted,
            AuditEventType::BotPaused,
            AuditEventType::BotStopped,
            AuditEventType::MemoryChanged {
                scope: "office".into(),
                action: "updated".into(),
                memory_id: Uuid::new_v4().to_string(),
                category: "rule".into(),
                office_id: Some(Uuid::new_v4().to_string()),
            },
            AuditEventType::MemoryEmbeddingsUpdated {
                provider: "ollama".into(),
                model: "nomic-embed-text".into(),
                base_url: "http://127.0.0.1:11434".into(),
                reindexed: 12,
            },
        ];
        for event in events {
            assert!(
                AUDIT_EVENT_KINDS.contains(&event.kind()),
                "{} is missing from the filter list",
                event.kind()
            );
        }
    }

    #[test]
    fn event_kinds_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for kind in AUDIT_EVENT_KINDS {
            assert!(seen.insert(*kind), "duplicate audit kind {kind}");
        }
    }
}
