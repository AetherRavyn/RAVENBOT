use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use uuid::Uuid;

/// Message role in the conversation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageRole {
    /// User message
    User,
    /// Assistant (bot) message
    Assistant,
    /// System message
    System,
    /// Tool call or result
    Tool,
}

/// Status of a checklist item in a structured message
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChecklistStatus {
    /// Not yet started
    Pending,
    /// Currently in progress
    InProgress,
    /// Completed successfully
    Completed,
    /// Failed
    Failed,
    /// Skipped
    Skipped,
}

/// An attachment to a message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    /// Unique identifier
    pub id: Uuid,
    /// File name
    pub name: String,
    /// MIME type
    pub mime_type: String,
    /// File size in bytes
    pub size: u64,
    /// Path to the file (stored locally)
    pub path: String,
    /// Inline base64 payload (for pasted/dropped images sent without disk IO)
    #[serde(default)]
    pub data: Option<String>,
    /// Whether this attachment is an image the model can see
    #[serde(default)]
    pub is_image: bool,
}

/// A checklist item in a structured message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChecklistItem {
    /// Label for the task
    pub label: String,
    /// Current status
    pub status: ChecklistStatus,
    /// Result or output (if completed)
    pub result: Option<String>,
    /// Link to a sub-thread (if delegated)
    pub thread_id: Option<Uuid>,
    /// Link to a sub-bot (if delegated)
    pub bot_id: Option<Uuid>,
}

/// A message in a thread
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Unique identifier
    pub id: Uuid,
    /// Thread this message belongs to
    pub thread_id: Uuid,
    /// Message role
    pub role: MessageRole,
    /// Message content (text, or structured)
    pub content: MessageContent,
    /// Attachments
    pub attachments: Vec<Attachment>,
    /// Timestamp
    pub created_at: DateTime<Utc>,
}

/// A web source (citation) backing an assistant answer
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Source {
    /// Source URL
    pub url: String,
    /// Source title
    pub title: String,
    /// Optional snippet of the supporting content
    #[serde(default)]
    pub snippet: Option<String>,
}

/// Content of a message - can be plain text or structured
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MessageContent {
    /// Plain text message
    Text {
        text: String,
        /// Web sources/citations backing this message (empty for most messages)
        #[serde(default)]
        sources: Vec<Source>,
    },
    /// Structured checklist message
    Checklist {
        text: Option<String>,
        items: Vec<ChecklistItem>,
    },
    /// Tool call
    ToolCall {
        tool_name: String,
        arguments: serde_json::Value,
    },
    /// Tool result
    ToolResult {
        tool_name: String,
        result: serde_json::Value,
        is_error: bool,
    },
}

impl Message {
    /// Create a new user message
    pub fn user(thread_id: Uuid, text: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            thread_id,
            role: MessageRole::User,
            content: MessageContent::Text { text: text.into(), sources: Vec::new() },
            attachments: Vec::new(),
            created_at: Utc::now(),
        }
    }

    /// Create a new assistant message
    pub fn assistant(thread_id: Uuid, text: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            thread_id,
            role: MessageRole::Assistant,
            content: MessageContent::Text { text: text.into(), sources: Vec::new() },
            attachments: Vec::new(),
            created_at: Utc::now(),
        }
    }

    /// Create a new assistant message with web sources/citations
    pub fn assistant_with_sources(thread_id: Uuid, text: impl Into<String>, sources: Vec<Source>) -> Self {
        Self {
            id: Uuid::new_v4(),
            thread_id,
            role: MessageRole::Assistant,
            content: MessageContent::Text { text: text.into(), sources },
            attachments: Vec::new(),
            created_at: Utc::now(),
        }
    }

    /// Create a new checklist message
    pub fn checklist(thread_id: Uuid, items: Vec<ChecklistItem>) -> Self {
        Self {
            id: Uuid::new_v4(),
            thread_id,
            role: MessageRole::Assistant,
            content: MessageContent::Checklist {
                text: None,
                items,
            },
            attachments: Vec::new(),
            created_at: Utc::now(),
        }
    }
}
