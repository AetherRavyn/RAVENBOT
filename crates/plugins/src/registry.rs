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
    /// Build Skill objects for plugins enabled for a bot — appear native (in-app, no external service)
    pub async fn skills_for_bot(&self, bot_id: uuid::Uuid) -> Result<Vec<Arc<dyn Skill>>, String> {
        let rows = self.store.list_enabled_for_bot(bot_id).await?;
        Ok(rows.into_iter().map(|(id, name, desc, _logo)| {
            let skill: Arc<dyn Skill> = Arc::new(OpenApiSkill {
                id: id.clone(), name, description: desc, method: "POST".to_string(),
                url: format!("inapp://plugins/{}", id), // in-app execution, no network
                input_schema: serde_json::json!({"type":"object","properties":{"input":{"type":"string"}}}),
                auth_header: None,
            });
            skill
        }).collect())
    }
    /// 3 meta tools that make 1000+ feel native without blowing context — in-app
    pub fn meta_skills(&self) -> Vec<Arc<dyn Skill>> {
        vec![
            Arc::new(crate::openapi::OpenApiSkill {
                id: "plugin_search".to_string(),
                name: "Plugin Search".to_string(),
                description: "Search 1000+ in-app plugins by keyword (e.g. gmail, notion) — discover tools at runtime".to_string(),
                method: "POST".to_string(),
                url: "inapp://plugins/search".to_string(),
                input_schema: serde_json::json!({"type":"object","properties":{"query":{"type":"string"}},"required":["query"]}),
                auth_header: None,
            }),
            Arc::new(crate::openapi::OpenApiSkill {
                id: "plugin_execute".to_string(),
                name: "Plugin Execute".to_string(),
                description: "Execute a plugin action (e.g. gmail_send) with params — in-app".to_string(),
                method: "POST".to_string(),
                url: "inapp://plugins/execute".to_string(),
                input_schema: serde_json::json!({"type":"object","properties":{"action":{"type":"string"},"params":{"type":"object"}},"required":["action"]}),
                auth_header: None,
            }),
            Arc::new(crate::openapi::OpenApiSkill {
                id: "plugin_connect".to_string(),
                name: "Plugin Connect".to_string(),
                description: "Connect an app (e.g. gmail) — in-app OAuth helper".to_string(),
                method: "POST".to_string(),
                url: "inapp://plugins/connect".to_string(),
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
