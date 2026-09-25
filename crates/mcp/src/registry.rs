use crate::client::McpTool;
use crate::servers::{all_servers, McpServerConfig, McpServerSummary, McpTestResult};
use async_trait::async_trait;
use ravenbot_core::Permission;
use ravenbot_skills::{Skill, SkillContext, SkillError, SkillResult};
use std::collections::HashMap;
use std::sync::Arc;

struct McpSkill {
    tool: McpTool,
    config: McpServerConfig,
    env: HashMap<String, String>,
}

#[async_trait]
impl Skill for McpSkill {
    fn id(&self) -> &str { &self.tool.name }
    fn name(&self) -> &str { &self.tool.name }
    fn description(&self) -> &str { &self.tool.description }
    fn version(&self) -> &str { "1.0.0-mcp" }
    fn required_permissions(&self) -> Vec<Permission> {
        // Map MCP server to permission
        if self.config.id == "filesystem" || self.config.id == "git" {
            vec![Permission::FileSystem { paths: vec![".".into()] }]
        } else if ["postgres","mysql","sqlite","mongodb","redis","supabase"].contains(&self.config.id.as_str()) {
            vec![Permission::FileSystem { paths: vec![".".into()] }]
        } else {
            vec![Permission::Network { domains: vec!["*".into()] }]
        }
    }
    fn input_schema(&self) -> serde_json::Value { self.tool.input_schema.clone() }
    async fn execute(&self, _ctx: &SkillContext, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let client = crate::client::McpClient::with_env(self.config.clone(), self.env.clone());
        match client.call_tool(&self.tool.name, args).await {
            Ok(v) => Ok(SkillResult::success(v)),
            Err(e) => Err(SkillError::Execution(e)),
        }
    }
}

/// How long a server's discovered tools stay cached before a re-discovery
const TOOLS_CACHE_TTL_SECS: u64 = 600;

/// Name rules (OpenMausBot parity): 1-32 lowercase letters/digits/_/-,
/// starting with a letter; harness-owned and reserved names rejected.
fn mcp_name_error(name: &str) -> Option<String> {
    if name.len() > 32 || name.is_empty() {
        return Some("Use 1-32 lowercase letters, numbers, underscores, or hyphens, starting with a letter.".to_string());
    }
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return Some("Use 1-32 lowercase letters, numbers, underscores, or hyphens, starting with a letter.".to_string()),
    }
    if !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-') {
        return Some("Use 1-32 lowercase letters, numbers, underscores, or hyphens, starting with a letter.".to_string());
    }
    const RESERVED: &[&str] = &[
        "ogb", "computer", "agents", "composio", "browser", "phone", "dweb",
        "openmausbot_connectors", "openmausbot_phone", "ravenbot", "builtin",
    ];
    if RESERVED.contains(&name) {
        return Some("That name is reserved.".to_string());
    }
    None
}

fn env_name_error(name: &str) -> Option<String> {
    if name.is_empty() || name.len() > 128 {
        return Some(format!("Environment variable \"{}\" is not valid.", name));
    }
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return Some(format!("Environment variable \"{}\" is not valid.", name)),
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Some(format!("Environment variable \"{}\" is not valid.", name));
    }
    // Harness-owned: must never be requestable by a custom server.
    if name == "ELECTRON_RUN_AS_NODE" || name.starts_with("OMB_") || name.starts_with("OGB_") {
        return Some(format!("Environment variable \"{}\" is reserved.", name));
    }
    None
}

/// Transport stored as lowercase text ('stdio' / 'http'), matching serde.
fn parse_transport(stored: &str) -> crate::servers::McpTransport {
    if stored == "http" {
        crate::servers::McpTransport::Http
    } else {
        crate::servers::McpTransport::Stdio
    }
}

/// URL XOR command (P8): a custom server is HTTP iff it carries a URL;
/// stdio servers must carry a command. Exactly one of the two — never both,
/// never neither. Returns (transport, normalized command, normalized url).
fn validate_server_for_save(
    id: &str,
    command: &str,
    url: Option<&str>,
    args: &[String],
    env_keys: &[String],
    headers: &HashMap<String, String>,
) -> Result<(crate::servers::McpTransport, String, Option<String>), String> {
    if let Some(err) = mcp_name_error(id) {
        return Err(err);
    }
    let command = command.trim().to_string();
    let url = url
        .map(|u| u.trim().trim_end_matches('/').to_string())
        .filter(|u| !u.is_empty());

    if args.len() > 64 {
        return Err("Use at most 64 arguments.".to_string());
    }
    if args.iter().any(|a| a.len() > 4096) {
        return Err("Each argument must be at most 4096 chars.".to_string());
    }
    if env_keys.len() > 64 {
        return Err("Use at most 64 environment variables.".to_string());
    }
    for key in env_keys {
        if let Some(err) = env_name_error(key) {
            return Err(err);
        }
    }

    match &url {
        Some(u) => {
            if !command.is_empty() {
                return Err("Provide either a command (stdio) or a URL (HTTP), not both.".to_string());
            }
            if !(u.starts_with("http://") || u.starts_with("https://")) || u.len() > 2048 {
                return Err("URL must start with http:// or https:// (max 2048 chars).".to_string());
            }
            if headers.len() > 32 {
                return Err("Use at most 32 headers.".to_string());
            }
            for (k, v) in headers {
                if k.is_empty()
                    || k.len() > 128
                    || k.chars().any(|c| c.is_control() || c == ':')
                {
                    return Err(format!("Header name \"{}\" is not valid.", k));
                }
                if v.len() > 2048 {
                    return Err(format!("Header \"{}\" value must be at most 2048 chars.", k));
                }
            }
            Ok((crate::servers::McpTransport::Http, String::new(), Some(u.clone())))
        }
        None => {
            if command.is_empty() || command.len() > 1024 {
                return Err("Command must be 1-1024 chars (or provide a URL for an HTTP server).".to_string());
            }
            Ok((crate::servers::McpTransport::Stdio, command, None))
        }
    }
}

