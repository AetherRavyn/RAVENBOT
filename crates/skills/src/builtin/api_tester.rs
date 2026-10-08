//! API tester — test REST/GraphQL/gRPC endpoints with collections, assertions, and reporting

use async_trait::async_trait;
use ravenbot_core::Permission;
use crate::traits::{Skill, SkillContext, SkillError, SkillResult};

pub struct ApiTesterSkill {
    client: reqwest::Client,
}

impl ApiTesterSkill { pub fn new() -> Self { Self { client: reqwest::Client::builder().timeout(std::time::Duration::from_secs(30)).build().unwrap_or_default() } } }

#[async_trait]
impl Skill for ApiTesterSkill {
    fn id(&self) -> &str { "api_tester" }
    fn name(&self) -> &str { "API Tester" }
    fn description(&self) -> &str { "Test REST/GraphQL endpoints: send requests, assert responses, chain requests, run collections" }
    fn version(&self) -> &str { "1.0.0" }
    fn required_permissions(&self) -> Vec<Permission> {
        vec![Permission::Network { domains: vec!["*".to_string()] }]
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["request", "collection", "graphql", "websocket_test", "health_check"],
                    "description": "Type of API test"
                },
                "method": {
                    "type": "string",
                    "enum": ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"],
                    "description": "HTTP method"
                },
                "url": {"type": "string", "description": "Endpoint URL"},
                "headers": {"type": "object", "description": "Request headers"},
                "body": {"type": "object", "description": "Request body (JSON)"},
                "query": {"type": "object", "description": "Query parameters"},
                "assertions": {
                    "type": "object",
                    "description": "Assertions to run on response",
                    "properties": {
                        "status": {"type": "integer", "description": "Expected status code"},
                        "body_contains": {"type": "string", "description": "Body should contain"},
                        "body_not_contains": {"type": "string", "description": "Body should not contain"},
                        "header_exists": {"type": "string", "description": "Header that should exist"},
                        "max_response_time_ms": {"type": "integer", "description": "Max acceptable response time"},
                        "json_path": {"type": "string", "description": "JSON path to check"},
                        "json_value": {"type": "string", "description": "Expected value at JSON path"}
                    }
                },
                "collection": {
                    "type": "array",
                    "description": "Array of requests to run in sequence",
                    "items": {"type": "object"}
                },
                "iterations": {"type": "integer", "description": "Number of iterations for load test (default: 1, max: 100)"},
                "delay_ms": {"type": "integer", "description": "Delay between requests in ms (default: 0)"}
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, _ctx: &SkillContext, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let action = args.get("action").and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'action'".into()))?;

        match action {
            "request" => self.single_request(args).await,
            "collection" => self.run_collection(args).await,
            "graphql" => self.graphql_request(args).await,
            "health_check" => self.health_check(args).await,
            "websocket_test" => Ok(SkillResult::failure("WebSocket testing not yet implemented".to_string())),
            _ => Err(SkillError::InvalidArguments(format!("Unknown action: {}", action))),
        }
    }
}

impl ApiTesterSkill {
    async fn single_request(&self, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let method = args.get("method").and_then(|v| v.as_str()).unwrap_or("GET");
        let url = args.get("url").and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'url'".into()))?;

        let mut req = match method {
            "POST" => self.client.post(url),
            "PUT" => self.client.put(url),
            "PATCH" => self.client.patch(url),
            "DELETE" => self.client.delete(url),
            "HEAD" => self.client.head(url),
            _ => self.client.get(url),
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

        if let Some(q) = args.get("query").and_then(|v| v.as_object()) {
            let params: Vec<(String, String)> = q.iter()
                .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                .collect();
            if !params.is_empty() {
                req = req.query(&params);
            }
        }

        let start = std::time::Instant::now();
        let resp = req.send().await.map_err(|e| SkillError::Network(e.to_string()))?;
        let elapsed = start.elapsed();

        let status = resp.status().as_u16();
        let headers: std::collections::HashMap<String, String> = resp.headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();
        let body_text = resp.text().await.unwrap_or_default();
        let body_json: serde_json::Value = serde_json::from_str(&body_text)
            .unwrap_or(serde_json::Value::String(body_text.clone()));

        // Run assertions
        let assertions = args.get("assertions");
        let assertion_results = self.run_assertions(assertions, status, &body_text, &body_json, elapsed);

        Ok(SkillResult::success(serde_json::json!({
            "status": status,
            "headers": headers,
            "body": body_json,
            "response_time_ms": elapsed.as_millis(),
            "assertions": assertion_results,
            "all_passed": assertion_results.as_array().map(|arr| arr.iter().all(|a| a["passed"].as_bool().unwrap_or(false))).unwrap_or(true)
        })))
    }

    async fn run_collection(&self, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let collection = args.get("collection").and_then(|v| v.as_array())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'collection'".into()))?;
        let iterations = args.get("iterations").and_then(|v| v.as_u64()).unwrap_or(1).min(100) as usize;
        let delay_ms = args.get("delay_ms").and_then(|v| v.as_u64()).unwrap_or(0);

        let mut results = Vec::new();
        let mut total_passed = 0;
        let mut total_failed = 0;

        for iter in 0..iterations {
            for (i, req_def) in collection.iter().enumerate() {
                let mut req_args = req_def.clone();
                if req_args.is_object() {
                    req_args["action"] = serde_json::json!("request");
                }

                match self.single_request(req_args).await {
                    Ok(result) => {
                        let passed = result.output["all_passed"].as_bool().unwrap_or(false);
                        if passed { total_passed += 1; } else { total_failed += 1; }
                        results.push(serde_json::json!({
                            "iteration": iter + 1,
                            "step": i + 1,
                            "passed": passed,
                            "result": result.output
                        }));
                    }
                    Err(e) => {
                        total_failed += 1;
                        results.push(serde_json::json!({
                            "iteration": iter + 1,
                            "step": i + 1,
                            "passed": false,
                            "error": e.to_string()
                        }));
                    }
                }

                if delay_ms > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                }
            }
        }

        Ok(SkillResult::success(serde_json::json!({
            "iterations": iterations,
            "total_steps": collection.len() * iterations,
            "passed": total_passed,
            "failed": total_failed,
            "results": results
        })))
    }

