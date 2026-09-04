use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use uuid::Uuid;

/// Source of a version change
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum VersionSource {
    /// User manually edited
    User,
    /// Bot self-proposed
    BotSelf,
}

/// A version of a bot's configuration/prompt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotVersion {
    /// Unique identifier
    pub id: Uuid,
    /// Bot this version belongs to
    pub bot_id: Uuid,
    /// Version number (monotonically increasing)
    pub version_number: u32,
    /// The system prompt at this version
    pub system_prompt: String,
    /// The bot config at this version
    pub config: serde_json::Value,
    /// Who made this change
    pub source: VersionSource,
    /// Optional description of the change
    pub description: Option<String>,
    /// Created timestamp
    pub created_at: DateTime<Utc>,
}

impl BotVersion {
    /// Create a new version
    pub fn new(
        bot_id: Uuid,
        version_number: u32,
        system_prompt: impl Into<String>,
        config: serde_json::Value,
        source: VersionSource,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            bot_id,
            version_number,
            system_prompt: system_prompt.into(),
            config,
            source,
            description: None,
            created_at: Utc::now(),
        }
    }
}
