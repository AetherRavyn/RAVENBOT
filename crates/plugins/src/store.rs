use sqlx::SqlitePool;
use uuid::Uuid;
use chrono::Utc;

pub struct PluginStore {
    pool: SqlitePool,
}

impl PluginStore {
    pub fn new(pool: SqlitePool) -> Self { Self { pool } }
    pub async fn ensure_tables(&self) -> Result<(), String> {
        sqlx::query(r#"CREATE TABLE IF NOT EXISTS plugins (
            id TEXT PRIMARY KEY, name TEXT NOT NULL, description TEXT NOT NULL,
            logo TEXT, manifest_url TEXT, openapi_spec TEXT, enabled INTEGER DEFAULT 1,
            created_at TEXT NOT NULL
        )"#).execute(&self.pool).await.map_err(|e| e.to_string())?;
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
        Ok(rows)
    }
}