/// One `mcp_servers` SELECT row: id, name, description, category, icon,
/// command, args, env_keys, enabled, is_custom, created_at, url, transport,
/// headers_json.
type ServerRow = (
    String, String, String, String, String, String, String, String,
    i64, i64, String, Option<String>, String, Option<String>,
);

pub struct McpRegistry {
    pool: sqlx::SqlitePool,
    /// Discovered tools per server id: (cached_at, tools)
    tools_cache: std::sync::Mutex<HashMap<String, (std::time::Instant, Vec<McpTool>)>>,
    /// Servers with a background discovery task currently in flight (so we
    /// don't spawn duplicate `npx` processes for the same server).
    discovering: std::sync::Mutex<std::collections::HashSet<String>>,
}

impl McpRegistry {
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self {
            pool,
            tools_cache: std::sync::Mutex::new(HashMap::new()),
            discovering: std::sync::Mutex::new(std::collections::HashSet::new()),
        }
    }

    /// Pool accessor (tests need to insert prerequisite rows)
    pub fn pool_ref(&self) -> &sqlx::SqlitePool {
        &self.pool
    }

    /// Freshly-cached tools for a server (within TTL), if any
    fn cached_fresh_tools(&self, server_id: &str) -> Option<Vec<McpTool>> {
        let cache = self.tools_cache.lock().ok()?;
        let (at, tools) = cache.get(server_id)?;
        if at.elapsed().as_secs() < TOOLS_CACHE_TTL_SECS {
            Some(tools.clone())
        } else {
            None
        }
    }

    /// Store discovered tools for a server (also used by resolution lookups)
    fn store_tools(&self, server_id: &str, tools: Vec<McpTool>) {
        if let Ok(mut cache) = self.tools_cache.lock() {
            cache.insert(server_id.to_string(), (std::time::Instant::now(), tools));
        }
    }

    pub async fn ensure_tables(&self) -> Result<(), String> {
        sqlx::query(r#"CREATE TABLE IF NOT EXISTS mcp_servers (
            id TEXT PRIMARY KEY, name TEXT NOT NULL, description TEXT NOT NULL, category TEXT NOT NULL,
            icon TEXT, command TEXT, args TEXT, env_keys TEXT, enabled INTEGER DEFAULT 0,
            is_custom INTEGER DEFAULT 0, created_at TEXT NOT NULL
        )"#).execute(&self.pool).await.map_err(|e| e.to_string())?;

        // Migration safety: ensure is_custom column exists
        let _ = sqlx::query("ALTER TABLE mcp_servers ADD COLUMN is_custom INTEGER DEFAULT 0")
            .execute(&self.pool).await;

        // Migration safety (P8): remote-server columns (mirrors migration 019)
        let _ = sqlx::query("ALTER TABLE mcp_servers ADD COLUMN url TEXT")
            .execute(&self.pool).await;
        let _ = sqlx::query("ALTER TABLE mcp_servers ADD COLUMN transport TEXT NOT NULL DEFAULT 'stdio'")
            .execute(&self.pool).await;
        let _ = sqlx::query("ALTER TABLE mcp_servers ADD COLUMN headers_json TEXT")
            .execute(&self.pool).await;

        sqlx::query(r#"CREATE TABLE IF NOT EXISTS mcp_bot_servers (
            bot_id TEXT NOT NULL, server_id TEXT NOT NULL, enabled INTEGER DEFAULT 1,
            PRIMARY KEY (bot_id, server_id)
        )"#).execute(&self.pool).await.map_err(|e| e.to_string())?;

        sqlx::query(r#"CREATE TABLE IF NOT EXISTS mcp_server_env (
            server_id TEXT NOT NULL,
            key TEXT NOT NULL,
            value TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (server_id, key)
        )"#).execute(&self.pool).await.map_err(|e| e.to_string())?;

        // Seed & Sync all 75+ built-in servers
        for s in all_servers() {
            sqlx::query(
                r#"INSERT INTO mcp_servers (id, name, description, category, icon, command, args, env_keys, enabled, is_custom, created_at)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?)
                ON CONFLICT(id) DO UPDATE SET
                    name = excluded.name,
                    description = excluded.description,
                    category = excluded.category,
                    icon = excluded.icon,
                    command = excluded.command,
                    args = excluded.args,
                    env_keys = excluded.env_keys
                WHERE mcp_servers.is_custom = 0"#
            )
            .bind(&s.id)
            .bind(&s.name)
            .bind(&s.description)
            .bind(&s.category)
            .bind(&s.icon)
            .bind(&s.command)
            .bind(serde_json::to_string(&s.args).unwrap_or_else(|_| "[]".into()))
            .bind(serde_json::to_string(&s.env_keys).unwrap_or_else(|_| "[]".into()))
            .bind(if s.enabled_by_default { 1 } else { 0 })
            .bind(chrono::Utc::now().to_rfc3339())
            .execute(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub async fn list_servers(&self, category: Option<&str>) -> Result<Vec<McpServerConfig>, String> {
        let rows: Vec<ServerRow> = if let Some(cat) = category {
            sqlx::query_as("SELECT id, name, description, category, icon, command, args, env_keys, enabled, is_custom, created_at, url, transport, headers_json FROM mcp_servers WHERE category = ? ORDER BY is_custom DESC, name")
                .bind(cat).fetch_all(&self.pool).await.map_err(|e| e.to_string())?
        } else {
            sqlx::query_as("SELECT id, name, description, category, icon, command, args, env_keys, enabled, is_custom, created_at, url, transport, headers_json FROM mcp_servers ORDER BY is_custom DESC, category, name")
                .fetch_all(&self.pool).await.map_err(|e| e.to_string())?
        };
        Ok(rows.into_iter().map(|r| {
            let verified = crate::servers::is_verified(&r.0);
            McpServerConfig {
            id: r.0, name: r.1, description: r.2, category: r.3, icon: r.4, command: r.5,
            args: serde_json::from_str(&r.6).unwrap_or_default(),
            env_keys: serde_json::from_str(&r.7).unwrap_or_default(),
            env_fields: Vec::new(),
            url: r.11,
            headers: r.13
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default(),
            auth: None,
            transport: parse_transport(&r.12),
            mcp_type: None,
            enabled_by_default: r.8 != 0,
            is_custom: r.9 != 0,
            verified,
            source: None,
        }}).collect())
    }

    pub async fn list_server_summaries(&self, category: Option<&str>) -> Result<Vec<McpServerSummary>, String> {
        let configs = self.list_servers(category).await?;
        
        // Fetch all assignments
        let assignments: Vec<(String, String)> = sqlx::query_as("SELECT server_id, bot_id FROM mcp_bot_servers WHERE enabled = 1")
            .fetch_all(&self.pool).await.unwrap_or_default();
        
        // Fetch all configured keys
        let configured_keys: Vec<(String, String)> = sqlx::query_as("SELECT server_id, key FROM mcp_server_env WHERE LENGTH(TRIM(value)) > 0")
            .fetch_all(&self.pool).await.unwrap_or_default();

        let mut map_assigned: HashMap<String, Vec<String>> = HashMap::new();
        for (sid, bid) in assignments {
            map_assigned.entry(sid).or_default().push(bid);
        }

        let mut map_env_keys: HashMap<String, Vec<String>> = HashMap::new();
        for (sid, key) in configured_keys {
            map_env_keys.entry(sid).or_default().push(key);
        }

        let mut summaries = Vec::new();
        for c in configs {
            let assigned_bots = map_assigned.get(&c.id).cloned().unwrap_or_default();
            let set_keys = map_env_keys.get(&c.id).cloned().unwrap_or_default();
            let env_configured = if c.env_keys.is_empty() {
                true
            } else {
                c.env_keys.iter().all(|k| set_keys.contains(k))
            };

            // Calculate tool count directly from synthesized tools
            let client = crate::client::McpClient::new(c.clone());
            let tools_count = client.synthesized_tools().len();

            summaries.push(McpServerSummary {
                id: c.id.clone(),
                name: c.name,
                description: c.description,
                category: c.category,
                icon: c.icon,
                command: c.command,
                args: c.args,
                env_keys: c.env_keys,
                env_fields: c.env_fields,
                url: c.url,
                transport: c.transport.clone(),
                enabled: c.enabled_by_default,
                is_custom: c.is_custom,
                verified: c.verified,
                env_configured,
                assigned_bot_ids: assigned_bots,
                tools_count,
            });
        }

        Ok(summaries)
    }

    pub async fn get_server_config(&self, id: &str) -> Result<Option<McpServerConfig>, String> {
        let row: Option<ServerRow> = sqlx::query_as(
            "SELECT id, name, description, category, icon, command, args, env_keys, enabled, is_custom, created_at, url, transport, headers_json FROM mcp_servers WHERE id = ?"
        ).bind(id).fetch_optional(&self.pool).await.map_err(|e| e.to_string())?;

        if let Some(r) = row {
            let verified = crate::servers::is_verified(&r.0);
            Ok(Some(McpServerConfig {
                id: r.0, name: r.1, description: r.2, category: r.3, icon: r.4, command: r.5,
                args: serde_json::from_str(&r.6).unwrap_or_default(),
                env_keys: serde_json::from_str(&r.7).unwrap_or_default(),
                env_fields: Vec::new(),
                url: r.11,
                headers: r.13
                    .as_deref()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or_default(),
                auth: None,
                transport: parse_transport(&r.12),
                mcp_type: None,
                enabled_by_default: r.8 != 0,
                is_custom: r.9 != 0,
                verified,
                source: None,
            }))
        } else {
            // Fallback to built-in all_servers
            Ok(all_servers().into_iter().find(|s| s.id == id))
        }
    }

    pub async fn save_custom_server(&self, config: McpServerConfig) -> Result<(), String> {
        let clean_id = config.id.trim().to_lowercase().replace(' ', "-");
        // New servers start disabled until tested + explicitly enabled
        // (OpenMausBot parity: inert until proven).
        let (transport, command, url) = validate_server_for_save(
            &clean_id,
            &config.command,
            config.url.as_deref(),
            &config.args,
            &config.env_keys,
            &config.headers,
        )?;
        let transport_str = if transport == crate::servers::McpTransport::Http { "http" } else { "stdio" };
        let headers_json = serde_json::to_string(&config.headers).unwrap_or_else(|_| "{}".into());

        sqlx::query(
            r#"INSERT OR REPLACE INTO mcp_servers
            (id, name, description, category, icon, command, args, env_keys, enabled, is_custom, created_at, url, transport, headers_json)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?, ?, ?)"#
        )
        .bind(&clean_id)
        .bind(&config.name)
        .bind(&config.description)
        .bind(&config.category)
        .bind(if config.icon.is_empty() { "⚡" } else { &config.icon })
        .bind(&command)
        .bind(serde_json::to_string(&config.args).unwrap_or_else(|_| "[]".into()))
        .bind(serde_json::to_string(&config.env_keys).unwrap_or_else(|_| "[]".into()))
        .bind(0)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(&url)
        .bind(transport_str)
        .bind(&headers_json)
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        Ok(())
    }

    pub async fn delete_server(&self, id: &str) -> Result<(), String> {
        sqlx::query("DELETE FROM mcp_servers WHERE id = ?").bind(id).execute(&self.pool).await.map_err(|e| e.to_string())?;
        sqlx::query("DELETE FROM mcp_bot_servers WHERE server_id = ?").bind(id).execute(&self.pool).await.map_err(|e| e.to_string())?;
        sqlx::query("DELETE FROM mcp_server_env WHERE server_id = ?").bind(id).execute(&self.pool).await.map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn set_server_enabled(&self, id: &str, enabled: bool) -> Result<(), String> {
        sqlx::query("UPDATE mcp_servers SET enabled = ? WHERE id = ?").bind(if enabled {1} else {0}).bind(id).execute(&self.pool).await.map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn get_server_env(&self, server_id: &str) -> Result<HashMap<String, String>, String> {
        // Values live here because process spawn needs them; API callers that
        // render to the UI must use `get_server_env_keys` (write-only).
        let rows: Vec<(String, String)> = sqlx::query_as("SELECT key, value FROM mcp_server_env WHERE server_id = ?")
            .bind(server_id).fetch_all(&self.pool).await.map_err(|e| e.to_string())?;

        let mut map = HashMap::new();
        for (k, v) in rows {
            map.insert(k, v);
        }
        Ok(map)
    }

    /// Write-only key names for UI rendering (never the values).
    pub async fn get_server_env_keys(&self, server_id: &str) -> Result<Vec<String>, String> {
        let rows: Vec<(String,)> = sqlx::query_as("SELECT key FROM mcp_server_env WHERE server_id = ?")
            .bind(server_id).fetch_all(&self.pool).await.map_err(|e| e.to_string())?;
        Ok(rows.into_iter().map(|(k,)| k).collect())
    }

    /// Save env values. Write-only semantics (OpenMausBot parity):
    /// - non-empty value  -> overwrite stored secret
    /// - empty value + key already stored -> KEEP (UI placeholder round-trip)
    /// - empty value + key unknown -> delete (explicit clear of a new row)
    pub async fn save_server_env(&self, server_id: &str, env: HashMap<String, String>) -> Result<(), String> {
        // Validate key names before touching the DB.
        for k in env.keys() {
            if let Some(err) = env_name_error(k) {
                return Err(err);
            }
        }
        let now = chrono::Utc::now().to_rfc3339();
        for (k, v) in env {
            if v.trim().is_empty() {
                let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mcp_server_env WHERE server_id = ? AND key = ?")
                    .bind(server_id).bind(&k).fetch_one(&self.pool).await.map_err(|e| e.to_string())?;
                if exists == 0 {
                    sqlx::query("DELETE FROM mcp_server_env WHERE server_id = ? AND key = ?")
                        .bind(server_id).bind(&k).execute(&self.pool).await.map_err(|e| e.to_string())?;
                }
                // else: keep saved value (write-only placeholder)
            } else {
                sqlx::query(
                    "INSERT OR REPLACE INTO mcp_server_env (server_id, key, value, updated_at) VALUES (?, ?, ?, ?)"
                )
                .bind(server_id).bind(&k).bind(&v).bind(&now).execute(&self.pool).await.map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }

    pub async fn test_server(&self, server_id: &str) -> Result<McpTestResult, String> {
        let config = self.get_server_config(server_id).await?
            .ok_or_else(|| format!("MCP server '{}' not found", server_id))?;
        let env = self.get_server_env(server_id).await.unwrap_or_default();
        let client = crate::client::McpClient::with_env(config, env);
        let mut result = client.test_connection().await?;
        // Be explicit about connectors whose launcher package was not found
        // during the catalog audit, so a failure is explained rather than
        // looking like a transient network problem.
        if !crate::servers::is_verified(server_id) {
            result.message = format!(
                "⚠️ Unverified connector — the upstream launcher package was not found during the catalog audit. {}",
                result.message
            );
        }
        Ok(result)
    }

    pub async fn batch_assign_bot_servers(&self, server_id: &str, bot_ids: Vec<uuid::Uuid>) -> Result<(), String> {
        // Remove all previous for this server
        sqlx::query("DELETE FROM mcp_bot_servers WHERE server_id = ?")
            .bind(server_id).execute(&self.pool).await.map_err(|e| e.to_string())?;
        
        for bid in bot_ids {
            sqlx::query("INSERT OR REPLACE INTO mcp_bot_servers (bot_id, server_id, enabled) VALUES (?, ?, 1)")
                .bind(bid.to_string()).bind(server_id).execute(&self.pool).await.map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub async fn batch_set_bot_servers(&self, bot_id: uuid::Uuid, server_ids: Vec<String>) -> Result<(), String> {
        sqlx::query("DELETE FROM mcp_bot_servers WHERE bot_id = ?")
            .bind(bot_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| e.to_string())?;

        for sid in server_ids {
            sqlx::query("INSERT OR REPLACE INTO mcp_bot_servers (bot_id, server_id, enabled) VALUES (?, ?, 1)")
                .bind(bot_id.to_string())
                .bind(sid)
                .execute(&self.pool)
                .await
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub async fn list_bot_servers(&self, bot_id: uuid::Uuid) -> Result<Vec<String>, String> {
        let rows: Vec<(String,)> = sqlx::query_as("SELECT server_id FROM mcp_bot_servers WHERE bot_id = ? AND enabled=1")
            .bind(bot_id.to_string()).fetch_all(&self.pool).await.map_err(|e| e.to_string())?;
        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    /// Server ids effective for a bot: its own enabled assignments, or every
    /// globally enabled server when the bot has none (INSERT-or-DELETE
    /// semantics make "zero enabled rows" identical to "no rows at all").
    pub async fn enabled_server_ids(&self, bot_id: uuid::Uuid) -> Result<Vec<String>, String> {
        let rows: Vec<(String,)> = sqlx::query_as("SELECT server_id FROM mcp_bot_servers WHERE bot_id = ? AND enabled=1").bind(bot_id.to_string()).fetch_all(&self.pool).await.map_err(|e| e.to_string())?;
        if !rows.is_empty() {
            return Ok(rows.into_iter().map(|r| r.0).collect());
        }
        let global: Vec<(String,)> = sqlx::query_as("SELECT id FROM mcp_servers WHERE enabled=1").fetch_all(&self.pool).await.map_err(|e| e.to_string())?;
        Ok(global.into_iter().map(|r| r.0).collect())
    }

    /// Config + resolved env for one server (DB env values win, then OS env
    /// for declared keys). One resolution path shared by the native loop and
    /// the external-engine forwarder.
    pub async fn server_config_with_env(&self, server_id: &str) -> Result<Option<(McpServerConfig, HashMap<String, String>)>, String> {
        let Some(cfg) = self.get_server_config(server_id).await? else {
            return Ok(None);
        };
        let mut env = self.get_server_env(server_id).await.unwrap_or_default();
        for key in &cfg.env_keys {
            if !env.contains_key(key) {
                if let Ok(val) = std::env::var(key) {
                    if !val.trim().is_empty() {
                        env.insert(key.clone(), val);
                    }
                }
            }
        }
        Ok(Some((cfg, env)))
    }

    pub async fn skills_for_bot(self: &Arc<Self>, bot_id: uuid::Uuid) -> Result<Vec<Arc<dyn Skill>>, String> {
        // Enabled servers for this bot, or globally enabled if none set per-bot
        let server_ids = self.enabled_server_ids(bot_id).await?;

        // Resolve config + env (DB env overrides OS env) up front.
        let mut resolved: Vec<(String, McpServerConfig, HashMap<String, String>)> = Vec::new();
        for sid in server_ids {
            if let Ok(Some((cfg, env))) = self.server_config_with_env(&sid).await {
                resolved.push((sid, cfg, env));
            }
        }

        // Build the per-bot tool list WITHOUT blocking the run on cold-starting
        // MCP servers. Fresh cache entries use their real discovered tools;
        // everything else gets its instant synthesized descriptors now, and the
        // real tools are discovered in the background for the next run. (Spawning
        // `npx -y …`/`uvx …` for 8 servers used to add ~50s to the first reply.)
        let mut skills: Vec<Arc<dyn Skill>> = Vec::new();
        let mut to_warm: Vec<(String, McpServerConfig, HashMap<String, String>)> = Vec::new();
        for (sid, cfg, env) in resolved {
            let client = crate::client::McpClient::with_env(cfg.clone(), env.clone());
            let tools = match self.cached_fresh_tools(&sid) {
                Some(tools) => tools,
                None => {
                    to_warm.push((sid.clone(), cfg.clone(), env.clone()));
                    client.synthesized_tools()
                }
            };
            for tool in tools {
                skills.push(Arc::new(McpSkill {
                    tool,
                    config: cfg.clone(),
                    env: env.clone(),
                }));
            }
        }
        self.spawn_discovery(to_warm);

        // Cap per-bot MCP tools; the runtime's overall cap (32) still applies
        if skills.len() > 24 { skills.truncate(24); }
        Ok(skills)
    }

    /// Discover tools for `servers` on a detached task and populate the cache.
    /// Never awaited by callers — the run has already used synthesized tools.
    fn spawn_discovery(
        self: &Arc<Self>,
        servers: Vec<(String, McpServerConfig, HashMap<String, String>)>,
    ) {
        if servers.is_empty() {
            return;
        }
        // Only warm servers that aren't already being discovered.
        let mut to_spawn = Vec::new();
        if let Ok(mut in_flight) = self.discovering.lock() {
            for (sid, cfg, env) in servers {
                if in_flight.insert(sid.clone()) {
                    to_spawn.push((sid, cfg, env));
                }
            }
        }
        if to_spawn.is_empty() {
            return;
        }

        let registry = self.clone();
        tokio::spawn(async move {
            let semaphore = Arc::new(tokio::sync::Semaphore::new(6));
            let mut set: tokio::task::JoinSet<(String, Vec<McpTool>)> = tokio::task::JoinSet::new();
            for (sid, cfg, env) in to_spawn {
                let sem = semaphore.clone();
                set.spawn(async move {
                    let _permit = sem.acquire_owned().await.ok();
                    let client = crate::client::McpClient::with_env(cfg, env);
                    let tools = client.list_tools().await.unwrap_or_default();
                    (sid, tools)
                });
            }
            while let Some(joined) = set.join_next().await {
                if let Ok((sid, tools)) = joined {
                    registry.store_tools(&sid, tools);
                    if let Ok(mut in_flight) = registry.discovering.lock() {
                        in_flight.remove(&sid);
                    }
                }
            }
        });
    }

    /// Resolve which enabled server owns a tool name (for dynamic tool calls
    /// the model makes that aren't among the pre-assembled per-bot skills).
    ///
    /// Two stages:
    /// 1. Tool-cache lookup across cached servers
    /// 2. Prefix heuristic for built-in synthesized names (`github_list_repos`
    ///    → server `github`)
    pub async fn resolve_tool(
        &self,
        tool_name: &str,
    ) -> Result<Option<(McpServerConfig, HashMap<String, String>)>, String> {
        // Stage 1: scan the tool cache for the owning server.
        // Clone the owning server id and drop the guard before any await.
        let owning_server: Option<String> = self.tools_cache.lock().ok().and_then(|cache| {
            cache
                .iter()
                .find(|(_, (_, tools))| tools.iter().any(|t| t.name == tool_name))
                .map(|(server_id, _)| server_id.clone())
        });
        if let Some(server_id) = owning_server {
            return self.load_resolved(&server_id).await;
        }

        // Stage 2: synthesized-name mapping across the catalog. Several
        // connectors expose aliases that do not share the server-id prefix
        // (`pg_query` → postgres, `vector_search` → pinecone, `browser_navigate`
        // → puppeteer, `read_file` → filesystem). Match those explicitly so
        // dynamically-called tools still resolve to their server.
        if let Ok(servers) = self.list_servers(None).await {
            for server in &servers {
                let client = crate::client::McpClient::new(server.clone());
                if client.synthesized_tools().iter().any(|t| t.name == tool_name) {
                    return self.load_resolved(&server.id).await;
                }
            }

            // Stage 3: prefix heuristic (`github_list_repos` → server `github`).
            for server in servers {
                let prefix = format!("{}_", server.id);
                if tool_name.starts_with(&prefix) {
                    return self.load_resolved(&server.id).await;
                }
            }
        }

        Ok(None)
    }

    /// Load config + env for a resolved server id
    async fn load_resolved(
        &self,
        server_id: &str,
    ) -> Result<Option<(McpServerConfig, HashMap<String, String>)>, String> {
        let Some(cfg) = self.get_server_config(server_id).await? else {
            return Ok(None);
        };
        let env = self.get_server_env(server_id).await.unwrap_or_default();
        Ok(Some((cfg, env)))
    }

    pub async fn merged_for_bot(self: &Arc<Self>, bot_id: uuid::Uuid, builtin: &ravenbot_skills::SkillRegistry) -> Vec<Arc<dyn Skill>> {
        let mut all = builtin.list();
        if let Ok(mcp_skills) = self.skills_for_bot(bot_id).await { all.extend(mcp_skills); }
        all
    }

    pub async fn set_bot_server(&self, bot_id: uuid::Uuid, server_id: &str, enabled: bool) -> Result<(), String> {
        if enabled {
            sqlx::query("INSERT OR REPLACE INTO mcp_bot_servers (bot_id, server_id, enabled) VALUES (?, ?, 1)")
                .bind(bot_id.to_string()).bind(server_id).execute(&self.pool).await.map_err(|e| e.to_string())?;
        } else {
            sqlx::query("DELETE FROM mcp_bot_servers WHERE bot_id = ? AND server_id = ?")
                .bind(bot_id.to_string()).bind(server_id).execute(&self.pool).await.map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod resolution_tests {
    use super::*;
    use std::path::PathBuf;

    fn base_config(id: &str) -> McpServerConfig {
        McpServerConfig {
            id: id.to_string(),
            name: id.to_string(),
            description: "test server".to_string(),
            category: "custom".to_string(),
            icon: "⚡".to_string(),
            command: String::new(),
            args: Vec::new(),
            env_keys: Vec::new(),
            env_fields: Vec::new(),
            url: None,
            headers: HashMap::new(),
            auth: None,
            transport: crate::servers::McpTransport::Stdio,
            mcp_type: None,
            enabled_by_default: false,
            is_custom: true,
            verified: true,
            source: None,
        }
    }

    async fn temp_registry() -> Arc<McpRegistry> {
        // Keep tests hermetic: never spawn npx/uvx during tool discovery.
        std::env::set_var("RAVENBOT_MCP_OFFLINE", "1");
        let path = PathBuf::from(std::env::temp_dir())
            .join(format!("ravenbot-mcp-test-{}.db", uuid::Uuid::new_v4()));
        let db = ravenbot_db::Database::new(&path).await.expect("temp db");
        let reg = Arc::new(McpRegistry::new(db.pool().clone()));
        reg.ensure_tables().await.expect("ensure tables");
        reg
    }

    #[tokio::test]
    async fn http_server_round_trips_url_transport_headers() {
        let reg = temp_registry().await;
        let mut cfg = base_config("remotetest");
        cfg.url = Some("https://mcp.example.com/api/".to_string());
        cfg.transport = crate::servers::McpTransport::Http;
        cfg.headers =
            HashMap::from([("Authorization".to_string(), "Bearer ${MY_MCP_TOKEN}".to_string())]);
        reg.save_custom_server(cfg).await.unwrap();

        let got = reg
            .get_server_config("remotetest")
            .await
            .unwrap()
            .expect("saved http server");
        assert_eq!(got.transport, crate::servers::McpTransport::Http);
        assert_eq!(got.url.as_deref(), Some("https://mcp.example.com/api"));
        assert_eq!(
            got.headers.get("Authorization").map(|s| s.as_str()),
            Some("Bearer ${MY_MCP_TOKEN}")
        );
        assert!(got.command.is_empty());

        // Summaries must stop nulling url/transport for remote servers.
        let summaries = reg.list_server_summaries(None).await.unwrap();
        let s = summaries.iter().find(|s| s.id == "remotetest").expect("summary present");
        assert_eq!(s.url.as_deref(), Some("https://mcp.example.com/api"));
        assert_eq!(s.transport, crate::servers::McpTransport::Http);
    }

    #[tokio::test]
    async fn url_xor_command_validation() {
        let reg = temp_registry().await;

        // Command AND url → rejected.
        let mut both = base_config("bothbad");
        both.command = "npx".into();
        both.url = Some("https://example.com/mcp".into());
        let err = reg.save_custom_server(both).await.unwrap_err();
        assert!(err.contains("not both"), "got: {err}");

        // Neither → rejected (stdio rules still require a command).
        let neither = base_config("neitherbad");
        let err = reg.save_custom_server(neither).await.unwrap_err();
        assert!(err.contains("Command must be 1-1024"), "got: {err}");

        // Non-http(s) url → rejected.
        let mut bad_scheme = base_config("schemebad");
        bad_scheme.url = Some("ftp://example.com/mcp".into());
        let err = reg.save_custom_server(bad_scheme).await.unwrap_err();
        assert!(err.contains("http://"), "got: {err}");
    }

    #[tokio::test]
    async fn stdio_server_still_saves_as_stdio() {
        let reg = temp_registry().await;
        let mut cfg = base_config("stdiotest");
        cfg.command = "npx".into();
        cfg.args = vec!["-y".into(), "@modelcontextprotocol/server-foo".into()];
        reg.save_custom_server(cfg).await.unwrap();
        let got = reg.get_server_config("stdiotest").await.unwrap().unwrap();
        assert_eq!(got.transport, crate::servers::McpTransport::Stdio);
        assert_eq!(got.command, "npx");
        assert!(got.url.is_none());
    }

    #[tokio::test]
    async fn prefix_heuristic_resolves_built_in_tool_names() {
        let reg = temp_registry().await;
        let resolved = reg.resolve_tool("github_list_repos").await.unwrap();
        assert!(resolved.is_some());
        let (cfg, _env) = resolved.unwrap();
        assert_eq!(cfg.id, "github");
    }

    #[tokio::test]
    async fn synthesized_alias_resolves_to_its_server() {
        // `pg_query` is synthesized for the `postgres` server but does not
        // share its id prefix — the alias map must still resolve it.
        let reg = temp_registry().await;
        let resolved = reg.resolve_tool("pg_query").await.unwrap();
        assert!(resolved.is_some(), "pg_query must resolve");
        assert_eq!(resolved.unwrap().0.id, "postgres");
    }

    #[tokio::test]
    async fn unknown_tools_resolve_to_none() {
        let reg = temp_registry().await;
        let resolved = reg.resolve_tool("totally_unknown_tool_xyz").await.unwrap();
        assert!(resolved.is_none());
    }

    #[tokio::test]
    async fn cache_stage_resolves_dynamic_tool_calls() {
        let reg = temp_registry().await;
        // Enable github globally so skills_for_bot falls back to it and
        // populates the tool cache
        reg.set_server_enabled("github", true).await.unwrap();
        let skills = reg.skills_for_bot(uuid::Uuid::new_v4()).await.unwrap();
        assert!(!skills.is_empty(), "globally enabled fallback should yield tools");

        // A dynamically-called tool from that listing must resolve to github
        let tool_name = skills[0].id().to_string();
        let resolved = reg.resolve_tool(&tool_name).await.unwrap();
        assert!(resolved.is_some());
        let (cfg, _env) = resolved.unwrap();
        assert_eq!(cfg.id, "github");
    }

    #[tokio::test]
    async fn per_bot_assignment_uses_assigned_servers() {
        let reg = temp_registry().await;
        // mcp_bot_servers references bots(id) — insert a real bot first
        let bot = ravenbot_core::Bot::new("McpTestBot", "registry test");
        ravenbot_db::queries::BotQueries::insert(reg.pool_ref(), &bot)
            .await
            .unwrap();
        reg.set_server_enabled("notion", false).await.unwrap();
        reg.set_bot_server(bot.id, "git", true).await.unwrap();

        let skills = reg.skills_for_bot(bot.id).await.unwrap();
        // git's synthesized tools (or live ones) — none should come from notion
        assert!(!skills.is_empty());
        let listed: Vec<String> = skills.iter().map(|s| s.id().to_string()).collect();
        assert!(listed.iter().all(|n| n.starts_with("git")), "got: {listed:?}");
    }

    #[tokio::test]
    async fn enabled_server_ids_matches_skills_for_bot_resolution() {
        // The engine forwarder shares this listing with skills_for_bot — both
        // must apply the same per-bot-else-global fallback.
        let reg = temp_registry().await;
        let bot = ravenbot_core::Bot::new("McpIdsBot", "registry test");
        ravenbot_db::queries::BotQueries::insert(reg.pool_ref(), &bot)
            .await
            .unwrap();

        // Global fallback: no per-bot rows → every enabled server id.
        reg.set_server_enabled("git", true).await.unwrap();
        reg.set_server_enabled("notion", false).await.unwrap();
        let global_ids = reg.enabled_server_ids(bot.id).await.unwrap();
        assert!(global_ids.iter().any(|id| id == "git"), "got: {global_ids:?}");
        assert!(!global_ids.iter().any(|id| id == "notion"), "disabled server leaked");

        // Per-bot assignment wins over the global listing.
        reg.set_bot_server(bot.id, "notion", true).await.unwrap();
        let bot_ids = reg.enabled_server_ids(bot.id).await.unwrap();
        assert_eq!(bot_ids, vec!["notion".to_string()]);

        // The ids the forwarder sees are exactly the servers the native loop
        // produced tools from.
        let skills = reg.skills_for_bot(bot.id).await.unwrap();
        let owners: Vec<String> = skills
            .iter()
            .map(|s| s.id().split('_').next().unwrap_or("").to_string())
            .collect();
        assert!(
            owners.iter().all(|o| bot_ids.iter().any(|b| b.starts_with(o.as_str()))),
            "skills {owners:?} outside listing {bot_ids:?}"
        );
    }
}
