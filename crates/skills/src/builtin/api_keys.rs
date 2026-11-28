//! API-key tools.
//!
//! `save_api_key` is intercepted by the runtime: it parks the run on the same
//! secure card as an `ask_user` `vault`/`api_key` request, and the owner enters
//! the value in the desktop app. This module exists so the model sees the tool,
//! and so `list_api_keys` and `remove_api_key` work like ordinary DB-backed
//! tools. No tool here accepts or returns a secret value.

use async_trait::async_trait;
use ravenbot_core::Permission;
use serde_json::{json, Value};

use crate::traits::{Skill, SkillContext, SkillError, SkillResult, SkillRisk};

fn required_str<'a>(arguments: &'a Value, key: &str) -> Result<&'a str, SkillError> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| SkillError::InvalidArguments(format!("Missing or invalid `{key}`.")))
}

pub struct ListApiKeysSkill;

impl ListApiKeysSkill {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ListApiKeysSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Skill for ListApiKeysSkill {
    fn id(&self) -> &str {
        "list_api_keys"
    }

    fn name(&self) -> &str {
        "List API keys"
    }

    fn description(&self) -> &str {
        "List the API keys stored in the local Vault. Returns each key's name, label, website and owner-only file path, plus whether the file exists. It never returns the value. Use the path in a command substitution, for example `curl -H \"Authorization: Bearer $(cat /path)\"`."
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn required_permissions(&self) -> Vec<Permission> {
        vec![]
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }

    fn risk(&self) -> SkillRisk {
        SkillRisk::ReadOnly
    }

    async fn execute(&self, ctx: &SkillContext, _args: Value) -> Result<SkillResult, SkillError> {
        let pool = ctx.db_pool()?;
        let keys = ravenbot_db::api_keys::list(pool)
            .await
            .map_err(SkillError::Execution)?;
        let items: Vec<Value> = keys
            .into_iter()
            .map(|key| {
                let path = ravenbot_core::api_keys::api_key_path(&key.name).ok();
                let path_string = path
                    .as_deref()
                    .and_then(std::path::Path::to_str)
                    .map(str::to_string);
                let exists = path.as_deref().is_some_and(std::path::Path::exists);
                json!({
                    "name": key.name,
                    "label": key.label,
                    "website": key.website,
                    "path": path_string,
                    "file_exists": exists,
                    "created_at": key.created_at,
                    "updated_at": key.updated_at,
                })
            })
            .collect();
        Ok(SkillResult::success(json!({
            "keys": items,
            "count": items.len(),
            "note": "Values are never returned. The path is readable by your user account; use it without printing the contents."
        })))
    }
}

pub struct SaveApiKeySkill;

impl SaveApiKeySkill {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SaveApiKeySkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Skill for SaveApiKeySkill {
    fn id(&self) -> &str {
        "save_api_key"
    }

    fn name(&self) -> &str {
        "Request an API key"
    }

    fn description(&self) -> &str {
        "Ask the owner to enter an API key in a secure desktop card and store it in the local Vault. Give the key a machine-safe `name` (lowercase letters, digits, '.', '_' or '-') and an optional `label`, `website` and `purpose`. The owner types the value; this tool never accepts it. On success you receive only the name and the owner-only file path. Use `list_api_keys` to see stored keys and `remove_api_key` to delete one."
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn required_permissions(&self) -> Vec<Permission> {
        vec![]
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Machine-safe key name, e.g. openai or anthropic. Lowercase letters, digits, '.', '_' or '-'; at most 64 characters."
                },
                "label": {
                    "type": "string",
                    "description": "Human label shown on the card and in the Vault, e.g. 'OpenAI API key'."
                },
                "website": {
                    "type": "string",
                    "description": "Optional site the key belongs to, shown as context."
                },
                "purpose": {
                    "type": "string",
                    "description": "One line explaining what the key will be used for."
                }
            },
            "required": ["name"],
            "additionalProperties": false
        })
    }

    fn risk(&self) -> SkillRisk {
        // The card is the interaction; the tool itself only asks.
        SkillRisk::ReadOnly
    }

    async fn execute(&self, _ctx: &SkillContext, _args: Value) -> Result<SkillResult, SkillError> {
        // The runtime intercepts this name before registry execution. A
        // standalone registry has no way to show the secure card, and silently
        // accepting a value there would defeat the whole design.
        Err(SkillError::Execution(
            "save_api_key needs the desktop runtime's secure card; standalone registry execution is unsupported."
                .to_string(),
        ))
    }
}

