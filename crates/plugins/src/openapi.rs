use async_trait::async_trait;
use ravenbot_core::Permission;
use ravenbot_skills::{Skill, SkillContext, SkillError, SkillResult};
use serde_json::Value;
use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct OpenApiSkill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub method: String,
    pub path_template: String,
    pub server_base: String,
    pub input_schema: Value,
    pub auth_header: Option<String>,
}

impl OpenApiSkill {
    pub fn operation_id(method: &str, path: &str, fallback: &str) -> String {
        // Build a stable, readable id: get_pet_by_id, list_users, …
        let mut id = format!("{}_{}", method.to_lowercase(), path);
        id = id.replace(|c: char| !c.is_alphanumeric() && c != '_', "_");
        id = id.replace("__", "_").trim_matches('_').to_string();
        if id.is_empty() || !id.chars().next().unwrap().is_ascii_alphabetic() {
            id = format!("op_{}", fallback);
        }
        id
    }
}

#[async_trait]
impl Skill for OpenApiSkill {
    fn id(&self) -> &str { &self.id }
    fn name(&self) -> &str { &self.name }
    fn description(&self) -> &str { &self.description }
    fn version(&self) -> &str { "1.0.0" }
    fn required_permissions(&self) -> Vec<Permission> {
        vec![Permission::Network { domains: vec!["*".to_string()] }]
    }
    fn input_schema(&self) -> Value { self.input_schema.clone() }

    async fn execute(&self, _ctx: &SkillContext, args: Value) -> Result<SkillResult, SkillError> {
        // In-app pseudo-plugins (search/connect) — fully local, no network.
        if self.server_base.starts_with("inapp://") {
            if self.server_base == "inapp://plugins/search" {
                let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
                return Ok(SkillResult::success(serde_json::json!({"query": query, "results": []})));
            }
            if self.server_base == "inapp://plugins/connect" {
                let app = args.get("appName").and_then(|v| v.as_str()).unwrap_or("app");
                return Ok(SkillResult::success(serde_json::json!({"app": app, "status": "connected"})));
            }
            return Ok(SkillResult::success(serde_json::json!({
                "tool": self.id, "args": args, "status:": "success"
            })));
        }

        // Real OpenAPI call: substitute path params, send query/body.
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .map_err(|e| SkillError::Execution(e.to_string()))?;

        // Path-param substitution: {id} → args.id
        let mut path = self.path_template.clone();
        if let Value::Object(map) = &args {
            for (k, v) in map {
                let placeholder = format!("{{{}}}", k);
                if path.contains(&placeholder) {
                    path = path.replace(&placeholder, &v.as_str().unwrap_or(&v.to_string()));
                }
            }
        }
        let url = format!("{}{}", self.server_base, path);

        let mut req = match self.method.to_uppercase().as_str() {
            "GET" => {
                let qs = args.as_object().map(|o| {
                    o.iter()
                        .filter(|(k, _)| !path.contains(&format!("{{{}}}", k)))
                        .map(|(k, v)| format!("{}={}", k, v.as_str().unwrap_or(&v.to_string())))
                        .collect::<Vec<_>>()
                        .join("&")
                }).unwrap_or_default();
                if qs.is_empty() { client.get(&url) } else { client.get(format!("{}?{}", url, qs)) }
            }
            "POST" => client.post(&url).json(&args),
            "PUT" => client.put(&url).json(&args),
            "PATCH" => client.patch(&url).json(&args),
            "DELETE" => client.delete(&url),
            _ => client.post(&url).json(&args),
        };
        if let Some(h) = &self.auth_header {
            req = req.header("Authorization", h.clone());
        }

        match req.send().await {
            Ok(r) => {
                let status = r.status();
                let body = r.text().await.unwrap_or_default();
                let json: Value = serde_json::from_str(&body).unwrap_or(Value::String(body.clone()));
                if status.is_success() {
                    Ok(SkillResult::success(json))
                } else {
                    Ok(SkillResult::failure(format!(
                        "API {} returned {}: {}",
                        self.id, status.as_u16(),
                        json.to_string().chars().take(500).collect::<String>()
                    )))
                }
            }
            Err(e) => Err(SkillError::Execution(format!(
                "{} failed: {} — check URL and network", self.id, e
            ))),
        }
    }
}

