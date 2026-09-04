//! RAVENBOT IPC contract
//!
//! This crate defines the typed commands and events that flow between
//! the Rust backend and the UI frontend.

use ravenbot_core::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// Commands (UI -> Backend)

/// Commands that the UI can send to the backend
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command")]
pub enum Command {
    // Bot commands
    /// Create a new bot
    CreateBot {
        name: String,
        description: String,
    },
    /// Get all bots
    ListBots,
    /// Get a bot by ID
    GetBot {
        bot_id: Uuid,
    },
    /// Update a bot
    UpdateBot {
        bot: Bot,
    },
    /// Delete a bot
    DeleteBot {
        bot_id: Uuid,
    },

    // Thread commands
    /// Create a new thread
    CreateThread {
        bot_id: Uuid,
        title: String,
    },
    /// Get threads for a bot
    ListThreads {
        bot_id: Uuid,
    },
    /// Get a thread
    GetThread {
        thread_id: Uuid,
    },

    // Message commands
    /// Get messages in a thread
    ListMessages {
        thread_id: Uuid,
    },
    /// Send a message to a bot
    SendMessage {
        thread_id: Uuid,
        content: String,
    },
    /// Send a multimodal message (with attachments)
    SendMessageWithAttachments {
        thread_id: Uuid,
        content: String,
        attachments: Vec<Attachment>,
    },

    // Run commands
    /// Start or resume a run
    StartRun {
        run_id: Uuid,
    },
    /// Pause a run
    PauseRun {
        run_id: Uuid,
    },
    /// Cancel a run
    CancelRun {
        run_id: Uuid,
    },
    /// Get run status
    GetRun {
        run_id: Uuid,
    },

    // Budget commands
    /// Set a bot's budget
    SetBudget {
        bot_id: Uuid,
        budget: Budget,
    },
    /// Get a bot's budget
    GetBudget {
        bot_id: Uuid,
    },
    /// Get current usage
    GetUsage {
        bot_id: Uuid,
    },

    // Memory commands
    /// Search memory
    SearchMemory {
        bot_id: Uuid,
        query: String,
    },
    /// Add a memory fact
    AddMemory {
        bot_id: Uuid,
        content: String,
    },

    // Skill commands
    /// Get available skills
    ListSkills,
    /// Enable a skill for a bot
    EnableSkill {
        bot_id: Uuid,
        skill_id: String,
    },
    /// Disable a skill for a bot
    DisableSkill {
        bot_id: Uuid,
        skill_id: String,
    },

    // Audit commands
    /// Get audit log
    GetAuditLog {
        bot_id: Option<Uuid>,
        limit: Option<u32>,
    },

    // Version commands
    /// Get bot version history
    GetVersionHistory {
        bot_id: Uuid,
    },
    /// Rollback to a previous version
    RollbackVersion {
        bot_id: Uuid,
        version_id: Uuid,
    },

    // Bundle commands
    /// Export a bot
    ExportBot {
        bot_id: Uuid,
        include_memory: bool,
    },
    /// Import a bot
    ImportBot {
        bundle: BotBundle,
    },

    // Global commands
    /// Global kill switch - pause all bots
    PauseAll,
    /// Resume all paused bots
    ResumeAll,
    /// Get app status
    GetStatus,
}

/// Events that the backend can send to the UI
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event")]
pub enum Event {
    // Bot events
    /// Bot created
    BotCreated { bot: Bot },
    /// Bot updated
    BotUpdated { bot: Bot },
    /// Bot deleted
    BotDeleted { bot_id: Uuid },
    /// Bot status changed
    BotStatusChanged {
        bot_id: Uuid,
        status: BotStatus,
    },

    // Thread events
    /// Thread created
    ThreadCreated { thread: Thread },

    // Message events
    /// New message
    MessageAdded { message: Message },
    /// Message updated (e.g., checklist status changed)
    MessageUpdated { message: Message },

    // Run events
    /// Run started
    RunStarted { run: Run },
    /// Run state changed
    RunStateChanged { run: Run },
    /// Run completed
    RunCompleted { run: Run },

    // Budget events
    /// Budget warning (approaching limit)
    BudgetWarning {
        bot_id: Uuid,
        remaining: f64,
    },
    /// Budget exceeded
    BudgetExceeded {
        bot_id: Uuid,
    },

    // Audit events
    /// Audit entry added
    AuditEntry { entry: AuditEntry },

    // System events
    /// Error occurred
    Error {
        message: String,
        code: Option<String>,
    },
    /// Status update
    StatusUpdate { status: AppStatus },
}

/// Application status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppStatus {
    /// Number of active bots
    pub active_bots: u32,
    /// Number of running tasks
    pub running_tasks: u32,
    /// Total tokens used this session
    pub session_tokens: u64,
    /// Total cost this session
    pub session_cost: f64,
}

// TypeScript type generation helpers
// These would be used to generate TypeScript types
// The actual generation happens in the build script

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_serialization() {
        let cmd = Command::CreateBot {
            name: "Test Bot".to_string(),
            description: "A test bot".to_string(),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("CreateBot"));
    }

    #[test]
    fn test_event_serialization() {
        let bot = Bot::new("Test", "A test bot");
        let event = Event::BotCreated { bot };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("BotCreated"));
    }
}
