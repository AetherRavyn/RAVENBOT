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
        let (lines_added, lines_deleted) = match &before {
            Some(old) => {
                let after = if mode == "append" { format!("{old}{content}") } else { content.to_string() };
                crate::diff::line_changes(old, &after)
            }
            // No readable previous version: either new, or binary. An
            // *existing* binary file still counts as a change, just an
            // uncountable one.
            None => (0, 0),
        };

        Ok(SkillResult::success(serde_json::json!({
            "path": path.display().to_string(),
            "bytes_written": content.len(),
            "mode": mode,
            "lines_added": lines_added,
            "lines_deleted": lines_deleted,
            "is_new_file": before.is_none(),
            // Same shape `code_edit` reports, so the journal reads one format
            // and adding a writer later does not mean adding a second parser.
            "file_changes": [{
                "path": path.display().to_string(),
                "lines_added": lines_added,
                "lines_deleted": lines_deleted,
            }],
        })))
    }
}

impl Default for FileWriteSkill {
    fn default() -> Self {
        Self::new()
    }
}
