use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use uuid::Uuid;

/// A thread of conversation with a bot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thread {
    /// Unique identifier
    pub id: Uuid,
    /// Bot this thread belongs to
    pub bot_id: Uuid,
    /// Thread title (auto-generated or user-set)
    pub title: String,
    /// Whether this thread is currently active
    pub is_active: bool,
    /// Ephemeral/temporary thread: skip agent-memory persistence (RAG, self-review)
    #[serde(default)]
    pub ephemeral: bool,
    /// Created timestamp
    pub created_at: DateTime<Utc>,
    /// Last updated timestamp
    pub updated_at: DateTime<Utc>,
}

impl Thread {
    /// Create a new thread for a bot
    pub fn new(bot_id: Uuid, title: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            bot_id,
            title: title.into(),
            is_active: true,
            ephemeral: false,
            created_at: now,
            updated_at: now,
        }
    }

    /// Create a new ephemeral (temporary) thread for a bot
    pub fn new_ephemeral(bot_id: Uuid, title: impl Into<String>) -> Self {
        let mut thread = Self::new(bot_id, title);
        thread.ephemeral = true;
        thread
    }
}
