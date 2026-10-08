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
    /// The unified diff that produced the two counts above, when one could be
    /// produced honestly.
    ///
    /// `None` is not an error: a file too large to line up, a rewrite of binary
    /// content, or a row written before this field existed. The counts remain
    /// correct in every one of those cases, and a diff that omitted most of a
    /// file would disagree with the numbers stored beside it — so absence is
    /// reported rather than approximated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
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
            diff: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    /// Attach the lines behind the counts.
    ///
    /// A builder rather than an eighth argument: `new` already takes seven,
    /// and the one caller that has a diff is the only one that would care
    /// about an eighth position — which is exactly how a `Some(...)` ends up
    /// bound to `skill`.
    ///
    /// A diff larger than [`MAX_DIFF`] is dropped rather than truncated. A
    /// truncated unified diff ends mid-hunk, and a hunk that stops before its
    /// change is a claim about the file that was never checked — the counts it
    /// was stored to explain would then outnumber the lines shown. The counts
    /// are still written either way, so nothing is lost but the expansion.
    pub fn with_diff(mut self, diff: Option<String>) -> Self {
        self.diff = diff.filter(|d| {
            // Whitespace-only is not "no diff", it is a diff with no lines in
            // it — and a row that expands onto blank space reads as a broken
            // renderer rather than as an honest absence.
            !d.trim().is_empty() && d.len() <= MAX_DIFF
        });
        self
    }
}

/// Largest diff stored per row, in bytes.
///
/// Generous on purpose. The interesting diffs — a function rewritten, a module
/// split — sit in the tens of kilobytes, and a limit tight enough to catch
/// those would also catch the ordinary ones. What it exists to stop is a
/// whole generated file landing in the ledger: bounded above by the line cap
/// in the skill for `file_write`, and by this for `code_edit`, whose patches
/// are not bounded at all.
const MAX_DIFF: usize = 256 * 1024;

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

#[cfg(test)]
mod tests {
    use super::*;

    fn change() -> FileChange {
        FileChange::new(Uuid::new_v4(), None, None, "src/lib.rs", "file_write", 3, 1)
    }

    #[test]
    fn a_diff_rides_along_with_the_counts() {
        let c = change().with_diff(Some("--- a/src/lib.rs\n+++ b/src/lib.rs\n".into()));
        assert!(c.diff.is_some());
        // And it serialises, because the whole point is that it reaches the UI.
        let json = serde_json::to_string(&c).expect("serialize");
        assert!(json.contains("\"diff\""), "{json}");
    }

    #[test]
    fn an_absent_diff_stays_absent_rather_than_empty() {
        // `Some("")` would render as an expandable row that opens onto
        // nothing. `None` is what the UI reads as "not expandable".
        assert!(change().with_diff(None).diff.is_none());
        assert!(change().with_diff(Some(String::new())).diff.is_none());
    }

    #[test]
    fn a_diff_past_the_cap_is_dropped_not_truncated() {
        // A truncated unified diff ends mid-hunk, and a hunk that stops before
        // its change is a claim that was never checked — while the counts it
        // was stored to explain still say the full number.
        let big = "x\n".repeat(MAX_DIFF / 2 + 1);
        assert!(change().with_diff(Some(big)).diff.is_none());
        assert!(change().with_diff(Some("x".repeat(1024))).diff.is_some());
    }

    #[test]
    fn counts_are_never_negative() {
        let c = FileChange::new(Uuid::new_v4(), None, None, "a", "file_write", -5, -1);
        assert_eq!((c.lines_added, c.lines_deleted), (0, 0));
    }
}
