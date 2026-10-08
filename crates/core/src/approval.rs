use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Per-bot approval mode — mirrors OpenMausBot's ask/auto/full levels:
/// - `ask`: risky tools pause for an Allow/Deny decision (default)
/// - `auto`: low-risk writes run free; high-risk still asks
/// - `full`: everything runs free (explicit opt-in, shown with a warning)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalMode {
    #[default]
    Ask,
    Auto,
    Full,
}

impl ApprovalMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            ApprovalMode::Ask => "ask",
            ApprovalMode::Auto => "auto",
            ApprovalMode::Full => "full",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "auto" => ApprovalMode::Auto,
            "full" => ApprovalMode::Full,
            _ => ApprovalMode::Ask,
        }
    }
}

/// Lifecycle of one approval request
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    /// Waiting for the user (run is parked)
    Pending,
    /// User allowed it — tool executed
    Allowed,
    /// User denied it — model was told and continued
    Denied,
    /// Request went stale (run finished/cancelled first)
    Expired,
}

/// One tool call parked for a user decision.
/// Persisted so a restart can't lose a pending gate, and so the transcript
/// can render the Allow/Deny card inline from the same row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    /// Unique identifier
    pub id: Uuid,
    /// Bot that wants to act
    pub bot_id: Uuid,
    /// Thread the request belongs to (card renders here)
    pub thread_id: Uuid,
    /// Run parked waiting on this decision
    pub run_id: Uuid,
    /// Tool name (e.g. "shell_exec")
    pub tool_name: String,
    /// Human label ("Run a command") for the card header
    pub tool_label: String,
    /// Arguments (rendered as the monospace detail block)
    pub arguments: serde_json::Value,
    /// Risk level at request time
    pub risk: String,
    /// Current lifecycle state
    pub status: ApprovalStatus,
    /// Optional user note attached to the decision
    pub note: Option<String>,
    /// Created timestamp
    pub created_at: DateTime<Utc>,
    /// Decided timestamp
    pub decided_at: Option<DateTime<Utc>>,
}

impl ApprovalRequest {
    pub fn pending(
        bot_id: Uuid,
        thread_id: Uuid,
        run_id: Uuid,
        tool_name: impl Into<String>,
        tool_label: impl Into<String>,
        arguments: serde_json::Value,
        risk: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            bot_id,
            thread_id,
            run_id,
            tool_name: tool_name.into(),
            tool_label: tool_label.into(),
            arguments,
            risk: risk.into(),
            status: ApprovalStatus::Pending,
            note: None,
            created_at: Utc::now(),
            decided_at: None,
        }
    }
}