/// Parse raw OpenAPI spec text (JSON or YAML) into executable skills.
/// Returns at most 50 operations to keep the skill list manageable.
pub fn parse_openapi_spec(spec_text: &str) -> Result<Vec<OpenApiSkill>, String> {
    const MAX_OPS: usize = 50;

    // Try JSON first, then YAML.
    let root: Value = match serde_json::from_str::<Value>(spec_text) {
        Ok(v) => v,
        Err(_) => serde_yaml::from_str::<Value>(spec_text)
            .map_err(|e| format!("Spec is neither valid JSON nor YAML: {}", e))?,
    };

    // OpenAPI 3.x: servers[0].url; Swagger 2.0: host + basePath + schemes.
    let server_base = {
        let v2 = root.get("swagger").and_then(|v| v.as_str()) == Some("2.0");
        if v2 {
            let schemes = root.get("schemes").and_then(|s| s.as_array())
                .and_then(|a| a.first()).and_then(|v| v.as_str()).unwrap_or("https");
            let host = root.get("host").and_then(|v| v.as_str()).unwrap_or("localhost");
            let base = root.get("basePath").and_then(|v| v.as_str()).unwrap_or("");
            format!("{}://{}{}", schemes, host, base)
        } else {
            let from_servers = root.get("servers").and_then(|s| s.as_array())
                .and_then(|a| a.first())
                .and_then(|s| s.get("url").and_then(|u| u.as_str()).map(|s| s.to_string()));
            let from_host = root.get("host").and_then(|h| h.as_str().map(|h| format!("https://{}", h)));
            from_servers.or(from_host).unwrap_or("https://localhost".to_string())
        }
    };

    let paths = root.get("paths").and_then(|p| p.as_object())
        .ok_or_else(|| "No 'paths' object in spec".to_string())?;

    let mut skills = Vec::new();
    let mut op_index = 0;

    for (path_template, item) in paths {
        let item = match item.as_object() {
            Some(o) => o,
            None => continue,
        };
        for method in &["get", "post", "put", "patch", "delete"] {
            let op = match item.get(*method).and_then(|v| v.as_object()) {
                Some(o) => o,
                None => continue,
            };
            if op.get("x-internal").and_then(|v| v.as_bool()).unwrap_or(false) {
                continue;
            }
            op_index += 1;
            if skills.len() >= MAX_OPS {
                break;
            }
            let op_id = match op.get("operationId").and_then(|v| v.as_str()) {
                Some(id) => id.to_string(),
                None => OpenApiSkill::operation_id(method, path_template, &op_index.to_string()),
            };
            let summary = op.get("summary").and_then(|v| v.as_str()).unwrap_or("");
            let description = op.get("description").and_then(|v| v.as_str()).unwrap_or(summary);
            let mut schema = op.get("requestBody")
                .and_then(|rb| rb.get("content"))
                .and_then(|c| c.get("application/json"))
                .and_then(|j| j.get("schema"))
                .cloned()
                .unwrap_or_else(|| {
                    // Build schema from parameters (query/path/header).
                    let mut props = serde_json::Map::new();
                    if let Some(params) = op.get("parameters").and_then(|p| p.as_array()) {
                        for p in params {
                            if let (Some(name), Some(in_)) = (
                                p.get("name").and_then(|n| n.as_str()),
                                p.get("in").and_then(|i| i.as_str()),
                            ) {
                                if in_ == "query" || in_ == "path" {
                                    props.insert(name.to_string(), p.get("schema").cloned().unwrap_or_else(|| {
                                        let mut s = serde_json::json!({"type": "string"});
                                        if let Some(desc) = p.get("description").and_then(|d| d.as_str()) {
                                            s["description"] = serde_json::json!(desc);
                                        }
                                        s
                                    }));
                                }
                            }
                        }
                    }
                    if props.is_empty() {
                        serde_json::json!({"type": "object"})
                    } else {
                        serde_json::json!({"type": "object", "properties": props})
                    }
                });

            // Ensure schema is an object type.
            if schema.is_null() {
                schema = serde_json::json!({"type": "object"});
            }
            if schema.get("type").is_none() {
                schema["type"] = serde_json::json!("object");
            }

            skills.push(OpenApiSkill {
                id: op_id.to_string(),
                name: summary.to_string(),
                description: description.to_string(),
                method: method.to_string(),
                path_template: path_template.to_string(),
                server_base: server_base.clone(),
                input_schema: schema,
                auth_header: None,
            });
        }
        if skills.len() >= MAX_OPS {
            break;
        }
    }

    if skills.is_empty() {
        return Err("No operations found in spec".to_string());
    }
    Ok(skills)
}

