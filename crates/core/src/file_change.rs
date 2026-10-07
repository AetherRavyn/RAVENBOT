//! A single file write made by an agent, as a ledger entry.
//!
//! "Which agent changed what" was unanswerable. Tool calls were transient
//! events in the browser, so an office of five could rewrite a project and
//! leave no record of who did which part — reload the page and every write in
//! the conversation was gone, leaving answers that claimed work with no way to
//! check it.
//!
//! One row per write rather than a running total per agent, because a total
//! cannot be asked anything: "what did Sam touch before the build broke",
//! "revert everything Priya did this run", "was this file written twice".
//! Totals are a query over these, not a field on them.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileChange {
    pub id: Uuid,
    pub bot_id: Uuid,
    /// The run that made the change. `None` for a write outside a run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<Uuid>,
    /// The path as the tool resolved it — what the agent saw when it wrote, and
    /// what a reader will search for. Not normalised to absolute, because an
    /// office's folders are named the way its owner named them.
    pub path: String,
    /// Which skill did it: `file_write`, `code_edit`, …
    pub skill: String,
    pub lines_added: i64,
    pub lines_deleted: i64,
    /// RFC 3339, matching every other timestamp in the schema.
    pub created_at: String,
}

impl FileChange {
    pub fn new(
        bot_id: Uuid,
        run_id: Option<Uuid>,
        thread_id: Option<Uuid>,
        path: impl Into<String>,
        skill: impl Into<String>,
        lines_added: i64,
        lines_deleted: i64,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            bot_id,
            run_id,
            thread_id,
            path: path.into(),
            skill: skill.into(),
            lines_added: lines_added.max(0),
            lines_deleted: lines_deleted.max(0),
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

/// One agent's totals over a set of changes.
///
/// Returned alongside the list rather than computed by the client, because the
/// client would then need every row to add up two numbers — and a summary that
/// is wrong only when the list is long is the kind of wrong nobody notices.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FileChangeTotals {
    pub files: i64,
    pub writes: i64,
    pub lines_added: i64,
    pub lines_deleted: i64,
}

impl FileChangeTotals {
    pub fn from_changes(changes: &[FileChange]) -> Self {
        let mut t = Self {
            files: 0,
            writes: changes.len() as i64,
            ..Default::default()
        };
        // Distinct paths, not rows: "12 files" must mean twelve files even if
        // one of them was written six times in a loop.
        let mut seen = std::collections::HashSet::new();
        for c in changes {
            if seen.insert(c.path.as_str()) {
                t.files += 1;
            }
            t.lines_added += c.lines_added;
            t.lines_deleted += c.lines_deleted;
        }
        t
    }
}