pub struct RemoveApiKeySkill;

impl RemoveApiKeySkill {
    pub fn new() -> Self {
        Self
    }
}

impl Default for RemoveApiKeySkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Skill for RemoveApiKeySkill {
    fn id(&self) -> &str {
        "remove_api_key"
    }

    fn name(&self) -> &str {
        "Remove an API key"
    }

    fn description(&self) -> &str {
        "Delete one stored API key and its owner-only file. Use `list_api_keys` first to confirm the name. The value is never returned."
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn required_permissions(&self) -> Vec<Permission> {
        vec![]
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Machine-safe key name from list_api_keys."
                }
            },
            "required": ["name"],
            "additionalProperties": false
        })
    }

    fn risk(&self) -> SkillRisk {
        SkillRisk::High
    }

    async fn execute(&self, ctx: &SkillContext, args: Value) -> Result<SkillResult, SkillError> {
        let name = required_str(&args, "name")?;
        let pool = ctx.db_pool()?;
        let removed = ravenbot_db::api_keys::remove(pool, name)
            .await
            .map_err(SkillError::Execution)?;
        if removed {
            Ok(SkillResult::success(json!({
                "removed": true,
                "name": name,
            })))
        } else {
            Ok(SkillResult::failure(format!(
                "No API key named '{name}' is stored."
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;
    use uuid::Uuid;

    async fn context() -> (SkillContext, sqlx::SqlitePool, std::path::PathBuf) {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        ravenbot_db::migrations::run(&pool)
            .await
            .expect("migrations");
        let ctx = SkillContext::with_default_tier(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4())
            .with_db(pool.clone());
        let dir = std::env::temp_dir().join(format!("rb-api-skill-{}", Uuid::new_v4()));
        (ctx, pool, dir)
    }

    #[tokio::test]
    async fn list_returns_metadata_and_never_the_value() {
        let (ctx, pool, dir) = context().await;
        ravenbot_db::api_keys::store_in(
            &pool,
            &dir,
            "openai",
            "OpenAI API key",
            "https://openai.com",
            "sk-skill-secret",
        )
        .await
        .expect("store");

        let result = ListApiKeysSkill::new()
            .execute(&ctx, json!({}))
            .await
            .expect("list");
        assert!(result.success);
        let text = serde_json::to_string(&result.output).unwrap();
        assert!(text.contains("\"name\":\"openai\""), "{text}");
        assert!(text.contains("\"label\":\"OpenAI API key\""), "{text}");
        assert!(text.contains("api-keys/openai"), "{text}");
        assert!(!text.contains("sk-skill-secret"), "{text}");
        assert!(!text.contains("\"secret\""), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn remove_deletes_the_row_and_is_honest_about_a_missing_name() {
        let (ctx, pool, dir) = context().await;
        ravenbot_db::api_keys::store_in(
            &pool,
            &dir,
            "openai",
            "OpenAI API key",
            "",
            "sk-skill-secret",
        )
        .await
        .expect("store");

        let removed = RemoveApiKeySkill::new()
            .execute(&ctx, json!({ "name": "openai" }))
            .await
            .expect("remove");
        assert!(removed.success);
        assert!(ravenbot_db::api_keys::list(&pool).await.unwrap().is_empty());

        let missing = RemoveApiKeySkill::new()
            .execute(&ctx, json!({ "name": "openai" }))
            .await
            .expect("missing");
        assert!(!missing.success);
        assert!(missing
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("No API key"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn save_without_the_runtime_is_refused_instead_of_accepting_a_value() {
        let (ctx, _, _) = context().await;
        let error = SaveApiKeySkill::new()
            .execute(
                &ctx,
                json!({ "name": "openai", "value": "sk-must-not-pass" }),
            )
            .await
            .expect_err("standalone");
        assert!(error.to_string().contains("secure card"));
    }

    #[test]
    fn the_save_schema_has_no_secret_field() {
        let schema = serde_json::to_string(&SaveApiKeySkill::new().input_schema()).unwrap();
        assert!(schema.contains("\"name\""));
        assert!(!schema.contains("\"value\""));
        assert!(!schema.contains("\"secret\""));
        assert!(!schema.contains("\"api_key\""));
    }
}
