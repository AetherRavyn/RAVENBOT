use crate::openapi::OpenApiSkill;
use ravenbot_skills::{Skill, SkillRegistry};
use std::sync::Arc;

pub struct PluginRegistry {
    pub store: crate::store::PluginStore,
}

impl PluginRegistry {
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self { store: crate::store::PluginStore::new(pool) }
    }
    /// Build Skill objects for plugins enabled for a bot — each imported
    /// OpenAPI operation becomes a native skill. Falls back to an in-app
    /// stub only when the plugin has no parsed operations yet.
    pub async fn skills_for_bot(&self, bot_id: uuid::Uuid) -> Result<Vec<Arc<dyn Skill>>, String> {
        let enabled = self.store.list_enabled_for_bot(bot_id).await?;
        let mut skills: Vec<Arc<dyn Skill>> = Vec::new();
        for (plugin_id, name, desc, _logo) in enabled {
            let ops = crate::openapi::load_operations(self.store.pool(), &plugin_id).await?;
            if ops.is_empty() {
                // No parsed operations yet: expose one in-app stub so the
                // plugin still appears in the skill list (will be replaced
                // after a successful import parses its spec).
                skills.push(Arc::new(OpenApiSkill {
                    id: plugin_id.clone(),
                    name,
                    description: desc,
                    method: "POST".to_string(),
                    path_template: String::new(),
                    server_base: format!("inapp://plugins/{}", plugin_id),
                    input_schema: serde_json::json!({"type":"object","properties":{"input":{"type":"string"}}}),
                    auth_header: None,
                }));
            } else {
                for op in ops {
                    skills.push(Arc::new(OpenApiSkill {
                        id: op.id,
                        name: op.name,
                        description: op.description,
                        method: op.method,
                        path_template: op.path_template,
                        server_base: op.server_base,
                        input_schema: op.input_schema,
                        auth_header: op.auth_header,
                    }));
                }
            }
        }
        Ok(skills)
    }
    /// 3 meta tools that make 1000+ feel native without blowing context — in-app
    pub fn meta_skills(&self) -> Vec<Arc<dyn Skill>> {
        vec![
            Arc::new(crate::openapi::OpenApiSkill {
                id: "plugin_search".to_string(),
                name: "Plugin Search".to_string(),
                description: "Search 1000+ in-app plugins by keyword (e.g. gmail, notion) — discover tools at runtime".to_string(),
                method: "POST".to_string(),
                path_template: String::new(),
                server_base: "inapp://plugins/search".to_string(),
                input_schema: serde_json::json!({"type":"object","properties":{"query":{"type":"string"}},"required":["query"]}),
                auth_header: None,
            }),
            Arc::new(crate::openapi::OpenApiSkill {
                id: "plugin_execute".to_string(),
                name: "Plugin Execute".to_string(),
                description: "Execute a plugin action (e.g. gmail_send) with params — in-app".to_string(),
                method: "POST".to_string(),
                path_template: String::new(),
                server_base: "inapp://plugins/execute".to_string(),
                input_schema: serde_json::json!({"type":"object","properties":{"action":{"type":"string"},"params":{"type":"object"}},"required":["action"]}),
                auth_header: None,
            }),
            Arc::new(crate::openapi::OpenApiSkill {
                id: "plugin_connect".to_string(),
                name: "Plugin Connect".to_string(),
                description: "Connect an app (e.g. gmail) — in-app OAuth helper".to_string(),
                method: "POST".to_string(),
                path_template: String::new(),
                server_base: "inapp://plugins/connect".to_string(),
                input_schema: serde_json::json!({"type":"object","properties":{"appName":{"type":"string"}},"required":["appName"]}),
                auth_header: None,
            }),
        ]
    }

    /// Merge with built-in registry for a bot — unified native list
    pub async fn merged_for_bot(&self, bot_id: uuid::Uuid, builtin: &SkillRegistry) -> Vec<Arc<dyn Skill>> {
        let mut all = builtin.list();
        // Always include 3 meta tools — one session = 1000 apps, no context bloat
        all.extend(self.meta_skills());
        if let Ok(plugins) = self.skills_for_bot(bot_id).await {
            // Cap direct plugins to 7 more (so total ~10 per bot, vector-search in prod)
            all.extend(plugins.into_iter().take(7));
        }
        all
    }
}
