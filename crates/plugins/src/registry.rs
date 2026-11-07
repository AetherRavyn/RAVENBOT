use crate::openapi::OpenApiSkill;
use crate::store::PluginStore;
use async_trait::async_trait;
use ravenbot_core::Permission;
use ravenbot_skills::{Skill, SkillContext, SkillError, SkillRegistry, SkillResult};
use serde_json::{json, Value};
use std::sync::Arc;

pub struct PluginRegistry {
    pub store: PluginStore,
}

impl PluginRegistry {
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self {
            store: PluginStore::new(pool),
        }
    }

    /// Build Skill objects for plugins enabled for a bot — each imported
    /// OpenAPI operation becomes a native skill.
    ///
    /// A plugin with zero parsed operations contributes nothing. The old
    /// implementation fabricated a generic "input" stub for those, and the
    /// stub returned a fake success without making a request — an agent could
    /// believe it had sent an email when nothing had happened.
    pub async fn skills_for_bot(&self, bot_id: uuid::Uuid) -> Result<Vec<Arc<dyn Skill>>, String> {
        let usable = self.store.list_usable_for_bot(bot_id).await?;
        let mut skills: Vec<Arc<dyn Skill>> = Vec::new();
        for (plugin_id, name, desc, _logo) in usable {
            let credential = self.store.get_credential(&plugin_id).await?;
            let ops = crate::openapi::load_operations(self.store.pool(), &plugin_id).await?;
            if ops.is_empty() {
                continue;
            }
            for op in ops {
                skills.push(Arc::new(OpenApiSkill {
                    id: op.id,
                    name: if op.name.is_empty() {
                        name.clone()
                    } else {
                        op.name
                    },
                    description: if op.description.is_empty() {
                        desc.clone()
                    } else {
                        op.description
                    },
                    method: op.method,
                    path_template: op.path_template,
                    server_base: op.server_base,
                    input_schema: op.input_schema,
                    body_format: op.body_format,
                    query_parameters: op.query_parameters,
                    auth_header: credential.as_ref().map(|c| c.header_value.clone()),
                    auth_header_name: credential
                        .as_ref()
                        .map(|c| c.header_name.clone())
                        .unwrap_or_else(|| "Authorization".to_string()),
                }));
            }
        }
        Ok(skills)
    }

    /// Three meta tools that let the model discover and call installed
    /// operations without putting every operation in the prompt. They are
    /// backed by the real plugin store; search returns real rows and execute
    /// dispatches through the same authenticated OpenAPI skill path.
    pub fn meta_skills(&self) -> Vec<Arc<dyn Skill>> {
        use PluginMetaKind::*;
        vec![
            Arc::new(PluginMetaSkill {
                kind: Search,
                store: self.store.clone(),
            }),
            Arc::new(PluginMetaSkill {
                kind: Execute,
                store: self.store.clone(),
            }),
            Arc::new(PluginMetaSkill {
                kind: Connect,
                store: self.store.clone(),
            }),
        ]
    }

    /// Merge with built-in registry for a bot — unified native list.
    pub async fn merged_for_bot(
        &self,
        bot_id: uuid::Uuid,
        builtin: &SkillRegistry,
    ) -> Vec<Arc<dyn Skill>> {
        let mut all = builtin.list();
        all.extend(self.meta_skills());
        if let Ok(plugins) = self.skills_for_bot(bot_id).await {
            // Cap direct plugins to 7 more (so total ~10 per bot).
            all.extend(plugins.into_iter().take(7));
        }
        all
    }
}

#[derive(Clone, Copy)]
enum PluginMetaKind {
    Search,
    Execute,
    Connect,
}

struct PluginMetaSkill {
    kind: PluginMetaKind,
    store: PluginStore,
}

impl PluginMetaSkill {
    fn label(&self) -> &'static str {
        match self.kind {
            PluginMetaKind::Search => "plugin_search",
            PluginMetaKind::Execute => "plugin_execute",
            PluginMetaKind::Connect => "plugin_connect",
        }
    }
}

