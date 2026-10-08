//! Environment manager — manage .env files, environment variables, and secrets

use async_trait::async_trait;
use ravenbot_core::Permission;
use crate::traits::{Skill, SkillContext, SkillError, SkillResult};
use std::path::Path;

pub struct EnvManagerSkill;

impl EnvManagerSkill { pub fn new() -> Self { Self } }

#[async_trait]
impl Skill for EnvManagerSkill {
    fn id(&self) -> &str { "env_manager" }
    fn name(&self) -> &str { "Environment Manager" }
    fn description(&self) -> &str { "Manage .env files: read, write, validate, diff, and sync environment variables" }
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
                    "enum": ["read", "write", "delete", "validate", "diff", "sync", "list", "export"],
                    "description": "Action to perform"
                },
                "file": {
                    "type": "string",
                    "description": "Path to .env file (default: .env)"
                },
                "key": {"type": "string", "description": "Environment variable key"},
                "value": {"type": "string", "description": "Environment variable value (for write)"},
                "variables": {
                    "type": "object",
                    "description": "Multiple key-value pairs to write (for write action)"
                },
                "required_keys": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Keys that must exist (for validate)"
                },
                "source": {"type": "string", "description": "Source .env file for diff/sync"},
                "target": {"type": "string", "description": "Target .env file for diff/sync"}
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, ctx: &SkillContext, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let action = args
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        if action.is_empty() {
            return Err(SkillError::InvalidArguments("Missing 'action'".into()));
        }
        // Every file this skill touches is confined to the run's workspace
        // before any action runs. Previously `file` was taken as given, so
        // `write_env` could write `~/.bashrc` or an application config, and
        // `sync_env` could copy values into any file on the computer.
        //
        // Resolving once here, rather than in each action, means there is a
        // single place the boundary is enforced and every action inherits it.
        let mut args = args;
        confine_path(ctx, &mut args, "file", ".env")?;
        confine_path(ctx, &mut args, "source", ".env")?;
        confine_path(ctx, &mut args, "target", ".env")?;
        let file = args.get("file").and_then(|v| v.as_str()).unwrap_or(".env").to_string();

        match action.as_str() {
            "read" => self.read_env(&file).await,
            "write" => self.write_env(&file, args).await,
            "delete" => self.delete_key(&file, args).await,
            "validate" => self.validate_env(&file, args).await,
            "diff" => self.diff_env(args).await,
            "sync" => self.sync_env(args).await,
            "list" => self.list_env(&file).await,
            "export" => self.export_env(&file, args).await,
            other => Err(SkillError::InvalidArguments(format!(
                "Unknown action: {other}"
            ))),
        }
    }
}

/// Rewrite one path-valued argument in place to its confined absolute form.
///
/// Absent fields are left absent so each action keeps its own default, which
/// is itself resolved.
fn confine_path(
    ctx: &SkillContext,
    args: &mut serde_json::Value,
    key: &str,
    default: &str,
) -> Result<(), SkillError> {
    let raw = args.get(key).and_then(|v| v.as_str()).unwrap_or(default);
    let resolved = ctx.resolve_path(raw)?;
    if let Some(obj) = args.as_object_mut() {
        obj.insert(key.to_string(), serde_json::Value::String(resolved.to_string_lossy().to_string()));
    }
    Ok(())
}

impl EnvManagerSkill {
    async fn read_env(&self, file: &str) -> Result<SkillResult, SkillError> {
        if !Path::new(file).exists() {
            return Ok(SkillResult::failure(format!("File not found: {}", file)));
        }

        let content = tokio::fs::read_to_string(file).await
            .map_err(|e| SkillError::Io(e.to_string()))?;

        let vars: std::collections::HashMap<String, String> = content
            .lines()
            .filter(|line| !line.trim().is_empty() && !line.trim().starts_with('#'))
            .filter_map(|line| {
                let mut parts = line.splitn(2, '=');
                Some((parts.next()?.trim().to_string(), parts.next()?.trim().trim_matches('"').trim_matches('\'').to_string()))
            })
            .collect();

        Ok(SkillResult::success(serde_json::json!({
            "file": file,
            "variables": vars,
            "count": vars.len()
        })))
    }

    async fn write_env(&self, file: &str, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let mut content = if Path::new(file).exists() {
            tokio::fs::read_to_string(file).await.map_err(|e| SkillError::Io(e.to_string()))?
        } else {
            String::new()
        };

        // Single key-value
        if let (Some(key), Some(value)) = (
            args.get("key").and_then(|v| v.as_str()),
            args.get("value").and_then(|v| v.as_str()),
        ) {
            content = self.set_env_var(&content, key, value);
        }

        // Multiple variables
        if let Some(vars) = args.get("variables").and_then(|v| v.as_object()) {
            for (key, value) in vars {
                if let Some(val_str) = value.as_str() {
                    content = self.set_env_var(&content, key, val_str);
                }
            }
        }

        tokio::fs::write(file, &content).await
            .map_err(|e| SkillError::Io(e.to_string()))?;

        Ok(SkillResult::success(serde_json::json!({
            "file": file,
            "action": "write",
            "success": true
        })))
    }

    fn set_env_var(&self, content: &str, key: &str, value: &str) -> String {
        let lines: Vec<String> = content.lines().map(String::from).collect();
        let mut found = false;
        let mut result = String::new();

        for line in &lines {
            if line.starts_with(&format!("{}=", key)) || line.starts_with(&format!("{} =", key)) {
                result.push_str(&format!("{}=\"{}\"\n", key, value));
                found = true;
            } else {
                result.push_str(&format!("{}\n", line));
            }
        }

        if !found {
            result.push_str(&format!("{}=\"{}\"\n", key, value));
        }

        result
    }

