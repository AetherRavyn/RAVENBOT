//! File read skill

use async_trait::async_trait;
use ravenbot_core::Permission;

use crate::traits::{Skill, SkillContext, SkillError, SkillResult, SkillRisk};

pub struct FileReadSkill;

impl FileReadSkill {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Skill for FileReadSkill {
    fn id(&self) -> &str {
        "file_read"
    }

    fn name(&self) -> &str {
        "File Read"
    }

    fn description(&self) -> &str {
        "Read a text file from the workspace. Prefer this over cat, head, or tail."
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
                    "description": "Path to the text file to read"
                },
                "offset": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "First line to return (1-based). Use with limit for large files."
                },
                "limit": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 5000,
                    "description": "Maximum lines to return (default 2000)"
                },
                "encoding": {
                    "type": "string",
                    "description": "File encoding (default: utf-8)",
                    "enum": ["utf-8", "ascii", "latin1"]
                }
            },
            "required": ["path"]
        })
    }

    fn risk(&self) -> SkillRisk {
        SkillRisk::ReadOnly
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

        // Resolve against the office's project folders (confined).
        let resolved = context.resolve_path(path)?;
        let path = resolved.as_path();

        // Check if file exists
        if !path.exists() {
            return Ok(SkillResult::failure(format!(
                "File not found: {}",
                path.display()
            )));
        }

        // Check if it's a file
        if !path.is_file() {
            return Ok(SkillResult::failure(format!(
                "Not a file: {}",
                path.display()
            )));
        }

        // A model cannot use a 100MB file in context, and reading one just to
        // truncate it is worse than refusing it with a pointer to a targeted
        // search. Eight megabytes is enough for a large source file or log
        // window; code_search is the right tool beyond that.
        let meta = tokio::fs::metadata(path)
            .await
            .map_err(|e| SkillError::Io(e.to_string()))?;
        if meta.len() > 8_000_000 {
            return Ok(SkillResult::failure(format!(
                "File is {} bytes, larger than the 8MB read limit. Use code_search to locate the relevant lines, then read a narrower file or range.",
                meta.len()
            )));
        }

        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| SkillError::Io(e.to_string()))?;

        let offset = arguments
            .get("offset")
            .and_then(|v| v.as_u64())
            .unwrap_or(1)
            .max(1) as usize;
        let limit = arguments
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(2000)
            .clamp(1, 5000) as usize;
        let total_lines = content.lines().count();
        let selected: Vec<&str> = content
            .lines()
            .skip(offset.saturating_sub(1))
            .take(limit)
            .collect();
        let mut display_content = selected.join("\n");
        let mut truncated = false;
        if display_content.len() > 1_000_000 {
            display_content = crate::clip_utf8(&display_content, 1_000_000).to_string();
            truncated = true;
        }
        let returned = selected.len();
        let next_offset = offset.saturating_add(returned);
        let more = next_offset <= total_lines;

        Ok(SkillResult::success(serde_json::json!({
            "path": path.display().to_string(),
            "content": display_content,
            "size": content.len(),
            "offset": offset,
            "limit": limit,
            "returned_lines": returned,
            "total_lines": total_lines,
            "next_offset": if more { Some(next_offset) } else { None },
            "truncated": truncated
        })))
    }
}

impl Default for FileReadSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ravenbot_core::SandboxTier;
    use uuid::Uuid;

    #[tokio::test]
    async fn reads_a_requested_line_window() {
        let root = std::env::temp_dir().join(format!("rb-read-window-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("notes.txt");
        std::fs::write(&path, "one\ntwo\nthree\nfour\nfive\n").unwrap();
        let context = SkillContext::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            SandboxTier::Host,
        )
        .with_working_dirs(vec![root.clone()]);

        let result = FileReadSkill::new()
            .execute(
                &context,
                serde_json::json!({ "path": "notes.txt", "offset": 3, "limit": 2 }),
            )
            .await
            .expect("execute");
        assert!(result.success);
        assert_eq!(result.output["content"].as_str(), Some("three\nfour"));
        assert_eq!(result.output["total_lines"].as_u64(), Some(5));
        assert_eq!(result.output["next_offset"].as_u64(), Some(5));

        let _ = std::fs::remove_dir_all(&root);
    }
}
