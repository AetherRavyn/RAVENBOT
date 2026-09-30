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

pub struct McpRegistry {
    pool: sqlx::SqlitePool,
    /// Discovered tools per server id: (cached_at, tools)
    tools_cache: std::sync::Mutex<HashMap<String, (std::time::Instant, Vec<McpTool>)>>,
}

impl McpRegistry {
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self {
            pool,
            tools_cache: std::sync::Mutex::new(HashMap::new()),
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
        let rows: Vec<(String,String,String,String,String,String,String,String,i64,i64,String)> = if let Some(cat) = category {
            sqlx::query_as("SELECT id, name, description, category, icon, command, args, env_keys, enabled, is_custom, created_at FROM mcp_servers WHERE category = ? ORDER BY is_custom DESC, name")
                .bind(cat).fetch_all(&self.pool).await.map_err(|e| e.to_string())?
        } else {
            sqlx::query_as("SELECT id, name, description, category, icon, command, args, env_keys, enabled, is_custom, created_at FROM mcp_servers ORDER BY is_custom DESC, category, name")
                .fetch_all(&self.pool).await.map_err(|e| e.to_string())?
        };
        Ok(rows.into_iter().map(|r| McpServerConfig {
            id: r.0, name: r.1, description: r.2, category: r.3, icon: r.4, command: r.5,
            args: serde_json::from_str(&r.6).unwrap_or_default(),
            env_keys: serde_json::from_str(&r.7).unwrap_or_default(),
            enabled_by_default: r.8 != 0,
            is_custom: r.9 != 0,
        }).collect())
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
                enabled: c.enabled_by_default,
                is_custom: c.is_custom,
                env_configured,
                assigned_bot_ids: assigned_bots,
                tools_count,
            });
        }

        Ok(summaries)
    }

    pub async fn get_server_config(&self, id: &str) -> Result<Option<McpServerConfig>, String> {
        let row: Option<(String,String,String,String,String,String,String,String,i64,i64,String)> = sqlx::query_as(
            "SELECT id, name, description, category, icon, command, args, env_keys, enabled, is_custom, created_at FROM mcp_servers WHERE id = ?"
        ).bind(id).fetch_optional(&self.pool).await.map_err(|e| e.to_string())?;

        if let Some(r) = row {
            Ok(Some(McpServerConfig {
                id: r.0, name: r.1, description: r.2, category: r.3, icon: r.4, command: r.5,
                args: serde_json::from_str(&r.6).unwrap_or_default(),
                env_keys: serde_json::from_str(&r.7).unwrap_or_default(),
                enabled_by_default: r.8 != 0,
                is_custom: r.9 != 0,
            }))
        } else {
            // Fallback to built-in all_servers
            Ok(all_servers().into_iter().find(|s| s.id == id))
        }
    }

    pub async fn save_custom_server(&self, config: McpServerConfig) -> Result<(), String> {
        let clean_id = config.id.trim().to_lowercase().replace(' ', "-");
        if clean_id.is_empty() {
            return Err("Server ID cannot be empty".to_string());
        }

        sqlx::query(
            r#"INSERT OR REPLACE INTO mcp_servers
            (id, name, description, category, icon, command, args, env_keys, enabled, is_custom, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?)"#
        )
        .bind(&clean_id)
        .bind(&config.name)
        .bind(&config.description)
        .bind(&config.category)
        .bind(if config.icon.is_empty() { "⚡" } else { &config.icon })
        .bind(&config.command)
        .bind(serde_json::to_string(&config.args).unwrap_or_else(|_| "[]".into()))
        .bind(serde_json::to_string(&config.env_keys).unwrap_or_else(|_| "[]".into()))
        .bind(if config.enabled_by_default { 1 } else { 0 })
        .bind(chrono::Utc::now().to_rfc3339())
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
        let rows: Vec<(String, String)> = sqlx::query_as("SELECT key, value FROM mcp_server_env WHERE server_id = ?")
            .bind(server_id).fetch_all(&self.pool).await.map_err(|e| e.to_string())?;
        
        let mut map = HashMap::new();
        for (k, v) in rows {
            map.insert(k, v);
        }
        Ok(map)
    }

    pub async fn save_server_env(&self, server_id: &str, env: HashMap<String, String>) -> Result<(), String> {
        let now = chrono::Utc::now().to_rfc3339();
        for (k, v) in env {
            if v.trim().is_empty() {
                sqlx::query("DELETE FROM mcp_server_env WHERE server_id = ? AND key = ?")
                    .bind(server_id).bind(&k).execute(&self.pool).await.map_err(|e| e.to_string())?;
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
        client.test_connection().await
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

    pub async fn skills_for_bot(&self, bot_id: uuid::Uuid) -> Result<Vec<Arc<dyn Skill>>, String> {
        // Enabled servers for this bot, or globally enabled if none set per-bot
        let rows: Vec<(String,)> = sqlx::query_as("SELECT server_id FROM mcp_bot_servers WHERE bot_id = ? AND enabled=1").bind(bot_id.to_string()).fetch_all(&self.pool).await.map_err(|e| e.to_string())?;
        let server_ids: Vec<String> = if rows.is_empty() {
            // Fall back to globally enabled servers
            let global: Vec<(String,)> = sqlx::query_as("SELECT id FROM mcp_servers WHERE enabled=1").fetch_all(&self.pool).await.map_err(|e| e.to_string())?;
            global.into_iter().map(|r| r.0).collect()
        } else {
            rows.into_iter().map(|r| r.0).collect()
        };

        let mut skills: Vec<Arc<dyn Skill>> = Vec::new();
        for sid in server_ids {
            if let Ok(Some(cfg)) = self.get_server_config(&sid).await {
                // Env precedence: DB env (saved per-connector) → OS env for any
                // configured keys still missing
                let mut env = self.get_server_env(&sid).await.unwrap_or_default();
                for key in &cfg.env_keys {
                    if !env.contains_key(key) {
                        if let Ok(val) = std::env::var(key) {
                            if !val.trim().is_empty() {
                                env.insert(key.clone(), val);
                            }
                        }
                    }
                }
                let client = crate::client::McpClient::with_env(cfg.clone(), env.clone());

                // Discover once per TTL window: spawning MCP servers on every
                // message made runs slow (5s spawn timeouts per server)
                let tools = match self.cached_fresh_tools(&sid) {
                    Some(tools) => tools,
                    None => {
                        let discovered = client.list_tools().await.unwrap_or_default();
                        self.store_tools(&sid, discovered.clone());
                        discovered
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
        }
        // Cap per-bot MCP tools; the runtime's overall cap (32) still applies
        if skills.len() > 24 { skills.truncate(24); }
        Ok(skills)
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

        // Stage 2: prefix heuristic over the server catalog
        if let Ok(servers) = self.list_servers(None).await {
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

    pub async fn merged_for_bot(&self, bot_id: uuid::Uuid, builtin: &ravenbot_skills::SkillRegistry) -> Vec<Arc<dyn Skill>> {
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

    async fn temp_registry() -> McpRegistry {
        let path = PathBuf::from(std::env::temp_dir())
            .join(format!("ravenbot-mcp-test-{}.db", uuid::Uuid::new_v4()));
        let db = ravenbot_db::Database::new(&path).await.expect("temp db");
        let reg = McpRegistry::new(db.pool().clone());
        reg.ensure_tables().await.expect("ensure tables");
        reg
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
        let mut bot = ravenbot_core::Bot::new("McpTestBot", "registry test");
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
}
