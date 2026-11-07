use chrono::Utc;
use serde::Serialize;
use sqlx::SqlitePool;
use uuid::Uuid;

/// An installed OpenAPI app, in the shape the picker needs.
#[derive(Debug, Clone, Serialize)]
pub struct PluginSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub logo: String,
    pub enabled_global: bool,
    /// Parsed operations. A plugin with zero operations is not executable.
    pub operations: i64,
    pub has_credential: bool,
    pub requires_auth: bool,
    pub source_url: Option<String>,
    /// Optional override for specs whose server URL is a placeholder.
    pub base_url_override: Option<String>,
    /// Header the spec expects, used to prefill the credential form.
    pub credential_header_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginOperation {
    pub id: String,
    pub method: String,
    pub path: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginDetails {
    pub summary: PluginSummary,
    pub operations: Vec<PluginOperation>,
    /// Header name only. The secret is never returned to the renderer.
    pub credential_header: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginCredential {
    pub header_name: String,
    pub header_value: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OperationMatch {
    pub plugin_id: String,
    pub plugin_name: String,
    pub operation_id: String,
    pub description: String,
    pub method: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncFailure {
    pub plugin_id: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncReport {
    pub verified: usize,
    pub removed: usize,
    pub failed: Vec<SyncFailure>,
}

#[derive(Clone)]
pub struct PluginStore {
    pool: SqlitePool,
}

impl PluginStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Pool accessor (OpenAPI operation persistence + skill assembly).
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn ensure_tables(&self) -> Result<(), String> {
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS plugins (
            id TEXT PRIMARY KEY, name TEXT NOT NULL, description TEXT NOT NULL,
            logo TEXT, manifest_url TEXT, openapi_spec TEXT, enabled INTEGER DEFAULT 1,
            created_at TEXT NOT NULL
        )"#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        // Migrations own these on a real database; the ALTERs keep the plugin
        // store self-contained for in-memory tests and very old local DBs.
        let _ =
            sqlx::query("ALTER TABLE plugins ADD COLUMN enabled_global INTEGER NOT NULL DEFAULT 0")
                .execute(&self.pool)
                .await;
        let _ =
            sqlx::query("ALTER TABLE plugins ADD COLUMN requires_auth INTEGER NOT NULL DEFAULT 0")
                .execute(&self.pool)
                .await;
        let _ = sqlx::query("ALTER TABLE plugins ADD COLUMN base_url_override TEXT")
            .execute(&self.pool)
            .await;
        let _ = sqlx::query("ALTER TABLE plugins ADD COLUMN credential_header_hint TEXT")
            .execute(&self.pool)
            .await;
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS bot_plugins (
            bot_id TEXT NOT NULL, plugin_id TEXT NOT NULL, enabled INTEGER DEFAULT 1,
            PRIMARY KEY (bot_id, plugin_id)
        )"#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS plugin_credentials (
            plugin_id TEXT PRIMARY KEY, header_name TEXT NOT NULL DEFAULT 'Authorization',
            header_value TEXT NOT NULL, updated_at TEXT NOT NULL
        )"#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn upsert_plugin(
        &self,
        id: &str,
        name: &str,
        description: &str,
        logo: Option<&str>,
        spec: &str,
    ) -> Result<(), String> {
        self.upsert_plugin_full(id, name, description, logo, None, spec, false, None)
            .await
    }

    /// Insert or update an installed spec without clobbering its global scope
    /// or its original install time.
    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_plugin_full(
        &self,
        id: &str,
        name: &str,
        description: &str,
        logo: Option<&str>,
        source_url: Option<&str>,
        spec: &str,
        requires_auth: bool,
        credential_header_hint: Option<&str>,
    ) -> Result<(), String> {
        sqlx::query(
            r#"INSERT INTO plugins
                 (id, name, description, logo, manifest_url, openapi_spec, enabled, requires_auth, credential_header_hint, created_at)
               VALUES (?, ?, ?, ?, ?, ?, 1, ?, ?, ?)
               ON CONFLICT(id) DO UPDATE SET
                 name = excluded.name,
                 description = excluded.description,
                 logo = excluded.logo,
                 manifest_url = excluded.manifest_url,
                 openapi_spec = excluded.openapi_spec,
                 requires_auth = excluded.requires_auth,
                 credential_header_hint = excluded.credential_header_hint,
                 enabled = 1"#,
        )
        .bind(id)
        .bind(name)
        .bind(description)
        .bind(logo.unwrap_or(""))
        .bind(source_url)
        .bind(spec)
        .bind(if requires_auth { 1 } else { 0 })
        .bind(credential_header_hint)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn set_metadata(
        &self,
        id: &str,
        name: &str,
        description: &str,
        logo: Option<&str>,
        requires_auth: bool,
        credential_header_hint: Option<&str>,
    ) -> Result<(), String> {
        sqlx::query(
            // Never downgrade a known-auth app to public during Verify: several
            // real specs (GitHub's, for one) omit their security scheme.
            "UPDATE plugins SET name = ?, description = ?, logo = COALESCE(?, logo), requires_auth = MAX(requires_auth, ?), credential_header_hint = COALESCE(?, credential_header_hint) WHERE id = ?",
        )
        .bind(name)
        .bind(description)
        .bind(logo)
        .bind(if requires_auth { 1 } else { 0 })
        .bind(credential_header_hint)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn list_summaries(&self, query: Option<&str>) -> Result<Vec<PluginSummary>, String> {
        type Row = (
            String,
            String,
            String,
            String,
            i64,
            i64,
            i64,
            i64,
            Option<String>,
            Option<String>,
            Option<String>,
        );
        let rows: Vec<Row> = if let Some(q) = query.map(str::trim).filter(|q| !q.is_empty()) {
            let pattern = format!("%{}%", q);
            sqlx::query_as(
                r#"SELECT p.id, p.name, p.description, COALESCE(p.logo, ''), p.enabled_global,
                          (SELECT COUNT(*) FROM openapi_operations o WHERE o.plugin_id = p.id),
                          (SELECT COUNT(*) FROM plugin_credentials c WHERE c.plugin_id = p.id),
                          p.requires_auth, p.manifest_url, p.base_url_override,
                          p.credential_header_hint
                   FROM plugins p
                   WHERE p.id LIKE ? OR p.name LIKE ? OR p.description LIKE ?
                   ORDER BY LOWER(p.name) LIMIT 200"#,
            )
            .bind(&pattern)
            .bind(&pattern)
            .bind(&pattern)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| e.to_string())?
        } else {
            sqlx::query_as(
                r#"SELECT p.id, p.name, p.description, COALESCE(p.logo, ''), p.enabled_global,
                          (SELECT COUNT(*) FROM openapi_operations o WHERE o.plugin_id = p.id),
                          (SELECT COUNT(*) FROM plugin_credentials c WHERE c.plugin_id = p.id),
                          p.requires_auth, p.manifest_url, p.base_url_override,
                          p.credential_header_hint
                   FROM plugins p
                   ORDER BY LOWER(p.name) LIMIT 200"#,
            )
            .fetch_all(&self.pool)
            .await
            .map_err(|e| e.to_string())?
        };
        let mut summaries: Vec<PluginSummary> = rows
            .into_iter()
            .map(
                |(
                    id,
                    name,
                    description,
                    logo,
                    enabled_global,
                    operations,
                    has_credential,
                    requires_auth,
                    source_url,
                    base_url_override,
                    credential_header_hint,
                )| {
                    PluginSummary {
                        id,
                        name,
                        description,
                        logo,
                        enabled_global: enabled_global != 0,
                        operations,
                        has_credential: has_credential != 0,
                        requires_auth: requires_auth != 0,
                        source_url,
                        base_url_override,
                        credential_header_hint,
                    }
                },
            )
            .collect();
        // Curated specs whose documents omit security (GitHub's does) still
        // have a known requirement. Apply it to existing installs without
        // rewriting the row.
        for summary in &mut summaries {
            if let Some((requires_auth, header)) =
                crate::catalog::curated_auth_for_source(summary.source_url.as_deref())
            {
                summary.requires_auth = requires_auth;
                if requires_auth && summary.credential_header_hint.is_none() {
                    summary.credential_header_hint = Some(header.to_string());
                }
            }
        }
        Ok(summaries)
    }

    pub async fn summary(&self, plugin_id: &str) -> Result<Option<PluginSummary>, String> {
        Ok(self
            .list_summaries(Some(plugin_id))
            .await?
            .into_iter()
            .find(|p| p.id == plugin_id))
    }

    pub async fn details(&self, plugin_id: &str) -> Result<Option<PluginDetails>, String> {
        let Some(summary) = self.summary(plugin_id).await? else {
            return Ok(None);
        };
        let rows: Vec<(String, String, String, String, String)> = sqlx::query_as(
            "SELECT operation_id, method, path_template, description, input_schema
             FROM openapi_operations WHERE plugin_id = ?
             ORDER BY operation_id LIMIT 100",
        )
        .bind(plugin_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        let operations = rows
            .into_iter()
            .map(|(id, method, path, description, schema)| PluginOperation {
                id,
                method: method.to_uppercase(),
                path,
                description,
                input_schema: serde_json::from_str(&schema)
                    .unwrap_or_else(|_| serde_json::json!({"type":"object"})),
            })
            .collect();
        let credential_header: Option<String> =
            sqlx::query_scalar("SELECT header_name FROM plugin_credentials WHERE plugin_id = ?")
                .bind(plugin_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| e.to_string())?;
        Ok(Some(PluginDetails {
            summary,
            operations,
            credential_header,
        }))
    }

    pub async fn set_bot_plugin(
        &self,
        bot_id: Uuid,
        plugin_id: &str,
        enabled: bool,
    ) -> Result<(), String> {
        if enabled {
            sqlx::query(
                "INSERT OR REPLACE INTO bot_plugins (bot_id, plugin_id, enabled) VALUES (?, ?, 1)",
            )
            .bind(bot_id.to_string())
            .bind(plugin_id)
            .execute(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
        } else {
            sqlx::query("DELETE FROM bot_plugins WHERE bot_id = ? AND plugin_id = ?")
                .bind(bot_id.to_string())
                .bind(plugin_id)
                .execute(&self.pool)
                .await
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub async fn list_bot_plugins(&self, bot_id: Uuid) -> Result<Vec<String>, String> {
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT plugin_id FROM bot_plugins WHERE bot_id = ? AND enabled=1")
                .bind(bot_id.to_string())
                .fetch_all(&self.pool)
                .await
                .map_err(|e| e.to_string())?;
        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    pub async fn list_enabled_for_bot(
        &self,
        bot_id: Uuid,
    ) -> Result<Vec<(String, String, String, String)>, String> {
        let rows: Vec<(String, String, String, String)> = sqlx::query_as(
            r#"SELECT p.id, p.name, p.description, p.logo FROM plugins p
               JOIN bot_plugins bp ON p.id = bp.plugin_id WHERE bp.bot_id = ? AND bp.enabled=1"#,
        )
        .bind(bot_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        if !rows.is_empty() {
            return Ok(rows);
        }
        // MCP-style fallback: a bot with no enabled plugin assignments sees
        // every globally-enabled plugin. This mirrors skills_for_bot in the
        // MCP registry so "enable once, use everywhere" behaves identically.
        self.list_global_plugins().await
    }

    /// Toggle a plugin's global scope (available to every bot with no
    /// per-bot assignments).
    pub async fn set_plugin_global(&self, plugin_id: &str, enabled: bool) -> Result<(), String> {
        sqlx::query("UPDATE plugins SET enabled_global = ? WHERE id = ?")
            .bind(if enabled { 1 } else { 0 })
            .bind(plugin_id)
            .execute(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Apps this bot can actually use: enabled, and with a credential when
    /// the app requires one. An auth-required app without a key is visible in
    /// the picker but must not become a tool that only returns 401.
    pub async fn list_usable_for_bot(
        &self,
        bot_id: Uuid,
    ) -> Result<Vec<(String, String, String, String)>, String> {
        let enabled = self.list_enabled_for_bot(bot_id).await?;
        let mut usable = Vec::new();
        for (id, name, description, logo) in enabled {
            let credential = self.get_credential(&id).await?;
            if let Some(summary) = self.summary(&id).await? {
                if summary.requires_auth && credential.is_none() {
                    continue;
                }
            }
            usable.push((id, name, description, logo));
        }
        Ok(usable)
    }

    /// Plugins enabled for every bot (global scope).
    pub async fn list_global_plugins(
        &self,
    ) -> Result<Vec<(String, String, String, String)>, String> {
        let rows: Vec<(String, String, String, String)> = sqlx::query_as(
            r#"SELECT id, name, description, logo FROM plugins
               WHERE enabled = 1 AND enabled_global = 1 ORDER BY name"#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        Ok(rows)
    }

    pub async fn set_credential(
        &self,
        plugin_id: &str,
        header_name: &str,
        header_value: &str,
    ) -> Result<(), String> {
        let header_name = if header_name.trim().is_empty() {
            "Authorization"
        } else {
            header_name.trim()
        };
        let header_value = header_value.trim();
        if header_value.is_empty() {
            return Err("The credential value cannot be empty.".to_string());
        }
        reqwest::header::HeaderName::from_bytes(header_name.as_bytes())
            .map_err(|_| "That is not a valid HTTP header name.".to_string())?;
        reqwest::header::HeaderValue::from_str(header_value)
            .map_err(|_| "That is not a valid HTTP header value.".to_string())?;
        sqlx::query(
            r#"INSERT INTO plugin_credentials (plugin_id, header_name, header_value, updated_at)
               VALUES (?, ?, ?, ?)
               ON CONFLICT(plugin_id) DO UPDATE SET
                 header_name = excluded.header_name,
                 header_value = excluded.header_value,
                 updated_at = excluded.updated_at"#,
        )
        .bind(plugin_id)
        .bind(header_name)
        .bind(header_value)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn clear_credential(&self, plugin_id: &str) -> Result<(), String> {
        sqlx::query("DELETE FROM plugin_credentials WHERE plugin_id = ?")
            .bind(plugin_id)
            .execute(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn set_base_url(
        &self,
        plugin_id: &str,
        base_url: Option<&str>,
    ) -> Result<(), String> {
        let value = base_url.map(str::trim).filter(|value| !value.is_empty());
        if let Some(value) = value {
            if !(value.starts_with("https://") || value.starts_with("http://")) {
                return Err("The server URL must start with http:// or https://.".to_string());
            }
        }
        sqlx::query("UPDATE plugins SET base_url_override = ? WHERE id = ?")
            .bind(value)
            .bind(plugin_id)
            .execute(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn get_credential(
        &self,
        plugin_id: &str,
    ) -> Result<Option<PluginCredential>, String> {
        let row: Option<(String, String)> = sqlx::query_as(
            "SELECT header_name, header_value FROM plugin_credentials WHERE plugin_id = ?",
        )
        .bind(plugin_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        Ok(row.map(|(header_name, header_value)| PluginCredential {
            header_name,
            header_value,
        }))
    }

    pub async fn search_operations(
        &self,
        query: &str,
        limit: i64,
    ) -> Result<Vec<OperationMatch>, String> {
        let pattern = format!("%{}%", query.trim());
        let rows: Vec<(String, String, String, String, String, String)> = sqlx::query_as(
            r#"SELECT o.plugin_id, p.name, o.operation_id, o.description, o.method, o.path_template
               FROM openapi_operations o JOIN plugins p ON p.id = o.plugin_id
               WHERE o.operation_id LIKE ? OR o.description LIKE ? OR p.name LIKE ?
               ORDER BY LOWER(p.name), o.operation_id LIMIT ?"#,
        )
        .bind(&pattern)
        .bind(&pattern)
        .bind(&pattern)
        .bind(limit.clamp(1, 50))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        Ok(rows
            .into_iter()
            .map(
                |(plugin_id, plugin_name, operation_id, description, method, path)| {
                    OperationMatch {
                        plugin_id: plugin_id.clone(),
                        plugin_name,
                        operation_id: crate::openapi::normalize_operation_id(
                            &plugin_id,
                            &operation_id,
                        ),
                        description,
                        method: method.to_uppercase(),
                        path,
                    }
                },
            )
            .collect())
    }

    /// Which plugin owns an operation id, including legacy unqualified rows.
    pub async fn plugin_for_operation(&self, operation_id: &str) -> Result<Option<String>, String> {
        let exact: Option<String> = sqlx::query_scalar(
            "SELECT plugin_id FROM openapi_operations WHERE operation_id = ? LIMIT 1",
        )
        .bind(operation_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        if exact.is_some() {
            return Ok(exact);
        }
        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT plugin_id, operation_id FROM openapi_operations")
                .fetch_all(&self.pool)
                .await
                .map_err(|e| e.to_string())?;
        Ok(rows
            .into_iter()
            .find(|(plugin_id, operation_id)| {
                crate::openapi::normalize_operation_id(plugin_id, operation_id)
                    == operation_id.as_str()
            })
            .map(|(plugin_id, _)| plugin_id))
    }

    /// Resolve an operation to a real executable skill plus its credential.
    pub async fn operation_for_skill(
        &self,
        operation_id: &str,
    ) -> Result<Option<(crate::openapi::OpenApiSkill, Option<PluginCredential>)>, String> {
        let exact: Option<String> = sqlx::query_scalar(
            "SELECT plugin_id FROM openapi_operations WHERE operation_id = ? LIMIT 1",
        )
        .bind(operation_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        let plugin_id = if let Some(plugin_id) = exact {
            plugin_id
        } else {
            // Legacy rows may still hold an unqualified id. Compare through
            // the same normalization the skill loader uses.
            let rows: Vec<(String, String)> =
                sqlx::query_as("SELECT plugin_id, operation_id FROM openapi_operations")
                    .fetch_all(&self.pool)
                    .await
                    .map_err(|e| e.to_string())?;
            match rows.into_iter().find(|(plugin_id, operation_id)| {
                crate::openapi::normalize_operation_id(plugin_id, operation_id)
                    == operation_id.as_str()
            }) {
                Some((plugin_id, _)) => plugin_id,
                None => return Ok(None),
            }
        };
        let operations = crate::openapi::load_operations(&self.pool, &plugin_id).await?;
        let Some(operation) = operations.into_iter().find(|op| op.id == operation_id) else {
            return Ok(None);
        };
        let credential = self.get_credential(&plugin_id).await?;
        Ok(Some((operation, credential)))
    }

    /// Remove legacy placeholder rows: a short stub spec with no parsed
    /// operations cannot run, so keeping it only produces fake cards.
    pub async fn remove_invalid_plugins(&self) -> Result<usize, String> {
        let mut tx = self.pool.begin().await.map_err(|e| e.to_string())?;
        let stale_ids: Vec<(String,)> = sqlx::query_as(
            r#"SELECT id FROM plugins
               WHERE COALESCE(length(openapi_spec), 0) < 200
                 AND id NOT IN (SELECT plugin_id FROM openapi_operations)"#,
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        for (id,) in &stale_ids {
            sqlx::query("DELETE FROM bot_plugins WHERE plugin_id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
            sqlx::query("DELETE FROM plugins WHERE id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
        }
        tx.commit().await.map_err(|e| e.to_string())?;
        Ok(stale_ids.len())
    }

    /// Every installed spec, for the verify pass.
    pub async fn all_specs(&self) -> Result<Vec<(String, String)>, String> {
        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT id, COALESCE(openapi_spec, '') FROM plugins ORDER BY id")
                .fetch_all(&self.pool)
                .await
                .map_err(|e| e.to_string())?;
        Ok(rows)
    }
}

#[cfg(test)]
mod scope_tests {
    use super::*;

    async fn temp_store() -> PluginStore {
        let path =
            std::env::temp_dir().join(format!("ravenbot-plugins-test-{}.db", Uuid::new_v4()));
        let db = ravenbot_db::Database::new(&path).await.expect("temp db");
        let store = PluginStore::new(db.pool().clone());
        store.ensure_tables().await.expect("ensure tables");
        store
    }

    #[tokio::test]
    async fn global_plugin_falls_back_for_unassigned_bots() {
        let store = temp_store().await;
        store
            .upsert_plugin("gmail", "Gmail", "Email", None, "{}")
            .await
            .unwrap();
        store
            .upsert_plugin("jira", "Jira", "Issues", None, "{}")
            .await
            .unwrap();
        store.set_plugin_global("gmail", true).await.unwrap();

        // A bot with zero assignments sees globally-enabled plugins.
        let seen = store.list_enabled_for_bot(Uuid::new_v4()).await.unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].0, "gmail");

        // A bot with its own assignment ignores the global set (MCP parity).
        let bot = ravenbot_core::Bot::new("PluginScopeBot", "t");
        ravenbot_db::queries::BotQueries::insert(store.pool(), &bot)
            .await
            .unwrap();
        store.set_bot_plugin(bot.id, "jira", true).await.unwrap();
        let seen = store.list_enabled_for_bot(bot.id).await.unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].0, "jira");

        // Turning global off hides it from unassigned bots again.
        store.set_plugin_global("gmail", false).await.unwrap();
        assert!(store
            .list_enabled_for_bot(Uuid::new_v4())
            .await
            .unwrap()
            .is_empty());
        assert!(store.list_global_plugins().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn summaries_count_real_operations_and_credentials() {
        let store = temp_store().await;
        let spec = r#"{"openapi":"3.0.0","info":{"title":"Petstore","description":"Pets","x-logo":{"url":"https://x/logo.png"}},"servers":[{"url":"https://api.petstore.test"}],"paths":{"/pets":{"get":{"operationId":"listPets","summary":"List pets"}}}}"#;
        let ops = crate::openapi::parse_openapi_spec(spec).unwrap();
        store
            .upsert_plugin_full(
                "petstore",
                "Petstore",
                "Pets",
                Some("https://x/logo.png"),
                Some("https://x/openapi.json"),
                spec,
                false,
                Some("X-API-Key"),
            )
            .await
            .unwrap();
        crate::openapi::store_operations(store.pool(), "petstore", &ops)
            .await
            .unwrap();

        let summary = store.summary("petstore").await.unwrap().unwrap();
        assert_eq!(summary.operations, 1);
        assert!(!summary.has_credential);
        assert_eq!(summary.credential_header_hint.as_deref(), Some("X-API-Key"));
        assert_eq!(
            summary.source_url.as_deref(),
            Some("https://x/openapi.json")
        );

        store
            .set_credential("petstore", "Authorization", "Bearer test")
            .await
            .unwrap();
        let details = store.details("petstore").await.unwrap().unwrap();
        assert!(details.summary.has_credential);
        assert_eq!(details.credential_header.as_deref(), Some("Authorization"));
        assert_eq!(details.operations.len(), 1);
        assert_eq!(details.operations[0].method, "GET");

        // A tenant-specific server overrides the spec's placeholder base.
        store
            .set_base_url("petstore", Some("https://tenant.example/api"))
            .await
            .unwrap();
        let loaded = crate::openapi::load_operations(store.pool(), "petstore")
            .await
            .unwrap();
        assert_eq!(loaded[0].server_base, "https://tenant.example/api");
        assert!(store
            .set_base_url("petstore", Some("ftp://bad"))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn curated_sources_get_known_auth_for_old_installs() {
        let store = temp_store().await;
        store
            .upsert_plugin_full(
                "github",
                "GitHub",
                "Repositories",
                None,
                Some("https://raw.githubusercontent.com/github/rest-api-description/main/descriptions/api.github.com/api.github.com.json"),
                "{}",
                false,
                None,
            )
            .await
            .unwrap();

        let summary = store.summary("github").await.unwrap().unwrap();
        assert!(summary.requires_auth);
        assert_eq!(
            summary.credential_header_hint.as_deref(),
            Some("Authorization")
        );
    }

    #[tokio::test]
    async fn verify_never_forgets_known_auth() {
        let store = temp_store().await;
        store
            .upsert_plugin_full(
                "github",
                "GitHub",
                "Repositories",
                None,
                Some("https://github.test/openapi.json"),
                "{}",
                true,
                Some("Authorization"),
            )
            .await
            .unwrap();

        // GitHub's public description omits security schemes. Verify must not
        // turn a known-auth app into a public one.
        store
            .set_metadata("github", "GitHub", "Repositories", None, false, None)
            .await
            .unwrap();
        let summary = store.summary("github").await.unwrap().unwrap();
        assert!(summary.requires_auth);
        assert_eq!(
            summary.credential_header_hint.as_deref(),
            Some("Authorization")
        );
    }

    #[tokio::test]
    async fn placeholder_plugins_are_removed() {
        let store = temp_store().await;
        store
            .upsert_plugin("fake", "Fake", "stub", None, "{}")
            .await
            .unwrap();
        assert_eq!(store.remove_invalid_plugins().await.unwrap(), 1);
        assert!(store.summary("fake").await.unwrap().is_none());
    }
}
