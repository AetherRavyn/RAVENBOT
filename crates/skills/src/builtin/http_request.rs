use crate::traits::{Skill, SkillContext, SkillError, SkillResult, SkillRisk};
use async_trait::async_trait;
use ravenbot_core::Permission;

pub struct HttpRequestSkill;
impl HttpRequestSkill {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Skill for HttpRequestSkill {
    fn id(&self) -> &str {
        "http_request"
    }
    fn name(&self) -> &str {
        "HTTP Request"
    }
    fn description(&self) -> &str {
        "Generic REST: GET/POST/PUT/PATCH/DELETE any URL with headers/body"
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
        serde_json::json!({"type":"object","properties":{
            "method":{"type":"string","enum":["GET","POST","PUT","PATCH","DELETE"]},
            "url":{"type":"string"},
            "headers":{"type":"object"},
            "body":{"type":"object"}
        },"required":["method","url"]})
    }
    fn risk(&self) -> SkillRisk {
        // GET can read internal services; POST/PUT/PATCH/DELETE can mutate
        // them, and every response is returned to the model. None of that is a
        // pure local read, so `Ask` must gate it.
        SkillRisk::High
    }

    async fn execute(
        &self,
        ctx: &SkillContext,
        args: serde_json::Value,
    ) -> Result<SkillResult, SkillError> {
        let method = args.get("method").and_then(|v| v.as_str()).unwrap_or("GET");
        let url = args
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing url".into()))?;
        let url = ctx
            .ensure_network_allowed(url)
            .map_err(SkillError::InvalidArguments)?;
        let (client, url) = crate::http::pinned_client(url.as_str())
            .await
            .map_err(SkillError::Network)?;
        let mut req = match method {
            "POST" => client.post(url),
            "PUT" => client.put(url),
            "PATCH" => client.patch(url),
            "DELETE" => client.delete(url),
            _ => client.get(url),
        };
        if let Some(h) = args.get("headers").and_then(|v| v.as_object()) {
            for (k, v) in h {
                if let Some(s) = v.as_str() {
                    req = req.header(k, s);
                }
            }
        }
        if let Some(body) = args.get("body") {
            req = req.json(body);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| SkillError::Network(e.to_string()))?;
        let status = resp.status().as_u16();
        let headers: std::collections::HashMap<String, String> = resp
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();
        let text = resp.text().await.unwrap_or_default();
        let json: serde_json::Value =
            serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text.clone()));
        Ok(SkillResult::success(
            serde_json::json!({"status": status, "headers": headers, "body": json, "truncated": text.len() > 100000}),
        ))
    }
}
impl Default for HttpRequestSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::SkillContext;
    use uuid::Uuid;

    fn context() -> SkillContext {
        SkillContext::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            ravenbot_core::SandboxTier::Host,
        )
    }

    #[tokio::test]
    async fn metadata_and_non_http_urls_are_refused_before_any_request() {
        let skill = HttpRequestSkill::new();
        for url in [
            "http://169.254.169.254/latest/meta-data",
            "http://metadata.google.internal/computeMetadata/v1",
            "file:///etc/passwd",
        ] {
            let error = skill
                .execute(&context(), serde_json::json!({"method": "GET", "url": url}))
                .await
                .expect_err("url must be refused");
            assert!(
                matches!(error, SkillError::InvalidArguments(_)),
                "expected a validation error, got {error:?}"
            );
        }
    }
}
