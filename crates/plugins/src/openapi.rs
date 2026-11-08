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
    /// How the request body is encoded: `json` or `form`.
    pub body_format: String,
    /// Fields that belong in the URL even on POST/PUT/PATCH.
    pub query_parameters: Vec<String>,
    pub auth_header: Option<String>,
    /// Header the credential belongs in (`Authorization`, `X-API-Key`, …).
    pub auth_header_name: String,
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

/// Attach a JSON or urlencoded body. Form values are flattened to strings;
/// legacy form APIs rarely accept nested JSON, and a nested field serialized
/// as JSON text is more useful than a request that fails to serialize.
fn query_value(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn send_body(
    request: reqwest::RequestBuilder,
    body: &Value,
    body_format: &str,
) -> reqwest::RequestBuilder {
    if body_format == "form" {
        let fields: std::collections::BTreeMap<String, String> = body
            .as_object()
            .map(|object| {
                object
                    .iter()
                    .map(|(key, value)| {
                        let value = match value {
                            Value::String(text) => text.clone(),
                            Value::Null => String::new(),
                            other => other.to_string(),
                        };
                        (key.clone(), value)
                    })
                    .collect()
            })
            .unwrap_or_default();
        request.form(&fields)
    } else {
        request.json(body)
    }
}

#[async_trait]
impl Skill for OpenApiSkill {
    fn id(&self) -> &str {
        &self.id
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn description(&self) -> &str {
        &self.description
    }
    fn version(&self) -> &str {
        "1.0.0"
    }
    fn required_permissions(&self) -> Vec<Permission> {
        vec![Permission::Network {
            domains: vec!["*".to_string()],
        }]
    }
    fn input_schema(&self) -> Value {
        self.input_schema.clone()
    }

    async fn execute(&self, _ctx: &SkillContext, args: Value) -> Result<SkillResult, SkillError> {
        if self.server_base.trim().is_empty() {
            return Ok(SkillResult::failure(format!(
                "{} has no server URL. Re-import its OpenAPI spec with a valid servers[0].url.",
                self.id
            )));
        }

        // Real OpenAPI call: substitute path params, send query/body.
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .user_agent("RAVENBOT/1.0")
            .build()
            .map_err(|e| SkillError::Execution(e.to_string()))?;

        // Path-param substitution: {id} → args.id. Remember which keys were
        // consumed so they are not repeated in the query string or JSON body.
        let mut path = self.path_template.clone();
        let mut path_args = std::collections::HashSet::new();
        if let Value::Object(map) = &args {
            for (k, v) in map {
                let placeholder = format!("{{{}}}", k);
                if path.contains(&placeholder) {
                    path = path.replace(&placeholder, v.as_str().unwrap_or(&v.to_string()));
                    path_args.insert(k.clone());
                }
            }
        }
        let url = format!("{}{}", self.server_base, path);
        let is_get = self.method.eq_ignore_ascii_case("GET");
        let query_names: std::collections::HashSet<&str> =
            self.query_parameters.iter().map(String::as_str).collect();
        let mut query_pairs: Vec<(String, String)> = Vec::new();
        let mut body_fields = serde_json::Map::new();
        if let Value::Object(map) = &args {
            for (key, value) in map {
                if path_args.contains(key) {
                    continue;
                }
                if is_get || query_names.contains(key.as_str()) {
                    query_pairs.push((key.clone(), query_value(value)));
                } else {
                    body_fields.insert(key.clone(), value.clone());
                }
            }
        }
        let body = Value::Object(body_fields);
        let url = match reqwest::Url::parse(&url) {
            Ok(mut endpoint) => {
                for (key, value) in &query_pairs {
                    endpoint.query_pairs_mut().append_pair(key, value);
                }
                endpoint.to_string()
            }
            Err(_) => url,
        };

        let mut req = match self.method.to_uppercase().as_str() {
            "GET" => client.get(&url),
            "POST" => send_body(client.post(&url), &body, &self.body_format),
            "PUT" => send_body(client.put(&url), &body, &self.body_format),
            "PATCH" => send_body(client.patch(&url), &body, &self.body_format),
            "DELETE" => {
                let request = client.delete(&url);
                if body
                    .as_object()
                    .map(|object| !object.is_empty())
                    .unwrap_or(false)
                {
                    send_body(request, &body, &self.body_format)
                } else {
                    request
                }
            }
            _ => send_body(client.post(&url), &body, &self.body_format),
        };
        if let Some(value) = &self.auth_header {
            let name = reqwest::header::HeaderName::from_bytes(self.auth_header_name.as_bytes())
                .map_err(|_| {
                    SkillError::Execution(format!(
                        "{} has an invalid auth header name: {}",
                        self.id, self.auth_header_name
                    ))
                })?;
            let value = reqwest::header::HeaderValue::from_str(value).map_err(|_| {
                SkillError::Execution(format!("{} has an invalid credential value.", self.id))
            })?;
            req = req.header(name, value);
        }

        match req.send().await {
            Ok(r) => {
                let status = r.status();
                let body = r.text().await.unwrap_or_default();
                let json: Value =
                    serde_json::from_str(&body).unwrap_or(Value::String(body.clone()));
                if status.is_success() {
                    Ok(SkillResult::success(json))
                } else {
                    Ok(SkillResult::failure(format!(
                        "API {} returned {}: {}",
                        self.id,
                        status.as_u16(),
                        json.to_string().chars().take(500).collect::<String>()
                    )))
                }
            }
            Err(e) => Err(SkillError::Execution(format!(
                "{} failed: {} — check URL and network",
                self.id, e
            ))),
        }
    }
}

/// Resolve local `$ref` pointers so the model sees the actual body schema.
///
/// Real specs (Stripe, GitHub, Jira) define request bodies almost entirely as
/// `#/components/schemas/...` references. Passing the raw `$ref` to a model
/// tells it nothing about the fields; resolving the local pointer is bounded
/// and enough for tool use.
fn resolve_schema(root: &Value, schema: &Value, depth: usize) -> Value {
    if depth > 16 {
        return schema.clone();
    }
    match schema {
        Value::Object(map) => {
            if let Some(Value::String(reference)) = map.get("$ref") {
                if let Some(target) = resolve_pointer(root, reference) {
                    return resolve_schema(root, &target, depth + 1);
                }
            }
            Value::Object(
                map.iter()
                    .map(|(key, value)| (key.clone(), resolve_schema(root, value, depth + 1)))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|value| resolve_schema(root, value, depth + 1))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn resolve_pointer(root: &Value, reference: &str) -> Option<Value> {
    let path = reference.strip_prefix("#/")?;
    let mut current = root;
    for segment in path.split('/') {
        let key = segment.replace("~1", "/").replace("~0", "~");
        current = current.get(&key)?;
    }
    Some(current.clone())
}

/// Schema for a Swagger 2 parameter. Swagger 2 puts `type`, `format`, and
/// `items` directly on the parameter instead of under a `schema` object.
fn v2_parameter_schema(parameter: &Value) -> Value {
    if let Some(schema) = parameter.get("schema") {
        return schema.clone();
    }
    let mut schema = serde_json::Map::new();
    if let Some(value_type) = parameter.get("type").and_then(|value| value.as_str()) {
        schema.insert("type".to_string(), serde_json::json!(value_type));
    }
    for key in [
        "format",
        "description",
        "enum",
        "default",
        "minimum",
        "maximum",
        "minLength",
        "maxLength",
        "pattern",
        "items",
    ] {
        if let Some(value) = parameter.get(key) {
            schema.insert(key.to_string(), value.clone());
        }
    }
    if schema.is_empty() {
        serde_json::json!({"type": "string"})
    } else {
        Value::Object(schema)
    }
}

/// Operation and path-level parameters, with the operation-level definition
/// winning when both declare the same `(name, in)` pair.
fn operation_parameters(
    root: &Value,
    item: &serde_json::Map<String, Value>,
    operation: &serde_json::Map<String, Value>,
) -> Vec<Value> {
    let mut seen = std::collections::HashSet::new();
    let mut parameters = Vec::new();
    for list in [operation.get("parameters"), item.get("parameters")] {
        let Some(list) = list.and_then(|value| value.as_array()) else {
            continue;
        };
        for parameter in list {
            // Parameters themselves can be `$ref`s in OpenAPI 3.
            let parameter = resolve_schema(root, parameter, 0);
            let Some(name) = parameter.get("name").and_then(|value| value.as_str()) else {
                continue;
            };
            let Some(location) = parameter.get("in").and_then(|value| value.as_str()) else {
                continue;
            };
            if seen.insert(format!("{location}:{name}")) {
                parameters.push(parameter);
            }
        }
    }
    parameters
}

/// Request body schema for either document format.
///
/// OpenAPI 3 puts the body under `requestBody.content`; Swagger 2 puts it in a
/// parameter with `in: body`, or describes form fields with `in: formData`.
/// Choosing the JSON media type first matters because tool arguments are JSON.
fn operation_body_schema(
    root: &Value,
    operation: &serde_json::Map<String, Value>,
    parameters: &[Value],
    is_swagger2: bool,
) -> Option<(Value, &'static str)> {
    if !is_swagger2 {
        let content = operation
            .get("requestBody")
            .and_then(|body| body.get("content"))
            .and_then(|content| content.as_object());
        if let Some(content) = content {
            for (media_type, format) in [
                ("application/json", "json"),
                ("application/x-www-form-urlencoded", "form"),
                // Multipart file uploads need a dedicated tool; for plain
                // fields urlencoded is the closest representation we can send.
                ("multipart/form-data", "form"),
            ] {
                if let Some(schema) = content
                    .get(media_type)
                    .and_then(|media| media.get("schema"))
                {
                    return Some((resolve_schema(root, schema, 0), format));
                }
            }
            return content
                .values()
                .find_map(|media| media.get("schema"))
                .map(|schema| (resolve_schema(root, schema, 0), "json"));
        }
        return None;
    }

    if let Some(body) = parameters
        .iter()
        .find(|parameter| parameter.get("in").and_then(|value| value.as_str()) == Some("body"))
    {
        return body
            .get("schema")
            .map(|schema| (resolve_schema(root, schema, 0), "json"));
    }

    let form: Vec<&Value> = parameters
        .iter()
        .filter(|parameter| {
            parameter.get("in").and_then(|value| value.as_str()) == Some("formData")
        })
        .collect();
    if form.is_empty() {
        return None;
    }
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();
    for parameter in form {
        let Some(name) = parameter.get("name").and_then(|value| value.as_str()) else {
            continue;
        };
        properties.insert(name.to_string(), v2_parameter_schema(parameter));
        if parameter
            .get("required")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
        {
            required.push(serde_json::json!(name));
        }
    }
    let mut schema = serde_json::json!({
        "type": "object",
        "properties": Value::Object(properties),
    });
    if !required.is_empty() {
        schema["required"] = Value::Array(required);
    }
    Some((schema, "form"))
}

/// Add query and path parameters to a body schema so one tool call can carry
/// both. A non-object body is left alone: splitting it would invent a shape the
/// API does not accept.
fn merge_parameter_schema(mut schema: Value, parameters: &[Value], root: &Value) -> Value {
    let mut extras = serde_json::Map::new();
    let mut required_extras = Vec::new();
    for parameter in parameters {
        let Some(name) = parameter.get("name").and_then(|value| value.as_str()) else {
            continue;
        };
        let location = parameter
            .get("in")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        if location != "query" && location != "path" {
            continue;
        }
        let already_present = schema
            .get("properties")
            .and_then(|value| value.as_object())
            .map(|properties| properties.contains_key(name))
            .unwrap_or(false);
        if already_present {
            continue;
        }
        let property = parameter
            .get("schema")
            .cloned()
            .unwrap_or_else(|| v2_parameter_schema(parameter));
        extras.insert(name.to_string(), resolve_schema(root, &property, 0));
        if parameter
            .get("required")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
        {
            required_extras.push(name.to_string());
        }
    }
    if extras.is_empty() {
        return schema;
    }

    let is_object = schema
        .get("type")
        .and_then(|value| value.as_str())
        .map(|value_type| value_type == "object")
        .unwrap_or_else(|| schema.get("properties").is_some());
    if !is_object {
        return schema;
    }
    if schema.get("type").is_none() {
        schema["type"] = serde_json::json!("object");
    }
    if let Some(object) = schema.as_object_mut() {
        if !object.contains_key("properties") {
            object.insert("properties".to_string(), serde_json::json!({}));
        }
        if let Some(properties) = object
            .get_mut("properties")
            .and_then(|value| value.as_object_mut())
        {
            for (name, property) in extras {
                properties.insert(name, property);
            }
        }
    }
    if !required_extras.is_empty() {
        if let Some(object) = schema.as_object_mut() {
            if !object.contains_key("required") {
                object.insert("required".to_string(), serde_json::json!([]));
            }
            if let Some(required) = object
                .get_mut("required")
                .and_then(|value| value.as_array_mut())
            {
                for name in required_extras {
                    if !required
                        .iter()
                        .any(|value| value.as_str() == Some(name.as_str()))
                    {
                        required.push(serde_json::json!(name));
                    }
                }
            }
        }
    }
    schema
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

    let is_swagger2 = root.get("swagger").and_then(|v| v.as_str()) == Some("2.0");

    // OpenAPI 3.x: servers[0].url; Swagger 2.0: host + basePath + schemes.
    let server_base = {
        if is_swagger2 {
            let schemes = root
                .get("schemes")
                .and_then(|s| s.as_array())
                .and_then(|a| a.first())
                .and_then(|v| v.as_str())
                .unwrap_or("https");
            let host = root
                .get("host")
                .and_then(|v| v.as_str())
                .unwrap_or("localhost");
            let base = root.get("basePath").and_then(|v| v.as_str()).unwrap_or("");
            format!("{}://{}{}", schemes, host, base)
        } else {
            let from_servers = root
                .get("servers")
                .and_then(|s| s.as_array())
                .and_then(|a| a.first())
                .and_then(|s| s.get("url").and_then(|u| u.as_str()).map(|s| s.to_string()));
            let from_host = root
                .get("host")
                .and_then(|h| h.as_str().map(|h| format!("https://{}", h)));
            from_servers
                .or(from_host)
                .unwrap_or("https://localhost".to_string())
        }
    };

    let paths = root
        .get("paths")
        .and_then(|p| p.as_object())
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
            if op
                .get("x-internal")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
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
            let description = op
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or(summary);
            let parameters = operation_parameters(&root, item, op);
            let query_parameters: Vec<String> = parameters
                .iter()
                .filter(|parameter| {
                    parameter.get("in").and_then(|value| value.as_str()) == Some("query")
                })
                .filter_map(|parameter| parameter.get("name").and_then(|value| value.as_str()))
                .map(str::to_string)
                .collect();
            let (body_schema, body_format) =
                operation_body_schema(&root, op, &parameters, is_swagger2)
                    .map(|(schema, format)| (Some(schema), format))
                    .unwrap_or((None, "json"));
            let mut schema = body_schema.unwrap_or_else(|| serde_json::json!({"type": "object"}));
            schema = merge_parameter_schema(schema, &parameters, &root);
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
                body_format: body_format.to_string(),
                query_parameters,
                auth_header: None,
                auth_header_name: "Authorization".to_string(),
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
    // Clear and replace atomically. A partially-written operation set is worse
    // than no set: the app card would claim operations that do not exist.
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM openapi_operations WHERE plugin_id = ?")
        .bind(plugin_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    let now = chrono::Utc::now().to_rfc3339();
    for op in ops {
        // Operation ids are only unique inside one document. Prefix with the
        // plugin id so two apps with a `list_items` operation can coexist and
        // `plugin_execute` has one unambiguous action to dispatch.
        let qualified = qualify_operation_id(plugin_id, &op.id);
        sqlx::query(
            "INSERT INTO openapi_operations (id, plugin_id, operation_id, method, path_template, description, input_schema, server_base, body_format, query_params, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&qualified)
        .bind(plugin_id)
        .bind(&qualified)
        .bind(&op.method)
        .bind(&op.path_template)
        .bind(&op.description)
        .bind(serde_json::to_string(&op.input_schema).unwrap_or_default())
        .bind(&op.server_base)
        .bind(&op.body_format)
        .bind(serde_json::to_string(&op.query_parameters).unwrap_or_else(|_| "[]".to_string()))
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Load previously parsed operations for a plugin.
pub async fn load_operations(
    pool: &SqlitePool,
    plugin_id: &str,
) -> Result<Vec<OpenApiSkill>, String> {
    type Row = (
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    );
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT operation_id, method, path_template, description, input_schema, COALESCE(server_base, ''), COALESCE(body_format, 'json'), COALESCE(query_params, '[]')
         FROM openapi_operations WHERE plugin_id = ?",
    )
    .bind(plugin_id).fetch_all(pool).await.map_err(|e| e.to_string())?;

    // Legacy rows (stored before P8) have no server_base — re-derive it from
    // the plugin's stored spec so imported tools work again after restart.
    let legacy_base: Option<String> = if rows.iter().any(|r| r.5.is_empty()) {
        let spec: Option<String> =
            sqlx::query_scalar("SELECT openapi_spec FROM plugins WHERE id = ?")
                .bind(plugin_id)
                .fetch_optional(pool)
                .await
                .ok()
                .flatten();
        spec.and_then(|s| {
            parse_openapi_spec(&s)
                .ok()
                .and_then(|ops| ops.first().map(|o| o.server_base.clone()))
        })
    } else {
        None
    };
    let base_override: Option<String> =
        sqlx::query_scalar("SELECT base_url_override FROM plugins WHERE id = ?")
            .bind(plugin_id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .filter(|value: &String| !value.trim().is_empty());

    Ok(rows
        .into_iter()
        .map(
            |(op_id, method, path, desc, schema, server_base, body_format, query_params)| {
                let id = normalize_operation_id(plugin_id, &op_id);
                let base = base_override
                    .clone()
                    .or_else(|| {
                        if server_base.is_empty() {
                            legacy_base.clone()
                        } else {
                            Some(server_base)
                        }
                    })
                    .unwrap_or_default();
                OpenApiSkill {
                    id: id.clone(),
                    name: id,
                    description: desc,
                    method,
                    path_template: path,
                    server_base: base,
                    input_schema: serde_json::from_str(&schema)
                        .unwrap_or(serde_json::json!({"type":"object"})),
                    body_format,
                    query_parameters: serde_json::from_str(&query_params).unwrap_or_default(),
                    auth_header: None,
                    auth_header_name: "Authorization".to_string(),
                }
            },
        )
        .collect())
}

/// Display metadata derived from an OpenAPI or Swagger document.
#[derive(Debug, Clone)]
pub struct SpecMetadata {
    pub title: String,
    pub description: String,
    pub logo: Option<String>,
    pub requires_auth: bool,
    /// Header the spec's security scheme expects (`Authorization`,
    /// `X-API-Key`, …). `None` when the document does not say.
    pub credential_header: Option<String>,
}

fn parse_document(spec_text: &str) -> Result<Value, String> {
    match serde_json::from_str::<Value>(spec_text) {
        Ok(v) => Ok(v),
        Err(_) => serde_yaml::from_str::<Value>(spec_text)
            .map_err(|e| format!("Spec is neither valid JSON nor YAML: {}", e)),
    }
}

/// Does this document declare an authentication scheme?
pub fn spec_requires_auth(root: &Value) -> bool {
    let non_empty_array = |v: Option<&Value>| {
        v.and_then(|v| v.as_array())
            .map(|a| !a.is_empty())
            .unwrap_or(false)
    };
    let non_empty_object = |v: Option<&Value>| {
        v.and_then(|v| v.as_object())
            .map(|o| !o.is_empty())
            .unwrap_or(false)
    };
    non_empty_array(root.get("security"))
        || non_empty_object(root.pointer("/components/securitySchemes"))
        || non_empty_object(root.get("securityDefinitions"))
}

/// Pick the header a single security scheme expects.
fn header_for_scheme(scheme: &Value) -> Option<String> {
    let scheme_type = scheme
        .get("type")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match scheme_type.as_str() {
        "apikey" => {
            let location = scheme
                .get("in")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            if !location.eq_ignore_ascii_case("header") {
                return None;
            }
            scheme
                .get("name")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_string)
        }
        "http" => {
            let http_scheme = scheme
                .get("scheme")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            matches!(http_scheme.as_str(), "bearer" | "basic" | "digest")
                .then(|| "Authorization".to_string())
        }
        // Swagger 2.0 uses `basic`/`oauth2` as top-level types. OpenID
        // Connect and OAuth 2 in OpenAPI 3 both use a bearer token.
        "basic" | "oauth2" | "openidconnect" => Some("Authorization".to_string()),
        _ => None,
    }
}

/// Recommended credential header from the document's security schemes.
///
/// Schemes named in the global `security` requirement are preferred; a
/// document can define several (for example a header key and OAuth) and the
/// first one the API actually requires is the useful default.
pub fn spec_credential_header(root: &Value) -> Option<String> {
    let schemes = root
        .pointer("/components/securitySchemes")
        .or_else(|| root.get("securityDefinitions"))
        .and_then(|value| value.as_object())?;

    let preferred: std::collections::HashSet<&str> = root
        .get("security")
        .and_then(|value| value.as_array())
        .map(|requirements| {
            requirements
                .iter()
                .filter_map(|requirement| requirement.as_object())
                .flat_map(|requirement| requirement.keys().map(String::as_str))
                .collect()
        })
        .unwrap_or_default();

    let mut ordered: Vec<(&String, &Value)> = schemes.iter().collect();
    ordered.sort_by_key(|(name, _)| !preferred.contains(name.as_str()));
    ordered
        .into_iter()
        .find_map(|(_, scheme)| header_for_scheme(scheme))
}

pub fn spec_metadata(spec_text: &str) -> Result<SpecMetadata, String> {
    let root = parse_document(spec_text)?;
    let info = root.get("info");
    let title = info
        .and_then(|i| i.get("title"))
        .and_then(|v| v.as_str())
        .or_else(|| root.get("name_for_human").and_then(|v| v.as_str()))
        .unwrap_or("OpenAPI App")
        .to_string();
    let description = info
        .and_then(|i| i.get("description"))
        .and_then(|v| v.as_str())
        .or_else(|| root.get("description_for_human").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();
    let logo = info
        .and_then(|i| i.get("x-logo"))
        .and_then(|l| l.get("url").and_then(|v| v.as_str()).or_else(|| l.as_str()))
        .or_else(|| root.get("logo_url").and_then(|v| v.as_str()))
        .map(str::to_string)
        .filter(|l| l.starts_with("http://") || l.starts_with("https://"));
    Ok(SpecMetadata {
        title,
        description,
        logo,
        requires_auth: spec_requires_auth(&root),
        credential_header: spec_credential_header(&root),
    })
}

/// Provider-safe callable name for an operation.
///
/// OpenAI-shaped APIs only accept `[a-zA-Z0-9_-]` in a tool name. A raw
/// `plugin::operation` id would reject the whole tools array, so the
/// qualifier is sanitized and capped at the 64-character provider limit.
fn tool_fragment(value: &str) -> String {
    let mut out = String::new();
    let mut underscore = false;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            underscore = false;
        } else if !underscore && !out.is_empty() {
            out.push('_');
            underscore = true;
        }
    }
    out.trim_matches('_').to_string()
}

pub fn qualify_operation_id(plugin_id: &str, operation_id: &str) -> String {
    let full = format!(
        "{}__{}",
        tool_fragment(plugin_id),
        tool_fragment(operation_id)
    );
    if full.chars().count() <= 64 {
        return full;
    }
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    full.hash(&mut hasher);
    let suffix = format!("{:08x}", hasher.finish());
    let prefix: String = full.chars().take(55).collect();
    format!("{prefix}_{suffix}")
}

/// Bring a legacy or raw operation id into the provider-safe qualified form.
pub fn normalize_operation_id(plugin_id: &str, operation_id: &str) -> String {
    let prefix = format!("{}__", tool_fragment(plugin_id));
    if operation_id.starts_with(&prefix) {
        operation_id.to_string()
    } else {
        let raw = operation_id.split("::").last().unwrap_or(operation_id);
        qualify_operation_id(plugin_id, raw)
    }
}

/// Stable, human-readable plugin id from an app title.
pub fn slugify_title(title: &str) -> String {
    let mut slug = String::new();
    let mut dash = false;
    for ch in title.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            if dash && !slug.is_empty() {
                slug.push('-');
            }
            dash = false;
            slug.push(ch);
        } else {
            dash = true;
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        format!(
            "openapi-{}",
            &uuid::Uuid::new_v4().simple().to_string()[..8]
        )
    } else {
        slug.chars().take(48).collect()
    }
}

#[cfg(test)]
mod server_base_tests {
    use super::*;

    const SPEC: &str = r#"{"openapi":"3.0.0","servers":[{"url":"https://api.petstore.test/v3"}],"paths":{"/pets":{"get":{"operationId":"listPets","summary":"List pets"}},"/pets/{id}":{"delete":{"operationId":"deletePet"}}}}"#;

    #[test]
    fn openapi3_body_and_query_parameters_share_one_schema() {
        let spec = serde_json::json!({
            "openapi": "3.0.0",
            "info": {"title": "Billing"},
            "servers": [{"url": "https://billing.test"}],
            "paths": {
                "/invoices/{invoice_id}": {
                    "post": {
                        "operationId": "createInvoice",
                        "parameters": [
                            {"name": "invoice_id", "in": "path", "required": true, "schema": {"type": "string"}},
                            {"name": "dry_run", "in": "query", "schema": {"type": "boolean"}}
                        ],
                        "requestBody": {
                            "content": {
                                "application/json": {
                                    "schema": {"$ref": "#/components/schemas/Invoice"}
                                }
                            }
                        }
                    }
                }
            },
            "components": {
                "schemas": {
                    "Invoice": {
                        "type": "object",
                        "required": ["amount"],
                        "properties": {
                            "amount": {"type": "integer"},
                            "customer": {"type": "string"}
                        }
                    }
                }
            }
        });
        let operations = parse_openapi_spec(&spec.to_string()).expect("operations");
        let schema = &operations[0].input_schema;
        assert_eq!(schema["properties"]["amount"]["type"], "integer");
        assert_eq!(schema["properties"]["customer"]["type"], "string");
        assert_eq!(schema["properties"]["invoice_id"]["type"], "string");
        assert_eq!(schema["properties"]["dry_run"]["type"], "boolean");
        let required = schema["required"].as_array().expect("required");
        assert!(required.iter().any(|value| value == "amount"));
        assert!(required.iter().any(|value| value == "invoice_id"));
    }

    #[test]
    fn swagger2_body_and_form_parameters_become_fields() {
        let spec = serde_json::json!({
            "swagger": "2.0",
            "info": {"title": "Legacy"},
            "host": "api.legacy.test",
            "basePath": "/v1",
            "schemes": ["https"],
            "paths": {
                "/invoices": {
                    "post": {
                        "operationId": "createInvoice",
                        "parameters": [
                            {"name": "body", "in": "body", "required": true, "schema": {"$ref": "#/definitions/Invoice"}},
                            {"name": "notify", "in": "query", "type": "boolean"}
                        ]
                    }
                },
                "/uploads": {
                    "post": {
                        "operationId": "upload",
                        "parameters": [
                            {"name": "name", "in": "formData", "required": true, "type": "string"},
                            {"name": "size", "in": "formData", "type": "integer"}
                        ]
                    }
                }
            },
            "definitions": {
                "Invoice": {
                    "type": "object",
                    "properties": {"amount": {"type": "integer"}}
                }
            }
        });
        let operations = parse_openapi_spec(&spec.to_string()).expect("operations");
        assert_eq!(operations[0].server_base, "https://api.legacy.test/v1");

        let create = operations
            .iter()
            .find(|operation| operation.id == "createInvoice")
            .expect("create");
        assert_eq!(
            create.input_schema["properties"]["amount"]["type"],
            "integer"
        );
        assert_eq!(
            create.input_schema["properties"]["notify"]["type"],
            "boolean"
        );
        assert!(create.input_schema["properties"].get("body").is_none());

        assert_eq!(create.body_format, "json");
        assert_eq!(create.query_parameters, vec!["notify".to_string()]);

        let upload = operations
            .iter()
            .find(|operation| operation.id == "upload")
            .expect("upload");
        assert_eq!(upload.input_schema["properties"]["name"]["type"], "string");
        assert_eq!(upload.input_schema["properties"]["size"]["type"], "integer");
        assert_eq!(upload.input_schema["required"][0], "name");
        assert_eq!(upload.body_format, "form");
    }

    #[test]
    fn metadata_reads_the_security_scheme_header() {
        let api_key = r#"{"openapi":"3.0.0","info":{"title":"Ledger"},"security":[{"KeyAuth":[]}],"components":{"securitySchemes":{"KeyAuth":{"type":"apiKey","in":"header","name":"X-API-Key"}}}}"#;
        let metadata = spec_metadata(api_key).expect("metadata");
        assert!(metadata.requires_auth);
        assert_eq!(metadata.credential_header.as_deref(), Some("X-API-Key"));

        let bearer = r#"{"openapi":"3.0.0","info":{"title":"Ledger"},"components":{"securitySchemes":{"BearerAuth":{"type":"http","scheme":"bearer"}}}}"#;
        assert_eq!(
            spec_metadata(bearer)
                .expect("bearer metadata")
                .credential_header
                .as_deref(),
            Some("Authorization")
        );

        // A query-string API key cannot be expressed as a header; the UI
        // should not pretend that it can.
        let query_key = r#"{"openapi":"3.0.0","info":{"title":"Ledger"},"components":{"securitySchemes":{"QueryAuth":{"type":"apiKey","in":"query","name":"api_key"}}}}"#;
        let metadata = spec_metadata(query_key).expect("query metadata");
        assert!(metadata.requires_auth);
        assert_eq!(metadata.credential_header, None);
    }

    async fn temp_pool_with_plugin(spec: &str) -> sqlx::SqlitePool {
        let path =
            std::env::temp_dir().join(format!("ravenbot-openapi-test-{}.db", uuid::Uuid::new_v4()));
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

    #[test]
    fn local_refs_in_request_bodies_are_resolved() {
        let spec = r##"{"openapi":"3.0.0","info":{"title":"Billing"},"servers":[{"url":"https://billing.test"}],"paths":{"/invoices":{"post":{"operationId":"createInvoice","requestBody":{"content":{"application/json":{"schema":{"$ref":"#/components/schemas/Invoice"}}}}}}},"components":{"schemas":{"Invoice":{"type":"object","required":["amount"],"properties":{"amount":{"type":"integer"},"customer":{"type":"string"}}}}}}"##;
        let ops = parse_openapi_spec(spec).unwrap();
        assert_eq!(ops.len(), 1);
        let schema = &ops[0].input_schema;
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["properties"]["amount"]["type"], "integer");
        assert_eq!(schema["required"][0], "amount");
    }

    #[tokio::test]
    async fn server_base_survives_store_and_reload() {
        let pool = temp_pool_with_plugin("{}").await;
        let ops = parse_openapi_spec(SPEC).unwrap();
        store_operations(&pool, "petstore", &ops).await.unwrap();
        let loaded = load_operations(&pool, "petstore").await.unwrap();
        assert_eq!(loaded.len(), 2);
        assert!(
            loaded
                .iter()
                .all(|o| o.server_base == "https://api.petstore.test/v3"),
            "got {:?}",
            loaded
                .iter()
                .map(|o| o.server_base.clone())
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn body_format_survives_store_and_reload() {
        let pool = temp_pool_with_plugin("{}").await;
        let spec = serde_json::json!({
            "swagger": "2.0",
            "info": {"title": "Legacy"},
            "host": "api.legacy.test",
            "paths": {
                "/upload": {
                    "post": {
                        "operationId": "upload",
                        "parameters": [
                            {"name": "name", "in": "formData", "type": "string"},
                            {"name": "notify", "in": "query", "type": "boolean"}
                        ]
                    }
                }
            }
        });
        let ops = parse_openapi_spec(&spec.to_string()).expect("operations");
        store_operations(&pool, "petstore", &ops).await.unwrap();
        let loaded = load_operations(&pool, "petstore").await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].body_format, "form");
        assert_eq!(loaded[0].query_parameters, vec!["notify".to_string()]);
        assert_eq!(
            loaded[0].input_schema["properties"]["name"]["type"],
            "string"
        );
    }

    #[tokio::test]
    async fn execute_sends_path_params_query_and_auth_header() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("address");
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut buffer = [0u8; 4096];
            let read = socket.read(&mut buffer).await.expect("read");
            let request = String::from_utf8_lossy(&buffer[..read]).to_string();
            let _ = socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\nconnection: close\r\n\r\n{\"ok\":true}",
                )
                .await;
            request
        });

        let skill = OpenApiSkill {
            id: "petstore::getPet".to_string(),
            name: "Get pet".to_string(),
            description: String::new(),
            method: "GET".to_string(),
            path_template: "/pets/{id}".to_string(),
            server_base: format!("http://{}", address),
            input_schema: serde_json::json!({"type":"object"}),
            body_format: "json".to_string(),
            query_parameters: vec!["q".to_string()],
            auth_header: Some("Bearer test-token".to_string()),
            auth_header_name: "Authorization".to_string(),
        };
        let context = SkillContext::new(
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            ravenbot_core::SandboxTier::OsLevel,
        );
        let result = skill
            .execute(&context, serde_json::json!({"id":"42","q":"active"}))
            .await
            .expect("execute");
        assert!(result.success, "result: {result:?}");

        let request = server.await.expect("server");
        assert!(
            request.starts_with("GET /pets/42?q=active "),
            "unexpected request line: {request}"
        );
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer test-token"),
            "missing credential header: {request}"
        );
    }

    #[tokio::test]
    async fn execute_sends_urlencoded_form_bodies() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("address");
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut buffer = [0u8; 4096];
            let read = socket.read(&mut buffer).await.expect("read");
            let request = String::from_utf8_lossy(&buffer[..read]).to_string();
            let _ = socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\nconnection: close\r\n\r\n{\"ok\":true}",
                )
                .await;
            request
        });

        let skill = OpenApiSkill {
            id: "legacy__upload".to_string(),
            name: "Upload".to_string(),
            description: String::new(),
            method: "POST".to_string(),
            path_template: "/upload".to_string(),
            server_base: format!("http://{}", address),
            input_schema: serde_json::json!({"type":"object"}),
            body_format: "form".to_string(),
            query_parameters: vec!["notify".to_string()],
            auth_header: None,
            auth_header_name: "Authorization".to_string(),
        };
        let context = SkillContext::new(
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            ravenbot_core::SandboxTier::OsLevel,
        );
        let result = skill
            .execute(
                &context,
                serde_json::json!({"name":"report","size":3,"notify":"true"}),
            )
            .await
            .expect("execute");
        assert!(result.success, "result: {result:?}");

        let request = server.await.expect("server");
        let (head, body) = request.split_once("\r\n\r\n").unwrap_or((&request, ""));
        assert!(
            head.starts_with("POST /upload?notify=true "),
            "unexpected request line: {request}"
        );
        assert!(
            head.to_ascii_lowercase()
                .contains("content-type: application/x-www-form-urlencoded"),
            "unexpected content type: {request}"
        );
        assert_eq!(
            body, "name=report&size=3",
            "unexpected form body: {request}"
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
            loaded
                .iter()
                .all(|o| o.server_base == "https://api.petstore.test/v3"),
            "legacy re-derive failed: {:?}",
            loaded
                .iter()
                .map(|o| o.server_base.clone())
                .collect::<Vec<_>>()
        );
    }
}
