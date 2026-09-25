//! Note manager — create, search, organize, and manage notes and documentation

use async_trait::async_trait;
use ravenbot_core::Permission;
use crate::traits::{Skill, SkillContext, SkillError, SkillResult, SkillRisk};
use std::path::Path;

pub struct NoteManagerSkill;

impl NoteManagerSkill { pub fn new() -> Self { Self } }

#[async_trait]
impl Skill for NoteManagerSkill {
    fn id(&self) -> &str { "note_manager" }
    fn name(&self) -> &str { "Note Manager" }
    fn description(&self) -> &str { "Create, search, organize, and manage notes, bookmarks, and documentation files" }
    fn version(&self) -> &str { "1.0.0" }
    fn required_permissions(&self) -> Vec<Permission> {
        vec![Permission::FileSystem { paths: vec![".".to_string()] }]
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["create", "read", "update", "delete", "search", "list", "tag", "export", "bookmark"],
                    "description": "Note action"
                },
                "path": {"type": "string", "description": "Note file path or directory"},
                "title": {"type": "string", "description": "Note title"},
                "content": {"type": "string", "description": "Note content (Markdown supported)"},
                "tags": {"type": "array", "items": {"type": "string"}, "description": "Tags for organization"},
                "query": {"type": "string", "description": "Search query"},
                "directory": {"type": "string", "description": "Notes directory (default: ./notes)"},
                "url": {"type": "string", "description": "URL for bookmark action"},
                "description": {"type": "string", "description": "Bookmark description"}
            },
            "required": ["action"]
        })
    }

    fn risk(&self) -> SkillRisk { SkillRisk::Low }

    async fn execute(&self, _ctx: &SkillContext, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let action = args.get("action").and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'action'".into()))?;
        let dir = args.get("directory").and_then(|v| v.as_str()).unwrap_or("./notes").to_string();

        match action {
            "create" => self.create_note(&dir, args).await,
            "read" => self.read_note(args).await,
            "update" => self.update_note(args).await,
            "delete" => self.delete_note(args).await,
            "search" => self.search_notes(&dir, args).await,
            "list" => self.list_notes(&dir, args).await,
            "tag" => self.tag_note(args).await,
            "export" => self.export_notes(&dir, args).await,
            "bookmark" => self.create_bookmark(&dir, args).await,
            _ => Err(SkillError::InvalidArguments(format!("Unknown action: {}", action))),
        }
    }
}

impl NoteManagerSkill {
    async fn create_note(&self, dir: &str, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let title = args.get("title").and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'title'".into()))?;
        let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");

        tokio::fs::create_dir_all(dir).await.ok();

        let filename = title.to_lowercase().replace(' ', "-").replace(|c: char| !c.is_alphanumeric() && c != '-', "");
        let path = format!("{}/{}.md", dir, filename);

        let tags_str = args.get("tags").and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(", "))
            .unwrap_or_default();

        let full_content = if tags_str.is_empty() {
            format!("# {}\n\n{}\n", title, content)
        } else {
            format!("---\ntags: [{}]\n---\n# {}\n\n{}\n", tags_str, title, content)
        };

        tokio::fs::write(&path, &full_content).await
            .map_err(|e| SkillError::Io(e.to_string()))?;