#[async_trait]
impl Skill for PluginMetaSkill {
    fn id(&self) -> &str {
        self.label()
    }

    fn name(&self) -> &str {
        match self.kind {
            PluginMetaKind::Search => "Plugin Search",
            PluginMetaKind::Execute => "Plugin Execute",
            PluginMetaKind::Connect => "Plugin Connect",
        }
    }

    fn description(&self) -> &str {
        match self.kind {
            PluginMetaKind::Search => {
                "Search installed OpenAPI operations by app or action name. Returns only real, imported operations."
            }
            PluginMetaKind::Execute => {
                "Execute an installed OpenAPI operation by its operation id, with parameters. Uses the app's configured credential."
            }
            PluginMetaKind::Connect => {
                "Explain how to add an OpenAPI app and its credential in Plugins & Native Tools."
            }
        }
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn required_permissions(&self) -> Vec<Permission> {
        match self.kind {
            PluginMetaKind::Execute => vec![Permission::Network {
                domains: vec!["*".to_string()],
            }],
            _ => Vec::new(),
        }
    }

    fn input_schema(&self) -> Value {
        match self.kind {
            PluginMetaKind::Search => json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "App, action, or keyword"}
                },
                "required": ["query"]
            }),
            PluginMetaKind::Execute => json!({
                "type": "object",
                "properties": {
                    "action": {"type": "string", "description": "Operation id from plugin_search"},
                    "params": {"type": "object", "description": "Arguments for the operation"}
                },
                "required": ["action"]
            }),
            PluginMetaKind::Connect => json!({
                "type": "object",
                "properties": {
                    "appName": {"type": "string", "description": "App name to connect"}
                },
                "required": ["appName"]
            }),
        }
    }

    async fn execute(&self, ctx: &SkillContext, args: Value) -> Result<SkillResult, SkillError> {
        match self.kind {
            PluginMetaKind::Search => {
                let query = args
                    .get("query")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                if query.is_empty() {
                    return Ok(SkillResult::failure(
                        "plugin_search needs a non-empty query.".to_string(),
                    ));
                }
                let usable = self
                    .store
                    .list_usable_for_bot(ctx.bot_id)
                    .await
                    .map_err(SkillError::Execution)?;
                let allowed: std::collections::HashSet<String> =
                    usable.into_iter().map(|(id, ..)| id).collect();
                let mut matches = self
                    .store
                    .search_operations(query, 50)
                    .await
                    .map_err(SkillError::Execution)?;
                matches.retain(|item| allowed.contains(&item.plugin_id));
                matches.truncate(20);
                Ok(SkillResult::success(json!({
                    "query": query,
                    "results": matches,
                })))
            }
            PluginMetaKind::Execute => {
                let action = args
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                if action.is_empty() {
                    return Ok(SkillResult::failure(
                        "plugin_execute needs an action id from plugin_search.".to_string(),
                    ));
                }
                let params = args.get("params").cloned().unwrap_or_else(|| json!({}));
                let usable = self
                    .store
                    .list_usable_for_bot(ctx.bot_id)
                    .await
                    .map_err(SkillError::Execution)?;
                let plugin = self
                    .store
                    .plugin_for_operation(action)
                    .await
                    .map_err(SkillError::Execution)?;
                let allowed = plugin
                    .as_ref()
                    .map(|plugin| usable.iter().any(|(id, ..)| id == plugin))
                    .unwrap_or(false);
                if !allowed {
                    return Ok(SkillResult::failure(format!(
                        "Operation '{action}' belongs to an app that is not equipped with a usable credential for this agent."
                    )));
                }
                match self.store.operation_for_skill(action).await {
                    Ok(Some((mut operation, credential))) => {
                        if let Some(credential) = credential {
                            operation.auth_header = Some(credential.header_value);
                            operation.auth_header_name = credential.header_name;
                        }
                        operation.execute(ctx, params).await
                    }
                    Ok(None) => Ok(SkillResult::failure(format!(
                        "No installed operation named '{action}'. Use plugin_search first."
                    ))),
                    Err(error) => Ok(SkillResult::failure(error)),
                }
            }
            PluginMetaKind::Connect => {
                let app = args
                    .get("appName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("the app");
                Ok(SkillResult::failure(format!(
                    "'{app}' is not connected. Open Plugins & Native Tools, paste its OpenAPI spec URL, import it, then add the API key or token in the app's details."
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::PluginStore;
    use ravenbot_skills::SkillContext;

    async fn registry() -> (PluginRegistry, uuid::Uuid) {
        let path = std::env::temp_dir().join(format!(
            "ravenbot-plugin-registry-{}.db",
            uuid::Uuid::new_v4()
        ));
        let db = ravenbot_db::Database::new(&path).await.expect("temp db");
        let store = PluginStore::new(db.pool().clone());
        store.ensure_tables().await.expect("tables");
        let bot = ravenbot_core::Bot::new("Plugin Bot", "test");
        ravenbot_db::queries::BotQueries::insert(store.pool(), &bot)
            .await
            .expect("bot");
        (PluginRegistry { store }, bot.id)
    }

    const SPEC: &str = r#"{"openapi":"3.0.0","info":{"title":"Petstore"},"servers":[{"url":"https://api.petstore.test"}],"paths":{"/pets":{"get":{"operationId":"listPets","summary":"List pets"}}}}"#;

    #[tokio::test]
    async fn only_real_operations_become_skills_and_ids_are_qualified() {
        let (registry, bot_id) = registry().await;
        let ops = crate::openapi::parse_openapi_spec(SPEC).unwrap();
        registry
            .store
            .upsert_plugin_full(
                "petstore",
                "Petstore",
                "Pets",
                None,
                Some("https://petstore.test/openapi.json"),
                SPEC,
                false,
                None,
            )
            .await
            .unwrap();
        crate::openapi::store_operations(registry.store.pool(), "petstore", &ops)
            .await
            .unwrap();
        registry
            .store
            .set_bot_plugin(bot_id, "petstore", true)
            .await
            .unwrap();

        // A placeholder app with no parsed operations must contribute nothing.
        registry
            .store
            .upsert_plugin("fake", "Fake", "stub", None, "{}")
            .await
            .unwrap();
        registry
            .store
            .set_bot_plugin(bot_id, "fake", true)
            .await
            .unwrap();

        let skills = registry.skills_for_bot(bot_id).await.unwrap();
        assert_eq!(skills.len(), 1, "fake stubs must not become skills");
        assert_eq!(skills[0].id(), "petstore__listpets");
    }

    #[tokio::test]
    async fn plugin_search_reads_the_real_operation_index() {
        let (registry, bot_id) = registry().await;
        let ops = crate::openapi::parse_openapi_spec(SPEC).unwrap();
        registry
            .store
            .upsert_plugin_full(
                "petstore",
                "Petstore",
                "Pets",
                None,
                Some("https://petstore.test/openapi.json"),
                SPEC,
                false,
                None,
            )
            .await
            .unwrap();
        crate::openapi::store_operations(registry.store.pool(), "petstore", &ops)
            .await
            .unwrap();
        registry
            .store
            .set_bot_plugin(bot_id, "petstore", true)
            .await
            .unwrap();

        let search = registry
            .meta_skills()
            .into_iter()
            .find(|skill| skill.id() == "plugin_search")
            .expect("search skill");
        let context = SkillContext::new(
            bot_id,
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            ravenbot_core::SandboxTier::OsLevel,
        );
        let result = search
            .execute(&context, serde_json::json!({"query":"pets"}))
            .await
            .expect("search");
        assert!(result.success);
        let results = result
            .output
            .get("results")
            .and_then(|value| value.as_array())
            .expect("results");
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0]
                .get("operation_id")
                .and_then(|value| value.as_str()),
            Some("petstore__listpets")
        );
    }

    #[tokio::test]
    async fn auth_required_apps_need_a_credential_before_becoming_tools() {
        let (registry, bot_id) = registry().await;
        let ops = crate::openapi::parse_openapi_spec(SPEC).unwrap();
        registry
            .store
            .upsert_plugin_full(
                "petstore",
                "Petstore",
                "Pets",
                None,
                Some("https://petstore.test/openapi.json"),
                SPEC,
                true,
                Some("Authorization"),
            )
            .await
            .unwrap();
        crate::openapi::store_operations(registry.store.pool(), "petstore", &ops)
            .await
            .unwrap();
        registry
            .store
            .set_bot_plugin(bot_id, "petstore", true)
            .await
            .unwrap();

        assert!(
            registry.skills_for_bot(bot_id).await.unwrap().is_empty(),
            "an auth-required app without a key must not become a tool"
        );

        let search = registry
            .meta_skills()
            .into_iter()
            .find(|skill| skill.id() == "plugin_search")
            .expect("search skill");
        let context = SkillContext::new(
            bot_id,
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            ravenbot_core::SandboxTier::OsLevel,
        );
        let hidden = search
            .execute(&context, serde_json::json!({"query":"pets"}))
            .await
            .expect("search");
        assert!(
            hidden.output["results"].as_array().unwrap().is_empty(),
            "auth-required app leaked into search without a key"
        );

        registry
            .store
            .set_credential("petstore", "Authorization", "Bearer test")
            .await
            .unwrap();
        let skills = registry.skills_for_bot(bot_id).await.unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id(), "petstore__listpets");
        let visible = search
            .execute(&context, serde_json::json!({"query":"pets"}))
            .await
            .expect("search after key");
        assert_eq!(visible.output["results"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn search_does_not_leak_unequipped_apps() {
        let (registry, bot_id) = registry().await;
        let ops = crate::openapi::parse_openapi_spec(SPEC).unwrap();
        registry
            .store
            .upsert_plugin_full(
                "petstore",
                "Petstore",
                "Pets",
                None,
                Some("https://petstore.test/openapi.json"),
                SPEC,
                false,
                None,
            )
            .await
            .unwrap();
        crate::openapi::store_operations(registry.store.pool(), "petstore", &ops)
            .await
            .unwrap();

        let search = registry
            .meta_skills()
            .into_iter()
            .find(|skill| skill.id() == "plugin_search")
            .expect("search skill");
        let context = SkillContext::new(
            bot_id,
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            ravenbot_core::SandboxTier::OsLevel,
        );
        let result = search
            .execute(&context, serde_json::json!({"query":"pets"}))
            .await
            .expect("search");
        let results = result.output["results"].as_array().expect("results");
        assert!(results.is_empty(), "unequipped app leaked: {results:?}");

        registry
            .store
            .set_bot_plugin(bot_id, "petstore", true)
            .await
            .unwrap();
        let result = search
            .execute(&context, serde_json::json!({"query":"pets"}))
            .await
            .expect("search after equip");
        assert_eq!(result.output["results"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn connect_without_credentials_is_an_honest_failure() {
        let (registry, bot_id) = registry().await;
        let connect = registry
            .meta_skills()
            .into_iter()
            .find(|skill| skill.id() == "plugin_connect")
            .expect("connect skill");
        let context = SkillContext::new(
            bot_id,
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            ravenbot_core::SandboxTier::OsLevel,
        );
        let result = connect
            .execute(&context, serde_json::json!({"appName":"Gmail"}))
            .await
            .expect("connect");
        assert!(!result.success);
        assert!(result
            .error
            .unwrap()
            .contains("Open Plugins & Native Tools"));
    }
}
