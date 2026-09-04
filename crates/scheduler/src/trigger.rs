//! Event-driven triggers

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Events that can trigger routines
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TriggerEvent {
    /// Cron schedule matched
    CronSchedule { routine_id: Uuid },
    /// User sends a message
    UserMessage { bot_id: Uuid, thread_id: Uuid },
    /// Bot completes a task
    TaskCompleted { bot_id: Uuid, run_id: Uuid },
    /// System startup
    SystemStartup,
    /// Manual trigger
    Manual { user_id: Option<String> },
}

/// Event trigger configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventTrigger {
    /// Trigger ID
    pub id: Uuid,
    /// Routine to execute
    pub routine_id: Uuid,
    /// Event that triggers this
    pub event: TriggerEvent,
    /// Whether trigger is enabled
    pub enabled: bool,
}

impl EventTrigger {
    pub fn new(routine_id: Uuid, event: TriggerEvent) -> Self {
        Self {
            id: Uuid::new_v4(),
            routine_id,
            event,
            enabled: true,
        }
    }
}
