use sqlx::SqlitePool;
use uuid::Uuid;
use chrono::Utc;

pub struct PluginStore {
    pool: SqlitePool,
}

impl PluginStore {
    pub fn new(pool: SqlitePool) -> Self { Self { pool } }

    /// Pool accessor (OpenAPI operation persistence + skill assembly).
    pub fn pool(&self) -> &SqlitePool { &self.pool }

    pub async fn ensure_tables(&self) -> Result<(), String> {
        sqlx::query(r#"CREATE TABLE IF NOT EXISTS plugins (
            id TEXT PRIMARY KEY, name TEXT NOT NULL, description TEXT NOT NULL,
            logo TEXT, manifest_url TEXT, openapi_spec TEXT, enabled INTEGER DEFAULT 1,
            created_at TEXT NOT NULL
        )"#).execute(&self.pool).await.map_err(|e| e.to_string())?;
        // Migration safety (P8): global-scope column (mirrors migration 019)
        let _ = sqlx::query("ALTER TABLE plugins ADD COLUMN enabled_global INTEGER NOT NULL DEFAULT 0")
            .execute(&self.pool).await;
        sqlx::query(r#"CREATE TABLE IF NOT EXISTS bot_plugins (
            bot_id TEXT NOT NULL, plugin_id TEXT NOT NULL, enabled INTEGER DEFAULT 1,
            PRIMARY KEY (bot_id, plugin_id)
        )"#).execute(&self.pool).await.map_err(|e| e.to_string())?;
        Ok(())
    }
    pub async fn upsert_plugin(&self, id: &str, name: &str, description: &str, logo: Option<&str>, spec: &str) -> Result<(), String> {
        sqlx::query(r#"INSERT OR REPLACE INTO plugins (id, name, description, logo, openapi_spec, enabled, created_at) VALUES (?, ?, ?, ?, ?, 1, ?)"#)
            .bind(id).bind(name).bind(description).bind(logo.unwrap_or("")).bind(spec)
            .bind(Utc::now().to_rfc3339())
            .execute(&self.pool).await.map_err(|e| e.to_string())?;
        Ok(())
    }
    pub async fn list_plugins(&self, q: Option<&str>) -> Result<Vec<(String,String,String,String)>, String> {
        let rows: Vec<(String,String,String,String)> = if let Some(query) = q {
            sqlx::query_as(r#"SELECT id, name, description, logo FROM plugins WHERE id LIKE ? OR name LIKE ? LIMIT 100"#)
                .bind(format!("%{}%", query)).bind(format!("%{}%", query))
                .fetch_all(&self.pool).await.map_err(|e| e.to_string())?
        } else {
            sqlx::query_as(r#"SELECT id, name, description, logo FROM plugins LIMIT 100"#)
                .fetch_all(&self.pool).await.map_err(|e| e.to_string())?
        };
        Ok(rows)
    }
    pub async fn set_bot_plugin(&self, bot_id: Uuid, plugin_id: &str, enabled: bool) -> Result<(), String> {
        if enabled {
            sqlx::query("INSERT OR REPLACE INTO bot_plugins (bot_id, plugin_id, enabled) VALUES (?, ?, 1)")
                .bind(bot_id.to_string()).bind(plugin_id).execute(&self.pool).await.map_err(|e| e.to_string())?;
        } else {
            sqlx::query("DELETE FROM bot_plugins WHERE bot_id = ? AND plugin_id = ?")
                .bind(bot_id.to_string()).bind(plugin_id).execute(&self.pool).await.map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    pub async fn list_bot_plugins(&self, bot_id: Uuid) -> Result<Vec<String>, String> {
        let rows: Vec<(String,)> = sqlx::query_as("SELECT plugin_id FROM bot_plugins WHERE bot_id = ? AND enabled=1")
            .bind(bot_id.to_string()).fetch_all(&self.pool).await.map_err(|e| e.to_string())?;
        Ok(rows.into_iter().map(|r| r.0).collect())
    }
    pub async fn list_enabled_for_bot(&self, bot_id: Uuid) -> Result<Vec<(String,String,String,String)>, String> {
        let rows: Vec<(String,String,String,String)> = sqlx::query_as(
            r#"SELECT p.id, p.name, p.description, p.logo FROM plugins p
               JOIN bot_plugins bp ON p.id = bp.plugin_id WHERE bp.bot_id = ? AND bp.enabled=1"#)
            .bind(bot_id.to_string()).fetch_all(&self.pool).await.map_err(|e| e.to_string())?;
        if !rows.is_empty() {
            return Ok(rows);
        }
        // MCP-style fallback (P8): a bot with no enabled plugin assignments
        // sees every globally-enabled plugin — mirrors skills_for_bot in the
        // MCP registry so "enable once, use everywhere" behaves identically.
        self.list_global_plugins().await
    }

    /// Toggle a plugin's global scope (available to every bot with no
    /// per-bot assignments).
    pub async fn set_plugin_global(&self, plugin_id: &str, enabled: bool) -> Result<(), String> {
        sqlx::query("UPDATE plugins SET enabled_global = ? WHERE id = ?")
            .bind(if enabled { 1 } else { 0 })
            .bind(plugin_id)
            .execute(&self.pool).await.map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Plugins enabled for every bot (global scope).
    pub async fn list_global_plugins(&self) -> Result<Vec<(String,String,String,String)>, String> {
        let rows: Vec<(String,String,String,String)> = sqlx::query_as(
            r#"SELECT id, name, description, logo FROM plugins
               WHERE enabled = 1 AND enabled_global = 1 ORDER BY name"#)
            .fetch_all(&self.pool).await.map_err(|e| e.to_string())?;
        Ok(rows)
    }
}

#[cfg(test)]
mod scope_tests {
    use super::*;

    async fn temp_store() -> PluginStore {
        let path = std::path::PathBuf::from(std::env::temp_dir())
            .join(format!("ravenbot-plugins-test-{}.db", Uuid::new_v4()));
        let db = ravenbot_db::Database::new(&path).await.expect("temp db");
        let store = PluginStore::new(db.pool().clone());
        store.ensure_tables().await.expect("ensure tables");
        store
    }

    #[tokio::test]
    async fn global_plugin_falls_back_for_unassigned_bots() {
        let store = temp_store().await;
        store.upsert_plugin("gmail", "Gmail", "Email", None, "{}").await.unwrap();
        store.upsert_plugin("jira", "Jira", "Issues", None, "{}").await.unwrap();
        store.set_plugin_global("gmail", true).await.unwrap();

        // A bot with zero assignments sees globally-enabled plugins.
        let seen = store.list_enabled_for_bot(Uuid::new_v4()).await.unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].0, "gmail");

        // A bot with its own assignment ignores the global set (MCP parity).
        let bot = ravenbot_core::Bot::new("PluginScopeBot", "t");
        ravenbot_db::queries::BotQueries::insert(store.pool(), &bot).await.unwrap();
        store.set_bot_plugin(bot.id, "jira", true).await.unwrap();
        let seen = store.list_enabled_for_bot(bot.id).await.unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].0, "jira");

        // Turning global off hides it from unassigned bots again.
        store.set_plugin_global("gmail", false).await.unwrap();
        assert!(store.list_enabled_for_bot(Uuid::new_v4()).await.unwrap().is_empty());
        assert!(store.list_global_plugins().await.unwrap().is_empty());
    }
}
