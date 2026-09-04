use async_trait::async_trait;
use ravenbot_core::Permission;
use crate::traits::{Skill, SkillContext, SkillError, SkillResult};

pub struct HttpRequestSkill { client: reqwest::Client }
impl HttpRequestSkill { pub fn new() -> Self { Self { client: reqwest::Client::new() } } }

#[async_trait]
impl Skill for HttpRequestSkill {
    fn id(&self) -> &str { "http_request" }
    fn name(&self) -> &str { "HTTP Request" }
    fn description(&self) -> &str { "Generic REST: GET/POST/PUT/PATCH/DELETE any URL with headers/body" }
    fn version(&self) -> &str { "1.0.0" }
    fn required_permissions(&self) -> Vec<Permission> { vec![Permission::Network { domains: vec!["*".into()] }] }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({"type":"object","properties":{
            "method":{"type":"string","enum":["GET","POST","PUT","PATCH","DELETE"]},
            "url":{"type":"string"},
            "headers":{"type":"object"},
            "body":{"type":"object"}
        },"required":["method","url"]})
    }
    async fn execute(&self, _ctx: &SkillContext, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let method = args.get("method").and_then(|v| v.as_str()).unwrap_or("GET");
        let url = args.get("url").and_then(|v| v.as_str()).ok_or_else(|| SkillError::InvalidArguments("Missing url".into()))?;
        let mut req = match method {
            "POST" => self.client.post(url),
            "PUT" => self.client.put(url),
            "PATCH" => self.client.patch(url),
            "DELETE" => self.client.delete(url),
            _ => self.client.get(url),
        };
        if let Some(h) = args.get("headers").and_then(|v| v.as_object()) {
            for (k,v) in h { if let Some(s) = v.as_str() { req = req.header(k, s); } }
        }
        if let Some(body) = args.get("body") { req = req.json(body); }
        let resp = req.send().await.map_err(|e| SkillError::Network(e.to_string()))?;
        let status = resp.status().as_u16();
        let headers: std::collections::HashMap<String,String> = resp.headers().iter().map(|(k,v)| (k.to_string(), v.to_str().unwrap_or("").to_string())).collect();
        let text = resp.text().await.unwrap_or_default();
        let json: serde_json::Value = serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text.clone()));
        Ok(SkillResult::success(serde_json::json!({"status": status, "headers": headers, "body": json, "truncated": text.len() > 100000})))
    }
}
impl Default for HttpRequestSkill { fn default() -> Self { Self::new() } }