    async fn graphql_request(&self, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let url = args.get("url").and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'url'".into()))?;
        let query = args.get("body").and_then(|v| v.get("query")).and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing query in body".into()))?;

        let mut req = self.client.post(url).json(&serde_json::json!({ "query": query }));

        if let Some(h) = args.get("headers").and_then(|v| v.as_object()) {
            for (k, v) in h {
                if let Some(s) = v.as_str() {
                    req = req.header(k, s);
                }
            }
        }

        let start = std::time::Instant::now();
        let resp = req.send().await.map_err(|e| SkillError::Network(e.to_string()))?;
        let elapsed = start.elapsed();

        let status = resp.status().as_u16();
        let body_text = resp.text().await.unwrap_or_default();
        let body_json: serde_json::Value = serde_json::from_str(&body_text)
            .unwrap_or(serde_json::Value::String(body_text.clone()));

        Ok(SkillResult::success(serde_json::json!({
            "status": status,
            "body": body_json,
            "response_time_ms": elapsed.as_millis(),
            "has_errors": body_json.get("errors").is_some()
        })))
    }

    async fn health_check(&self, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let url = args.get("url").and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'url'".into()))?;
        let iterations = args.get("iterations").and_then(|v| v.as_u64()).unwrap_or(3).min(20);

        let mut results = Vec::new();
        for _ in 0..iterations {
            let start = std::time::Instant::now();
            match self.client.get(url).send().await {
                Ok(resp) => {
                    results.push(serde_json::json!({
                        "status": resp.status().as_u16(),
                        "response_time_ms": start.elapsed().as_millis(),
                        "healthy": resp.status().is_success()
                    }));
                }
                Err(e) => {
                    results.push(serde_json::json!({
                        "status": 0,
                        "response_time_ms": start.elapsed().as_millis(),
                        "healthy": false,
                        "error": e.to_string()
                    }));
                }
            }
        }

        let healthy_count = results.iter().filter(|r| r["healthy"].as_bool().unwrap_or(false)).count();
        Ok(SkillResult::success(serde_json::json!({
            "url": url,
            "iterations": iterations,
            "healthy_count": healthy_count,
            "unhealthy_count": iterations as usize - healthy_count,
            "results": results
        })))
    }

    fn run_assertions(
        &self,
        assertions: Option<&serde_json::Value>,
        status: u16,
        body_text: &str,
        _body_json: &serde_json::Value,
        elapsed: std::time::Duration,
    ) -> serde_json::Value {
        let mut results = Vec::new();
        let assertions = match assertions {
            Some(a) => a,
            None => return serde_json::json!([]),
        };

        if let Some(expected) = assertions.get("status").and_then(|v| v.as_u64()) {
            let passed = status as u64 == expected;
            results.push(serde_json::json!({
                "type": "status",
                "expected": expected,
                "actual": status,
                "passed": passed
            }));
        }

        if let Some(needle) = assertions.get("body_contains").and_then(|v| v.as_str()) {
            let passed = body_text.contains(needle);
            results.push(serde_json::json!({
                "type": "body_contains",
                "expected": needle,
                "passed": passed
            }));
        }

        if let Some(needle) = assertions.get("body_not_contains").and_then(|v| v.as_str()) {
            let passed = !body_text.contains(needle);
            results.push(serde_json::json!({
                "type": "body_not_contains",
                "expected": needle,
                "passed": passed
            }));
        }

        if let Some(max_ms) = assertions.get("max_response_time_ms").and_then(|v| v.as_u64()) {
            let passed = elapsed.as_millis() as u64 <= max_ms;
            results.push(serde_json::json!({
                "type": "max_response_time_ms",
                "expected": max_ms,
                "actual": elapsed.as_millis(),
                "passed": passed
            }));
        }

        serde_json::json!(results)
    }
}

impl Default for ApiTesterSkill {
    fn default() -> Self { Self::new() }
}
