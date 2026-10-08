//! File write skill

use async_trait::async_trait;
use ravenbot_core::Permission;

use crate::traits::{Skill, SkillContext, SkillError, SkillResult};

pub struct FileWriteSkill;

impl FileWriteSkill {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Skill for FileWriteSkill {
    fn id(&self) -> &str {
        "file_write"
    }

    fn name(&self) -> &str {
        "File Write"
    }

    fn description(&self) -> &str {
        "Write content to a file"
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn required_permissions(&self) -> Vec<Permission> {
        vec![Permission::FileSystem {
            paths: vec!["/".to_string()],
        }]
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to write"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write to the file"
                },
                "mode": {
                    "type": "string",
                    "description": "Write mode (default: overwrite)",
                    "enum": ["overwrite", "append", "create_only"]
                }
            },
            "required": ["path", "content"]
        })
    }

    async fn execute(
        &self,
        context: &SkillContext,
        arguments: serde_json::Value,
    ) -> Result<SkillResult, SkillError> {
        let path = arguments
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'path' field".to_string()))?;

        let content = arguments
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'content' field".to_string()))?;

        let mode = arguments
            .get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("overwrite");

        // Resolve against the office's project folders (confined).
        let resolved = context.resolve_path(path)?;
        let path = resolved.as_path();

        // Check if file exists for create_only mode
        if mode == "create_only" && path.exists() {
            return Ok(SkillResult::failure(format!(
                "File already exists: {}",
                path.display()
            )));
        }

        // Read the previous contents *before* writing, because this is the only
        // moment the old version still exists. Without it, "what changed" can
        // only be guessed — and the obvious guess (count the new file's lines)
        // reports a one-line fix as one line invented rather than one replaced.
        //
        // Failure here is not an error: the file may not exist yet, or may be
        // binary. Both are handled by skipping the counts rather than failing a
        // write that would otherwise have succeeded.
        //
        // `existed_before` is captured for the same reason: "unreadable" and
        // "not there" read identically once the write has happened, and they
        // mean different things. A file that was absent is every line new; a
        // file that was present but not text is a rewrite we cannot line up.
        let existed_before = path.exists();
        let before = tokio::fs::read_to_string(path).await.ok();

        // Create parent directories if they don't exist
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| SkillError::Io(e.to_string()))?;
            }
        }

        // Write content
        match mode {
            "append" => {
                use tokio::io::AsyncWriteExt;
                let mut file = tokio::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .await
                    .map_err(|e| SkillError::Io(e.to_string()))?;
                file.write_all(content.as_bytes())
                    .await
                    .map_err(|e| SkillError::Io(e.to_string()))?;
            }
            _ => {
                tokio::fs::write(path, content)
                    .await
                    .map_err(|e| SkillError::Io(e.to_string()))?;
            }
        }

        // Diff the version that existed against the version now on disk. For an
        // append that is old-vs-(old+new), which is what the line counts say; for
        // an overwrite it is old-vs-new. A file that could not be read as text
        // gets no counts rather than wrong ones — reporting a 2MB binary as
        // "+40000 lines" would poison every total it fed.
        //
        // Three cases, because collapsing them is how a ledger starts lying:
        //
        //  - **Read the old text.** Diff it. This is the common path.
        //  - **The file did not exist.** Every line in it is an addition.
        //    Reporting `(0, 0)` here — which is what "no previous version to
        //    compare against" naturally gives — would mean *creating* files
        //    never appears in the journal at all, and creating files is most of
        //    what a new project looks like.
        //  - **Existed, but not as text.** A rewrite of something we cannot
        //    line up. Counts and diff both stay out rather than guessing.
        let display = path.display().to_string();
        let (lines_added, lines_deleted, diff) = if let Some(old) = &before {
            let after = if mode == "append" { format!("{old}{content}") } else { content.to_string() };
            let (a, d) = crate::diff::line_changes(old, &after);
            (a, d, crate::diff::unified(old, &after, &display))
        } else if !existed_before {
            let n = content.lines().count();
            (n, 0, crate::diff::unified("", content, &display))
        } else {
            (0, 0, None)
        };

        Ok(SkillResult::success(serde_json::json!({
            "path": display,
            "bytes_written": content.len(),
            "mode": mode,
            "lines_added": lines_added,
            "lines_deleted": lines_deleted,
            "is_new_file": !existed_before,
            // Same shape `code_edit` reports, so the journal reads one format
            // and adding a writer later does not mean adding a second parser.
            //
            // `diff` is the lines themselves, not only their number. `+31 −12`
            // on a file cannot be checked; expanding it into the hunk that
            // produced those two numbers can.
            "file_changes": [{
                "path": path.display().to_string(),
                "lines_added": lines_added,
                "lines_deleted": lines_deleted,
                "diff": diff,
            }],
        })))
    }
}

