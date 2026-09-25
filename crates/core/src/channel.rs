use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A channel — a named working context (Work, Personal, a project) that keeps
/// its own shared instructions, working folder, and bot roster. Threads filed
/// under a channel inherit its instructions and folder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    /// Shared instructions injected into every bot's system prompt in-context.
    pub instructions: String,
    /// Default working folder for tools run in this channel.
    pub working_folder: Option<String>,
    /// Accent color for the sidebar.
    pub color: Option<String>,
    /// Sort position (lower first).
    pub position: i64,
    /// Responder policy for the channel: `{"mode":"all"|"lead"|"manual", "lead_bot_id": "…"}`.
    /// Controls which of the channel's bots react to a message.
    #[serde(default)]
    pub responder_rules: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Channel {
    pub fn new(name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            description: String::new(),
            instructions: String::new(),
            working_folder: None,
            color: None,
            position: 0,
            responder_rules: None,
            created_at: now,
            updated_at: now,
        }
    }
}