        Ok(SkillResult::success(serde_json::json!({
            "path": path,
            "title": title,
            "created": true
        })))
    }

    async fn read_note(&self, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let path = args.get("path").and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'path'".into()))?;

        let content = tokio::fs::read_to_string(path).await
            .map_err(|e| SkillError::Io(format!("Cannot read note: {}", e)))?;

        Ok(SkillResult::success(serde_json::json!({
            "path": path,
            "content": content
        })))
    }

    async fn update_note(&self, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let path = args.get("path").and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'path'".into()))?;
        let content = args.get("content").and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'content'".into()))?;

        if !Path::new(path).exists() {
            return Ok(SkillResult::failure(format!("Note not found: {}", path)));
        }

        tokio::fs::write(path, content).await
            .map_err(|e| SkillError::Io(e.to_string()))?;

        Ok(SkillResult::success(serde_json::json!({
            "path": path,
            "updated": true
        })))
    }

    async fn delete_note(&self, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let path = args.get("path").and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'path'".into()))?;

        tokio::fs::remove_file(path).await
            .map_err(|e| SkillError::Io(format!("Cannot delete: {}", e)))?;

        Ok(SkillResult::success(serde_json::json!({
            "path": path,
            "deleted": true
        })))
    }

    async fn search_notes(&self, dir: &str, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();

        let mut entries = tokio::fs::read_dir(dir).await
            .map_err(|_| SkillError::Io("Notes directory not found".into()))?;

        let mut results = Vec::new();
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("md") {
                if let Ok(content) = tokio::fs::read_to_string(&path).await {
                    if query.is_empty() || content.to_lowercase().contains(&query) {
                        results.push(serde_json::json!({
                            "path": path.display().to_string(),
                            "preview": content.lines().take(3).collect::<Vec<_>>().join(" ")
                        }));
                    }
                }
            }
        }

        Ok(SkillResult::success(serde_json::json!({
            "query": query,
            "results": results,
            "count": results.len()
        })))
    }

    async fn list_notes(&self, dir: &str, _args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let mut entries = tokio::fs::read_dir(dir).await
            .map_err(|_| SkillError::Io("Notes directory not found".into()))?;

        let mut notes = Vec::new();
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("md") {
                if let Ok(metadata) = entry.metadata().await {
                    notes.push(serde_json::json!({
                        "path": path.display().to_string(),
                        "name": path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown"),
                        "size": metadata.len(),
                        "modified": metadata.modified().ok().map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs())
                    }));
                }
            }
        }

        Ok(SkillResult::success(serde_json::json!({
            "directory": dir,
            "notes": notes,
            "count": notes.len()
        })))
    }

    async fn tag_note(&self, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let path = args.get("path").and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'path'".into()))?;
        let tags: Vec<String> = args.get("tags").and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'tags'".into()))?;

        let content = tokio::fs::read_to_string(path).await
            .map_err(|e| SkillError::Io(e.to_string()))?;

        let new_tags = tags.join(", ");
        let tags_line = format!("tags: [{}]", new_tags);
        let updated = if content.starts_with("---") {
            // Has frontmatter, update it
            let mut lines: Vec<String> = content.lines().map(String::from).collect();
            let mut found_tags = false;
            for line in &mut lines {
                if line.starts_with("tags:") {
                    *line = tags_line.clone();
                    found_tags = true;
                    break;
                }
            }
            if !found_tags {
                // Add tags line after first ---
                if let Some(pos) = lines.iter().position(|l| l == "---") {
                    lines.insert(pos + 1, tags_line.clone());
                }
            }
            lines.join("\n")
        } else {
            format!("---\ntags: [{}]\n---\n{}", new_tags, content)
        };

        tokio::fs::write(path, &updated).await
            .map_err(|e| SkillError::Io(e.to_string()))?;

        Ok(SkillResult::success(serde_json::json!({
            "path": path,
            "tags": tags,
            "success": true
        })))
    }

    async fn export_notes(&self, dir: &str, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let format = args.get("query").and_then(|v| v.as_str()).unwrap_or("markdown");

        let mut entries = tokio::fs::read_dir(dir).await
            .map_err(|_| SkillError::Io("Notes directory not found".into()))?;

        let mut all_content = String::new();
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("md") {
                if let Ok(content) = tokio::fs::read_to_string(&path).await {
                    all_content.push_str(&format!("{}\n\n---\n\n", content));
                }
            }
        }

        let exported = match format {
            "html" => format!("<html><body><pre>{}</pre></body></html>", all_content),
            "text" => all_content.replace('#', "").replace("---", ""),
            _ => all_content,
        };

        Ok(SkillResult::success(serde_json::json!({
            "format": format,
            "content": exported,
            "note_count": exported.matches("---").count()
        })))
    }

    async fn create_bookmark(&self, dir: &str, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let url = args.get("url").and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'url'".into()))?;
        let desc = args.get("description").and_then(|v| v.as_str()).unwrap_or("");
        let title = args.get("title").and_then(|v| v.as_str()).unwrap_or(url);

        tokio::fs::create_dir_all(dir).await.ok();

        let path = format!("{}/bookmarks.md", dir);
        let entry = format!("\n- [{}]({}) — {}\n", title, url, desc);

        let mut content = if Path::new(&path).exists() {
            tokio::fs::read_to_string(&path).await.unwrap_or_default()
        } else {
            "# Bookmarks\n".to_string()
        };

        content.push_str(&entry);

        tokio::fs::write(&path, &content).await
            .map_err(|e| SkillError::Io(e.to_string()))?;

        Ok(SkillResult::success(serde_json::json!({
            "path": path,
            "url": url,
            "title": title,
            "bookmarked": true
        })))
    }
}

impl Default for NoteManagerSkill {
    fn default() -> Self { Self::new() }
}