    async fn delete_key(&self, file: &str, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let key = args.get("key").and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'key'".into()))?;

        let content = tokio::fs::read_to_string(file).await
            .map_err(|e| SkillError::Io(e.to_string()))?;

        let new_content: String = content.lines()
            .filter(|line| !line.starts_with(&format!("{}=", key)))
            .collect::<Vec<_>>()
            .join("\n");

        tokio::fs::write(file, &new_content).await
            .map_err(|e| SkillError::Io(e.to_string()))?;

        Ok(SkillResult::success(serde_json::json!({
            "file": file,
            "action": "delete",
            "key": key,
            "success": true
        })))
    }

    async fn validate_env(&self, file: &str, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let content = tokio::fs::read_to_string(file).await
            .map_err(|e| SkillError::Io(e.to_string()))?;

        let existing: std::collections::HashSet<String> = content.lines()
            .filter(|line| !line.trim().is_empty() && !line.trim().starts_with('#'))
            .filter_map(|line| line.splitn(2, '=').next().map(|s| s.trim().to_string()))
            .collect();

        let required: Vec<String> = args.get("required_keys")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        let missing: Vec<String> = required.iter()
            .filter(|key| !existing.contains(*key))
            .cloned()
            .collect();

        Ok(SkillResult::success(serde_json::json!({
            "file": file,
            "valid": missing.is_empty(),
            "missing": missing,
            "existing_count": existing.len()
        })))
    }

    async fn diff_env(&self, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let source = args.get("source").and_then(|v| v.as_str()).unwrap_or(".env");
        let target = args.get("target").and_then(|v| v.as_str()).unwrap_or(".env.example");

        let source_content = tokio::fs::read_to_string(source).await
            .map_err(|e| SkillError::Io(e.to_string()))?;
        let target_content = tokio::fs::read_to_string(target).await
            .map_err(|e| SkillError::Io(e.to_string()))?;

        let source_vars: std::collections::HashSet<String> = source_content.lines()
            .filter(|l| !l.trim().is_empty() && !l.trim().starts_with('#'))
            .filter_map(|l| l.splitn(2, '=').next().map(|s| s.trim().to_string()))
            .collect();

        let target_vars: std::collections::HashSet<String> = target_content.lines()
            .filter(|l| !l.trim().is_empty() && !l.trim().starts_with('#'))
            .filter_map(|l| l.splitn(2, '=').next().map(|s| s.trim().to_string()))
            .collect();

        let only_in_source: Vec<_> = source_vars.difference(&target_vars).collect();
        let only_in_target: Vec<_> = target_vars.difference(&source_vars).collect();
        let common: Vec<_> = source_vars.intersection(&target_vars).collect();

        Ok(SkillResult::success(serde_json::json!({
            "source": source,
            "target": target,
            "only_in_source": only_in_source,
            "only_in_target": only_in_target,
            "common": common
        })))
    }

    async fn sync_env(&self, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let source = args.get("source").and_then(|v| v.as_str()).unwrap_or(".env.example");
        let target = args.get("target").and_then(|v| v.as_str()).unwrap_or(".env");

        let source_content = tokio::fs::read_to_string(source).await
            .map_err(|e| SkillError::Io(e.to_string()))?;

        let mut target_content = if Path::new(target).exists() {
            tokio::fs::read_to_string(target).await.map_err(|e| SkillError::Io(e.to_string()))?
        } else {
            String::new()
        };

        let mut added = 0;
        for line in source_content.lines() {
            if line.trim().is_empty() || line.trim().starts_with('#') {
                continue;
            }
            if let Some(key) = line.splitn(2, '=').next() {
                let key = key.trim();
                if !target_content.lines().any(|l| l.starts_with(&format!("{}=", key))) {
                    target_content.push_str(&format!("{}=\n", line.splitn(2, '=').next().unwrap_or("")));
                    added += 1;
                }
            }
        }

        tokio::fs::write(target, &target_content).await
            .map_err(|e| SkillError::Io(e.to_string()))?;

        Ok(SkillResult::success(serde_json::json!({
            "source": source,
            "target": target,
            "added": added,
            "success": true
        })))
    }

    async fn list_env(&self, file: &str) -> Result<SkillResult, SkillError> {
        self.read_env(file).await
    }

    async fn export_env(&self, file: &str, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let format = args.get("key").and_then(|v| v.as_str()).unwrap_or("shell");
        let content = tokio::fs::read_to_string(file).await
            .map_err(|e| SkillError::Io(e.to_string()))?;

        let export = match format {
            "shell" | "sh" => content.lines()
                .filter(|l| !l.trim().is_empty() && !l.trim().starts_with('#'))
                .map(|l| format!("export {}", l.trim()))
                .collect::<Vec<_>>()
                .join("\n"),
            "json" => {
                let vars: std::collections::HashMap<String, String> = content.lines()
                    .filter(|l| !l.trim().is_empty() && !l.trim().starts_with('#'))
                    .filter_map(|l| {
                        let mut parts = l.splitn(2, '=');
                        Some((parts.next()?.trim().to_string(), parts.next()?.trim().trim_matches('"').to_string()))
                    })
                    .collect();
                serde_json::to_string_pretty(&vars).unwrap_or_default()
            }
            _ => content.clone(),
        };

        Ok(SkillResult::success(serde_json::json!({
            "format": format,
            "export": export
        })))
    }
}

impl Default for EnvManagerSkill {
    fn default() -> Self { Self::new() }
}
