use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use uuid::Uuid;

/// Message role in the conversation
///
/// The wire format is lowercase (`"user"`, `"assistant"`, …), matching the
/// TypeScript `MessageRole` and optimistic browser messages. Capitalized
/// aliases preserve compatibility with previously emitted payloads.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    /// User message
    #[serde(alias = "User")]
    User,
    /// Assistant (bot) message
    #[serde(alias = "Assistant")]
    Assistant,
    /// System message
    #[serde(alias = "System")]
    System,
    /// Tool call or result
    #[serde(alias = "Tool")]
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
    /// Which bot spoke (None = human user / system). In office group threads
    /// this is the ONLY way to tell agents apart — the role is "assistant"
    /// for all of them, so the UI renders avatar+name from this field.
    #[serde(default)]
    pub sender_bot_id: Option<Uuid>,
    /// Human label for the sender ("You", or the bot's name/rank at send
    /// time). Denormalized so history survives renames/deletes.
    #[serde(default)]
    pub sender_name: Option<String>,
    /// Which message this replies to (office agent-to-agent discussion).
    #[serde(default)]
    pub reply_to_id: Option<Uuid>,
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
        /**
         * The model's reasoning for this message, across every round of the run
         * that produced it.
         *
         * A field of its own rather than `<think>` markers inside `text`, because
         * the two were indistinguishable in practice: the streamed buffer was
         * cleared at each tool round so the trace vanished, an interrupted stream
         * left the markers open in the user's paragraph, and a renderer that
         * missed a marker showed private notes as answer text. Stored apart, the
         * reasoning cannot leak into the answer no matter how the front end is
         * written.
         *
         * `serde(default)` because `content` is stored as a JSON blob, so this
         * needs no migration — every existing row decodes with `None`.
         */
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reasoning: Option<String>,
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
            content: MessageContent::Text { text: text.into(), sources: Vec::new(), reasoning: None },
            attachments: Vec::new(),
            sender_bot_id: None,
            sender_name: Some("You".to_string()),
            reply_to_id: None,
            created_at: Utc::now(),
        }
    }

    /// Create a new assistant message
    pub fn assistant(thread_id: Uuid, text: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            thread_id,
            role: MessageRole::Assistant,
            content: MessageContent::Text { text: text.into(), sources: Vec::new(), reasoning: None },
            attachments: Vec::new(),
            sender_bot_id: None,
            sender_name: None,
            reply_to_id: None,
            created_at: Utc::now(),
        }
    }

    /// Create a new assistant message spoken by a specific bot (office rooms).
    pub fn assistant_from_bot(
        thread_id: Uuid,
        bot_id: Uuid,
        bot_name: impl Into<String>,
        text: impl Into<String>,
    ) -> Self {
        let mut msg = Self::assistant(thread_id, text);
        msg.sender_bot_id = Some(bot_id);
        msg.sender_name = Some(bot_name.into());
        msg
    }

    /// Create a new assistant message with web sources/citations
    pub fn assistant_with_sources(thread_id: Uuid, text: impl Into<String>, sources: Vec<Source>) -> Self {
        Self {
            id: Uuid::new_v4(),
            thread_id,
            role: MessageRole::Assistant,
            content: MessageContent::Text { text: text.into(), sources, reasoning: None },
            attachments: Vec::new(),
            sender_bot_id: None,
            sender_name: None,
            reply_to_id: None,
            created_at: Utc::now(),
        }
    }

    /// An assistant reply that carries its reasoning separately from its answer.
    ///
    /// The reasoning is passed through rather than wrapped in markers so it stays
    /// a distinct thing all the way to the renderer. `strip_thinking` below exists
    /// for the messages written before this field did, which still carry markers
    /// inside their text.
    pub fn assistant_with_reasoning(
        thread_id: Uuid,
        text: impl Into<String>,
        reasoning: impl Into<String>,
        sources: Vec<Source>,
    ) -> Self {
        let reasoning = reasoning.into();
        let reasoning = if reasoning.trim().is_empty() { None } else { Some(reasoning) };
        Self {
            id: Uuid::new_v4(),
            thread_id,
            role: MessageRole::Assistant,
            content: MessageContent::Text { text: text.into(), sources, reasoning },
            attachments: Vec::new(),
            sender_bot_id: None,
            sender_name: None,
            reply_to_id: None,
            created_at: Utc::now(),
        }
    }

    /**
     * Pull any `<think>` block out of a message body, returning it separately.
     *
     * For messages stored before `MessageContent::Text::reasoning` existed, whose
     * text still carries the markers. Handles *every* block, not just the first:
     * a run is many model rounds and each contributed one, so a single-regex
     * version silently dropped all but the first and left the rest in the answer
     * as raw markup.
     *
     * Returns `None` when there is nothing to lift, which is the common case now.
     */
    pub fn split_thinking(text: &str) -> (String, Option<String>) {
        if !text.to_ascii_lowercase().contains("<think>") {
            return (text.to_string(), None);
        }
        let mut body = String::with_capacity(text.len());
        let mut thinking: Vec<String> = Vec::new();
        let mut rest = text;
        // Loop rather than a single `replace`, because a model can emit more than
        // one block and a run of rounds produced several.
        while let Some(start) = rest.to_ascii_lowercase().find("<think>") {
            let (before, after_open) = rest.split_at(start + "<think>".len());
            body.push_str(before);
            match after_open.to_ascii_lowercase().find("</think>") {
                Some(end) => {
                    let (block, tail) = after_open.split_at(end);
                    let block = block.trim();
                    if !block.is_empty() {
                        thinking.push(block.to_string());
                    }
                    rest = &tail["</think>".len()..];
                }
                // Unterminated: a stream cut mid-thought. Treated as reasoning so
                // it cannot be shown as answer text.
                None => {
                    let block = after_open.trim();
                    if !block.is_empty() {
                        thinking.push(block.to_string());
                    }
                    rest = "";
                }
            }
        }
        body.push_str(rest);
        let joined = if thinking.is_empty() { None } else { Some(thinking.join("\n\n")) };
        (body.trim().to_string(), joined)
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
            sender_bot_id: None,
            sender_name: None,
            reply_to_id: None,
            created_at: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn message_roles_round_trip_as_frontend_lowercase_values() {
        let user = Message::user(Uuid::new_v4(), "hi");
        let assistant = Message::assistant(Uuid::new_v4(), "hello");

        assert_eq!(
            serde_json::to_value(&user.role).expect("serialize user role"),
            Value::String("user".to_string())
        );
        assert_eq!(
            serde_json::to_value(&assistant.role).expect("serialize assistant role"),
            Value::String("assistant".to_string())
        );
        assert_eq!(
            serde_json::from_value::<Message>(serde_json::to_value(&user).expect("serialize user"))
                .expect("deserialize user")
                .role,
            MessageRole::User
        );
    }

    #[test]
    fn message_role_accepts_legacy_capitalized_values() {
        assert_eq!(
            serde_json::from_value::<MessageRole>(json!("User")).expect("deserialize User"),
            MessageRole::User
        );
        assert_eq!(
            serde_json::from_value::<MessageRole>(json!("Assistant")).expect("deserialize Assistant"),
            MessageRole::Assistant
        );
    }
}
