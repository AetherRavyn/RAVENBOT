//! Tavily Search — research-grade (beats DuckLite)

use crate::traits::{Skill, SkillContext, SkillError, SkillResult, SkillRisk};
use async_trait::async_trait;
use ravenbot_core::Permission;

pub struct TavilySearchSkill {
    client: reqwest::Client,
}

impl TavilySearchSkill {
    pub fn new() -> Self {
        Self {
            client: crate::http::http_client(),
        }
    }
}

#[async_trait]
impl Skill for TavilySearchSkill {
    fn id(&self) -> &str {
        "tavily_search"
    }
    fn name(&self) -> &str {
        "Tavily Search"
    }
    fn description(&self) -> &str {
        "Research-grade web search via Tavily (or fallback to DuckLite if no key)"
    }
    fn version(&self) -> &str {
        "1.0.0"
    }
    fn required_permissions(&self) -> Vec<Permission> {
        vec![Permission::Network {
            domains: vec!["*".into()],
        }]
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type":"object","properties":{
                "query":{"type":"string","description":"Research query"},
                "include_answer":{"type":"boolean","description":"Include AI answer"},
                "max_results":{"type":"integer","minimum":1,"maximum":10}
            },"required":["query"]
        })
    }
    fn risk(&self) -> SkillRisk {
        SkillRisk::ReadOnly
    }

    async fn execute(
        &self,
        ctx: &SkillContext,
        args: serde_json::Value,
    ) -> Result<SkillResult, SkillError> {
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing query".into()))?;
        let max = args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(5);
        // Try Tavily if key in env, else fallback to web_search logic
        if let Ok(key) = std::env::var("TAVILY_API_KEY") {
            ctx.ensure_network_allowed("https://api.tavily.com/search")
                .map_err(SkillError::InvalidArguments)?;
            let res = self.client.post("https://api.tavily.com/search")
                .json(&serde_json::json!({"api_key": key, "query": query, "max_results": max, "include_answer": true}))
                .send().await;
            if let Ok(r) = res {
                // A rejected key or quota error is not a paid search. Falling
                // through keeps the free fallback working and avoids charging
                // for an error body parsed as a result.
                if r.status().is_success() {
                    if let Ok(mut j) = r.json::<serde_json::Value>().await {
                        if let Some(object) = j.as_object_mut() {
                            object.insert("provider".to_string(), serde_json::json!("tavily"));
                        }
                        return Ok(SkillResult::success(j));
                    }
                }
            }
        }
        // Fallback: use DuckLite via web_search skill
        let fallback = crate::builtin::web_search::WebSearchSkill::new();
        // Carry the confinement across with the rest of the context, so a
        // delegated call is bound exactly like the original.
        let ctx = ctx.clone_for_delegation();
        fallback
            .execute(
                &ctx,
                serde_json::json!({"query": query, "max_results": max}),
            )
            .await
    }
}
impl Default for TavilySearchSkill {
    fn default() -> Self {
        Self::new()
    }
}
