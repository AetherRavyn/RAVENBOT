//! File read skill

use async_trait::async_trait;
use ravenbot_core::Permission;
use std::path::Path;

use crate::traits::{Skill, SkillContext, SkillError, SkillResult};

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
        "Read the contents of a file"
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
                    "description": "Path to the file to read"
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

    async fn execute(
        &self,
        _context: &SkillContext,
        arguments: serde_json::Value,
    ) -> Result<SkillResult, SkillError> {
        let path = arguments
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'path' field".to_string()))?;

        // Security: basic path validation
        let path = Path::new(path);
        
        // Check if file exists
        if !path.exists() {
            return Ok(SkillResult::failure(format!("File not found: {}", path.display())));
        }

        // Check if it's a file
        if !path.is_file() {
            return Ok(SkillResult::failure(format!("Not a file: {}", path.display())));
        }

        // Read file content
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| SkillError::Io(e.to_string()))?;

        // Truncate if too large (1MB limit)
        let truncated = content.len() > 1_000_000;
        let display_content = if truncated {
            &content[..1_000_000]
        } else {
            &content
        };

        Ok(SkillResult::success(serde_json::json!({
            "path": path.display().to_string(),
            "content": display_content,
            "size": content.len(),
            "truncated": truncated
        })))
    }
}

impl Default for FileReadSkill {
    fn default() -> Self {
        Self::new()
    }
}
