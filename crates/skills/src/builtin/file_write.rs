//! File write skill

use async_trait::async_trait;
use ravenbot_core::Permission;
use std::path::Path;

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
        _context: &SkillContext,
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

        let path = Path::new(path);

        // Check if file exists for create_only mode
        if mode == "create_only" && path.exists() {
            return Ok(SkillResult::failure(format!(
                "File already exists: {}",
                path.display()
            )));
        }

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

        Ok(SkillResult::success(serde_json::json!({
            "path": path.display().to_string(),
            "bytes_written": content.len(),
            "mode": mode
        })))
    }
}

impl Default for FileWriteSkill {
    fn default() -> Self {
        Self::new()
    }
}
