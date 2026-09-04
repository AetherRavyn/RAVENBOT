//! RAVENBOT routines (scheduled tasks)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A scheduled routine for a bot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Routine {
    /// Unique identifier
    pub id: Uuid,
    /// Bot this routine belongs to
    pub bot_id: Uuid,
    /// Human-readable name
    pub name: String,
    /// Description of what the routine does
    pub description: String,
    /// Schedule expression (cron format or simple interval)
    pub schedule: String,
    /// Instruction to give the bot when the routine runs
    pub instruction: String,
    /// Whether this routine is enabled
    pub is_enabled: bool,
    /// Last time this routine ran
    pub last_run_at: Option<DateTime<Utc>>,
    /// Created timestamp
    pub created_at: DateTime<Utc>,
    /// Last updated timestamp
    pub updated_at: DateTime<Utc>,
}

impl Routine {
    /// Create a new routine
    pub fn new(
        bot_id: Uuid,
        name: impl Into<String>,
        schedule: impl Into<String>,
        instruction: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            bot_id,
            name: name.into(),
            description: String::new(),
            schedule: schedule.into(),
            instruction: instruction.into(),
            is_enabled: true,
            last_run_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Mark as executed
    pub fn mark_executed(&mut self) {
        self.last_run_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }
}
