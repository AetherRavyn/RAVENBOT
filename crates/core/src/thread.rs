use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use uuid::Uuid;

/// What produced a thread.
///
/// Office nodes, delegations and routines are real runs with real messages, but
/// they are not conversations the owner started from the chat switcher. The kind
/// keeps that distinction explicit instead of inferring it from a title prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadKind {
    #[default]
    Chat,
    Office,
    Delegation,
    Routine,
    Node,
    Channel,
    Goal,
}

impl ThreadKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Office => "office",
            Self::Delegation => "delegation",
            Self::Routine => "routine",
            Self::Node => "node",
            Self::Channel => "channel",
            Self::Goal => "goal",
        }
    }

    pub fn from_db(value: &str) -> Self {
        match value {
            "office" => Self::Office,
            "delegation" => Self::Delegation,
            "routine" => Self::Routine,
            "node" => Self::Node,
            "channel" => Self::Channel,
            "goal" => Self::Goal,
            _ => Self::Chat,
        }
    }
}

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
    /// Channel (context) this thread is filed under, if any.
    #[serde(default)]
    pub channel_id: Option<Uuid>,
    /// What created this thread.
    #[serde(default)]
    pub kind: ThreadKind,
    /// Whether the thread belongs in the user-facing history list.
    #[serde(default = "default_true")]
    pub visible: bool,
    /// Created timestamp
    pub created_at: DateTime<Utc>,
    /// Last updated timestamp
    pub updated_at: DateTime<Utc>,
}

fn default_true() -> bool {
    true
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
            channel_id: None,
            kind: ThreadKind::Chat,
            visible: true,
            created_at: now,
            updated_at: now,
        }
    }

    /// A thread that exists for traceability but is not part of the chat list.
    pub fn new_internal(kind: ThreadKind, bot_id: Uuid, title: impl Into<String>) -> Self {
        let mut thread = Self::new(bot_id, title);
        thread.kind = kind;
        thread.visible = false;
        thread
    }

    /// Create a new ephemeral (temporary) thread for a bot
    pub fn new_ephemeral(bot_id: Uuid, title: impl Into<String>) -> Self {
        let mut thread = Self::new(bot_id, title);
        thread.ephemeral = true;
        thread
    }

    /// File this thread under a channel (context).
    pub fn in_channel(mut self, channel_id: Option<Uuid>) -> Self {
        self.channel_id = channel_id;
        self
    }

    /// Mark an office thread as internal to the chat list.
    pub fn in_office(mut self) -> Self {
        self.kind = ThreadKind::Office;
        self.visible = false;
        self
    }
}