impl Default for FileWriteSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    /// A workspace of its own, so a test never writes near the repository.
    ///
    /// Confinement is on (a non-empty folder list turns it on), which is also
    /// how every real run is configured — a test that exercised the unconfined
    /// path would be testing the one branch nobody uses in the app.
    async fn workspace() -> (SkillContext, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("raven_fw_{}", Uuid::new_v4()));
        tokio::fs::create_dir_all(&dir).await.expect("mk temp workspace");
        let ctx = SkillContext::with_default_tier(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4())
            .with_working_dirs(vec![dir.clone()]);
        (ctx, dir)
    }

    async fn write(ctx: &SkillContext, path: &str, content: &str) -> SkillResult {
        FileWriteSkill::new()
            .execute(ctx, serde_json::json!({ "path": path, "content": content }))
            .await
            .expect("execute file_write")
    }

    #[tokio::test]
    async fn a_new_file_counts_every_line_as_an_addition() {
        let (ctx, dir) = workspace().await;
        let result = write(&ctx, "fresh.txt", "one\ntwo\nthree\n").await;
        assert!(result.success);

        // This used to report (0,0) — "there is no previous version to diff
        // against" — and the journal drops (0,0) rows, so creating a file
        // never appeared in the ledger at all. Creating files is most of what
        // a new project looks like, so that was the majority of writes
        // silently missing.
        assert_eq!(result.output["lines_added"].as_i64(), Some(3));
        assert_eq!(result.output["lines_deleted"].as_i64(), Some(0));
        assert_eq!(result.output["is_new_file"].as_bool(), Some(true));

        let diff = result.output["file_changes"][0]["diff"]
            .as_str()
            .expect("a new file has a diff — every line in it is new");
        assert!(diff.contains("+one"), "{diff}");
        assert!(diff.contains("+three"), "{diff}");
        assert!(diff.contains("--- a/"), "{diff}");

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn an_overwrite_reports_the_lines_it_replaced() {
        let (ctx, dir) = workspace().await;
        write(&ctx, "a.txt", "alpha\nbeta\n").await;

        let result = write(&ctx, "a.txt", "alpha\nBETA\n").await;
        assert!(result.success);
        assert_eq!(result.output["lines_added"].as_i64(), Some(1));
        assert_eq!(result.output["lines_deleted"].as_i64(), Some(1));
        assert_eq!(result.output["is_new_file"].as_bool(), Some(false));

        let diff = result.output["file_changes"][0]["diff"]
            .as_str()
            .expect("a text-to-text rewrite is diffable");
        assert!(diff.contains("-beta"), "{diff}");
        assert!(diff.contains("+BETA"), "{diff}");

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn a_no_op_rewrite_records_nothing_to_expand() {
        let (ctx, dir) = workspace().await;
        let body = "same\nlines\n";
        write(&ctx, "same.txt", body).await;
        let result = write(&ctx, "same.txt", body).await;

        // Counts of zero mean the journal will skip the row, and a diff would
        // be a row that opens onto nothing. Both sides agree here, which is
        // the property that matters: the total and the expansion cannot
        // disagree, because neither exists.
        assert_eq!(result.output["lines_added"].as_i64(), Some(0));
        assert_eq!(result.output["lines_deleted"].as_i64(), Some(0));
        assert!(result.output["file_changes"][0]["diff"].is_null());

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn an_append_diffs_the_result_of_the_append() {
        let (ctx, dir) = workspace().await;
        write(&ctx, "log.txt", "first\n").await;

        let result = FileWriteSkill::new()
            .execute(&ctx, serde_json::json!({ "path": "log.txt", "content": "second\n", "mode": "append" }))
            .await
            .expect("execute file_write");
        assert!(result.success);
        assert_eq!(result.output["lines_added"].as_i64(), Some(1));
        assert_eq!(result.output["lines_deleted"].as_i64(), Some(0));

        let diff = result.output["file_changes"][0]["diff"].as_str().expect("diff");
        assert!(diff.contains("+second"), "{diff}");

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}
