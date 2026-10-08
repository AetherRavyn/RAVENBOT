use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Lifecycle of one `ask_user` question.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QuestionStatus {
    /// Waiting for the user (run is parked)
    Pending,
    /// User answered — the answer was fed back to the model
    Answered,
    /// The run finished or timed out before an answer
    Expired,
}

/// A question the agent asked the user mid-run (human-in-the-loop).
/// Persisted and streamed so the chat can render an answer card inline and
/// the run resumes with the chosen answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionRequest {
    /// Unique identifier
    pub id: Uuid,
    /// Bot that asked
    pub bot_id: Uuid,
    /// Thread the question belongs to (card renders here)
    pub thread_id: Uuid,
    /// Run parked waiting on the answer
    pub run_id: Uuid,
    /// Short card header ("Which database?")
    pub header: String,
    /// Full question text
    pub question: String,
    /// Optional predefined choices rendered as buttons
    #[serde(default)]
    pub options: Vec<String>,
    /// Whether a free-text answer is also allowed
    #[serde(default)]
    pub allow_custom: bool,
    /// Current lifecycle state
    pub status: QuestionStatus,
    /// The user's answer (set when answered)
    pub answer: Option<String>,
    /// Created timestamp
    pub created_at: DateTime<Utc>,
    /// Answered timestamp
    pub answered_at: Option<DateTime<Utc>>,
}

impl QuestionRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn pending(
        bot_id: Uuid,
        thread_id: Uuid,
        run_id: Uuid,
        header: impl Into<String>,
        question: impl Into<String>,
        options: Vec<String>,
        allow_custom: bool,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            bot_id,
            thread_id,
            run_id,
            header: header.into(),
            question: question.into(),
            options,
            allow_custom,
            status: QuestionStatus::Pending,
            answer: None,
            created_at: Utc::now(),
            answered_at: None,
        }
    }
}
