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
    #[serde(default = "chrono::Utc::now")]
    pub created_at: DateTime<Utc>,
    #[serde(default = "chrono::Utc::now")]
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

/// Who wrote a team-channel message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChannelAuthorKind {
    Human,
    Agent,
    System,
}

impl ChannelAuthorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::Agent => "agent",
            Self::System => "system",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "agent" => Self::Agent,
            "system" => Self::System,
            _ => Self::Human,
        }
    }
}

/// One entry in a channel's shared timeline. Human and agent authors use the
/// same shape on purpose: a channel is a shared record, not a chat with a bot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelMessage {
    pub id: Uuid,
    pub channel_id: Uuid,
    pub author_kind: ChannelAuthorKind,
    pub author_id: Option<Uuid>,
    pub author_name: String,
    pub content: String,
    pub reply_to_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

impl ChannelMessage {
    pub fn human(
        channel_id: Uuid,
        author_name: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            channel_id,
            author_kind: ChannelAuthorKind::Human,
            author_id: None,
            author_name: author_name.into(),
            content: content.into(),
            reply_to_id: None,
            created_at: Utc::now(),
        }
    }

    pub fn agent(
        channel_id: Uuid,
        bot_id: Uuid,
        author_name: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            channel_id,
            author_kind: ChannelAuthorKind::Agent,
            author_id: Some(bot_id),
            author_name: author_name.into(),
            content: content.into(),
            reply_to_id: None,
            created_at: Utc::now(),
        }
    }

    pub fn system(channel_id: Uuid, content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            channel_id,
            author_kind: ChannelAuthorKind::System,
            author_id: None,
            author_name: "RAVENBOT".to_string(),
            content: content.into(),
            reply_to_id: None,
            created_at: Utc::now(),
        }
    }
}

/// Where a channel task is in its lifecycle. Archived is separate from done:
/// it is the "kept for the record, cannot run" state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChannelTaskState {
    Pending,
    Running,
    Paused,
    Done,
    Failed,
    Archived,
}

impl ChannelTaskState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Archived => "archived",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "running" => Self::Running,
            "paused" => Self::Paused,
            "done" => Self::Done,
            "failed" => Self::Failed,
            "archived" => Self::Archived,
            _ => Self::Pending,
        }
    }
}

/// A unit of work in a channel. Exactly one owner may run it at a time; the
/// ownership transfer is a row update, never a second task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelTask {
    pub id: Uuid,
    pub channel_id: Uuid,
    pub title: String,
    pub description: String,
    pub owner_bot_id: Uuid,
    pub state: ChannelTaskState,
    pub created_by: String,
    pub source_thread_id: Option<Uuid>,
    pub result: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub archived_at: Option<DateTime<Utc>>,
}

impl ChannelTask {
    pub fn new(
        channel_id: Uuid,
        title: impl Into<String>,
        description: impl Into<String>,
        owner_bot_id: Uuid,
        created_by: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            channel_id,
            title: title.into(),
            description: description.into(),
            owner_bot_id,
            state: ChannelTaskState::Pending,
            created_by: created_by.into(),
            source_thread_id: None,
            result: None,
            created_at: now,
            updated_at: now,
            archived_at: None,
        }
    }
}

/// An immutable audit entry for a task transition. The current task row is the
/// state; this is the history that explains how it got there.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelTaskEvent {
    pub id: Uuid,
    pub task_id: Uuid,
    pub kind: String,
    pub actor: String,
    pub from_owner: Option<Uuid>,
    pub to_owner: Option<Uuid>,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
}
