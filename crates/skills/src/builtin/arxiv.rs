use async_trait::async_trait;
use ravenbot_core::Permission;
use crate::traits::{Skill, SkillContext, SkillError, SkillResult};

pub struct ArxivSkill { client: reqwest::Client }
impl ArxivSkill { pub fn new() -> Self { Self { client: reqwest::Client::new() } } }

#[async_trait]
impl Skill for ArxivSkill {
    fn id(&self) -> &str { "arxiv_search" }
    fn name(&self) -> &str { "ArXiv Search" }
    fn description(&self) -> &str { "Search arXiv papers — research lane" }
    fn version(&self) -> &str { "1.0.0" }
    fn required_permissions(&self) -> Vec<Permission> { vec![Permission::Network { domains: vec!["export.arxiv.org".into(), "arxiv.org".into()] }] }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({"type":"object","properties":{"query":{"type":"string"},"max_results":{"type":"integer","minimum":1,"maximum":10}},"required":["query"]})
    }
    async fn execute(&self, _ctx: &SkillContext, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let query = args.get("query").and_then(|v| v.as_str()).ok_or_else(|| SkillError::InvalidArguments("Missing query".into()))?;
        let max = args.get("max_results").and_then(|v| v.as_u64()).unwrap_or(5);
        let url = format!("http://export.arxiv.org/api/query?search_query=all:{}&start=0&max_results={}", urlencoding(query), max);
        let resp = self.client.get(&url).send().await.map_err(|e| SkillError::Network(e.to_string()))?;
        let xml = resp.text().await.map_err(|e| SkillError::Network(e.to_string()))?;
        // Simple parse: extract title/summary via string ops (no extra deps)
        let papers: Vec<serde_json::Value> = xml.split("<entry>").skip(1).take(max as usize).map(|e| {
            let title = between(e, "<title>", "</title>").unwrap_or("Untitled").trim().to_string();
            let summary = between(e, "<summary>", "</summary>").unwrap_or("").trim().chars().take(300).collect::<String>();
            let id = between(e, "<id>", "</id>").unwrap_or("").trim().to_string();
            serde_json::json!({"title": title, "summary": summary, "id": id})
        }).collect();
        Ok(SkillResult::success(serde_json::json!({"query": query, "papers": papers})))
    }
}
fn between<'a>(s: &'a str, a: &str, b: &str) -> Option<&'a str> { s.find(a).and_then(|i| s[i+a.len()..].find(b).map(|j| &s[i+a.len()..i+a.len()+j])) }
fn urlencoding(s: &str) -> String { s.replace(' ', "+") }
impl Default for ArxivSkill { fn default() -> Self { Self::new() } }