/// Persist parsed operations so they survive restarts and can be reloaded.
pub async fn store_operations(
    pool: &SqlitePool,
    plugin_id: &str,
    ops: &[OpenApiSkill],
) -> Result<(), String> {
    // Clear old ops for this plugin, then insert fresh.
    sqlx::query("DELETE FROM openapi_operations WHERE plugin_id = ?")
        .bind(plugin_id).execute(pool).await.map_err(|e| e.to_string())?;
    let now = chrono::Utc::now().to_rfc3339();
    for op in ops {
        sqlx::query(
            "INSERT INTO openapi_operations (id, plugin_id, operation_id, method, path_template, description, input_schema, server_base, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&op.id)
        .bind(plugin_id)
        .bind(&op.id)
        .bind(&op.method)
        .bind(&op.path_template)
        .bind(&op.description)
        .bind(serde_json::to_string(&op.input_schema).unwrap_or_default())
        .bind(&op.server_base)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Load previously parsed operations for a plugin.
pub async fn load_operations(pool: &SqlitePool, plugin_id: &str) -> Result<Vec<OpenApiSkill>, String> {
    type Row = (String, String, String, String, String, String);
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT operation_id, method, path_template, description, input_schema, COALESCE(server_base, '')
         FROM openapi_operations WHERE plugin_id = ?",
    )
    .bind(plugin_id).fetch_all(pool).await.map_err(|e| e.to_string())?;

    // Legacy rows (stored before P8) have no server_base — re-derive it from
    // the plugin's stored spec so imported tools work again after restart.
    let legacy_base: Option<String> = if rows.iter().any(|r| r.5.is_empty()) {
        let spec: Option<String> = sqlx::query_scalar("SELECT openapi_spec FROM plugins WHERE id = ?")
            .bind(plugin_id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();
        spec.and_then(|s| parse_openapi_spec(&s).ok().and_then(|ops| ops.first().map(|o| o.server_base.clone())))
    } else {
        None
    };

    Ok(rows.into_iter().map(|(op_id, method, path, desc, schema, server_base)| {
        OpenApiSkill {
            id: op_id.clone(),
            name: op_id,
            description: desc,
            method,
            path_template: path,
            server_base: if server_base.is_empty() {
                legacy_base.clone().unwrap_or_default()
            } else {
                server_base
            },
            input_schema: serde_json::from_str(&schema).unwrap_or(serde_json::json!({"type":"object"})),
            auth_header: None,
        }
    }).collect())
}

#[cfg(test)]
mod server_base_tests {
    use super::*;

    const SPEC: &str = r#"{"openapi":"3.0.0","servers":[{"url":"https://api.petstore.test/v3"}],"paths":{"/pets":{"get":{"operationId":"listPets","summary":"List pets"}},"/pets/{id}":{"delete":{"operationId":"deletePet"}}}}"#;

    async fn temp_pool_with_plugin(spec: &str) -> sqlx::SqlitePool {
        let path = std::path::PathBuf::from(std::env::temp_dir())
            .join(format!("ravenbot-openapi-test-{}.db", uuid::Uuid::new_v4()));
        let db = ravenbot_db::Database::new(&path).await.expect("temp db");
        let pool = db.pool().clone();
        sqlx::query(
            "INSERT INTO plugins (id, name, description, logo, openapi_spec, enabled, created_at)
             VALUES ('petstore', 'Petstore', 'demo', '', ?, 1, datetime('now'))",
        )
        .bind(spec)
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    #[tokio::test]
    async fn server_base_survives_store_and_reload() {
        let pool = temp_pool_with_plugin("{}").await;
        let ops = parse_openapi_spec(SPEC).unwrap();
        store_operations(&pool, "petstore", &ops).await.unwrap();
        let loaded = load_operations(&pool, "petstore").await.unwrap();
        assert_eq!(loaded.len(), 2);
        assert!(
            loaded.iter().all(|o| o.server_base == "https://api.petstore.test/v3"),
            "got {:?}",
            loaded.iter().map(|o| o.server_base.clone()).collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn legacy_rows_rederive_server_base_from_spec() {
        let pool = temp_pool_with_plugin(SPEC).await;
        let ops = parse_openapi_spec(SPEC).unwrap();
        store_operations(&pool, "petstore", &ops).await.unwrap();
        // Simulate a pre-P8 row: server_base lost.
        sqlx::query("UPDATE openapi_operations SET server_base = '' WHERE plugin_id = 'petstore'")
            .execute(&pool)
            .await
            .unwrap();
        let loaded = load_operations(&pool, "petstore").await.unwrap();
        assert_eq!(loaded.len(), 2);
        assert!(
            loaded.iter().all(|o| o.server_base == "https://api.petstore.test/v3"),
            "legacy re-derive failed: {:?}",
            loaded.iter().map(|o| o.server_base.clone()).collect::<Vec<_>>()
        );
    }
}

