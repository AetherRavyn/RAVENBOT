//! DB Query — read-only SQL over the local RAVENBOT SQLite database.
//!
//! Opens the database **read-only** and returns structured JSON rows. The
//! previous implementation shelled out to the `sqlite3` CLI with hand-rolled,
//! broken quoting that both mangled legitimate queries and let a query such as
//! `SELECT 1; DROP TABLE bots` execute the second statement. A read-only
//! connection plus single-statement execution closes that hole.

use async_trait::async_trait;
use ravenbot_core::Permission;
use sqlx::sqlite::{SqliteConnectOptions, SqliteRow};
use sqlx::{Column, Row, SqlitePool};

use crate::traits::{Skill, SkillContext, SkillError, SkillResult, SkillRisk};

pub struct DbQuerySkill;

impl DbQuerySkill {
    pub fn new() -> Self {
        Self
    }
}

/// Only read-oriented statements are accepted.
fn is_read_only_statement(sql: &str) -> bool {
    let lower = sql.trim_start().to_lowercase();
    lower.starts_with("select")
        || lower.starts_with("pragma")
        || lower.starts_with("explain")
        || lower.starts_with("with")
}

/// Best-effort conversion of a SQLite cell to JSON by trying the common types.
fn cell_to_json(row: &SqliteRow, idx: usize) -> serde_json::Value {
    if let Ok(Some(v)) = row.try_get::<Option<i64>, _>(idx) {
        return serde_json::Value::from(v);
    }
    if let Ok(Some(v)) = row.try_get::<Option<f64>, _>(idx) {
        return serde_json::Value::from(v);
    }
    if let Ok(Some(v)) = row.try_get::<Option<String>, _>(idx) {
        return serde_json::Value::from(v);
    }
    serde_json::Value::Null
}

#[async_trait]
impl Skill for DbQuerySkill {
    fn id(&self) -> &str {
        "db_query"
    }
    fn name(&self) -> &str {
        "Database Query"
    }
    fn description(&self) -> &str {
        "Run a read-only SQL query against the local RAVENBOT SQLite database \
         (tables: bots, threads, messages, runs, routines, memories, …). \
         Returns structured rows as JSON."
    }
    fn version(&self) -> &str {
        "1.1.0"
    }
    fn required_permissions(&self) -> Vec<Permission> {
        vec![Permission::FileSystem { paths: vec![".".into()] }]
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type":"object","properties":{
                "sql":{"type":"string","description":"A single read-only statement (SELECT/PRAGMA/EXPLAIN/WITH)"},
                "path":{"type":"string","description":"Database path (default: the RAVENBOT app database)"},
                "limit":{"type":"integer","description":"Max rows returned (default 100, max 1000)"}
            },"required":["sql"]
        })
    }
    fn risk(&self) -> SkillRisk {
        SkillRisk::ReadOnly
    }

    async fn execute(
        &self,
        _ctx: &SkillContext,
        args: serde_json::Value,
    ) -> Result<SkillResult, SkillError> {
        let sql = args
            .get("sql")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing sql".into()))?
            .trim();
        if sql.is_empty() {
            return Err(SkillError::InvalidArguments("Empty sql".into()));
        }
        if !is_read_only_statement(sql) {
            return Ok(SkillResult::failure(
                "Only read-only statements are allowed (SELECT/PRAGMA/EXPLAIN/WITH).",
            ));
        }

        let limit = args
            .get("limit")
            .and_then(|v| v.as_i64())
            .unwrap_or(100)
            .clamp(1, 1000);

        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .filter(|p| !p.trim().is_empty())
            .map(std::path::PathBuf::from)
            .unwrap_or_else(ravenbot_core::default_db_path);

        if !path.exists() {
            return Ok(SkillResult::failure(format!(
                "Database not found at {}",
                path.display()
            )));
        }

        // Read-only connection: a write attempted through any means is refused
        // by SQLite itself, independent of the prefix check above.
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .read_only(true)
            .create_if_missing(false);

        let pool = SqlitePool::connect_with(options)
            .await
            .map_err(|e| SkillError::Execution(format!("Failed to open database: {}", e)))?;

        // `.query` executes a single statement; sqlx rejects multiple statements
        // in one call, so `SELECT 1; DROP TABLE x` cannot run the drop.
        let rows_result = sqlx::query(sql).fetch_all(&pool).await;
        pool.close().await;

        let rows = match rows_result {
            Ok(rows) => rows,
            Err(e) => {
                return Ok(SkillResult::failure(format!("Query failed: {}", e)));
            }
        };

        let truncated = rows.len() > limit as usize;
        let json_rows: Vec<serde_json::Value> = rows
            .iter()
            .take(limit as usize)
            .map(|row| {
                let mut obj = serde_json::Map::new();
                for (idx, col) in row.columns().iter().enumerate() {
                    obj.insert(col.name().to_string(), cell_to_json(row, idx));
                }
                serde_json::Value::Object(obj)
            })
            .collect();

        Ok(SkillResult::success(serde_json::json!({
            "sql": sql,
            "path": path.to_string_lossy(),
            "count": json_rows.len(),
            "truncated": truncated,
            "rows": json_rows,
        })))
    }
}

impl Default for DbQuerySkill {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn ctx() -> SkillContext {
        SkillContext::with_default_tier(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4())
    }

    async fn temp_db_with_data() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("ravenbot-dbq-{}.db", Uuid::new_v4()));
        let db = ravenbot_db::Database::new(&path).await.expect("temp db");
        let bot = ravenbot_core::Bot::new("QueryBot", "db test");
        ravenbot_db::queries::BotQueries::insert(db.pool(), &bot).await.unwrap();
        path
    }

    #[tokio::test]
    async fn selects_structured_rows() {
        let path = temp_db_with_data().await;
        let skill = DbQuerySkill::new();
        let result = skill
            .execute(
                &ctx(),
                serde_json::json!({ "sql": "SELECT name FROM bots LIMIT 5", "path": path.to_string_lossy() }),
            )
            .await
            .expect("execute");
        assert!(result.success);
        let rows = result.output.get("rows").and_then(|v| v.as_array()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get("name").and_then(|v| v.as_str()), Some("QueryBot"));
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn rejects_write_statements() {
        let skill = DbQuerySkill::new();
        for sql in ["DROP TABLE bots", "DELETE FROM bots", "UPDATE bots SET name='x'"] {
            let result = skill
                .execute(&ctx(), serde_json::json!({ "sql": sql }))
                .await
                .expect("execute");
            assert!(!result.success, "must reject: {sql}");
        }
    }

    #[tokio::test]
    async fn cannot_chain_a_write_after_a_select() {
        let path = temp_db_with_data().await;
        let skill = DbQuerySkill::new();
        // The classic bypass: passes the prefix check but would run the drop.
        let result = skill
            .execute(
                &ctx(),
                serde_json::json!({
                    "sql": "SELECT 1; DROP TABLE bots",
                    "path": path.to_string_lossy()
                }),
            )
            .await
            .expect("execute");
        // Either it is refused or it errors — the important thing is the table
        // still exists afterwards.
        assert!(result.success == false || result.output.get("rows").is_some());
        let check = skill
            .execute(
                &ctx(),
                serde_json::json!({ "sql": "SELECT COUNT(*) AS n FROM bots", "path": path.to_string_lossy() }),
            )
            .await
            .expect("execute");
        let n = check.output.pointer("/rows/0/n").and_then(|v| v.as_i64());
        assert_eq!(n, Some(1), "bots table must still exist: {:?}", check.output);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn prefix_classifier_is_conservative() {
        assert!(is_read_only_statement("SELECT 1"));
        assert!(is_read_only_statement("  with x as (select 1) select * from x"));
        assert!(is_read_only_statement("PRAGMA table_info(bots)"));
        assert!(!is_read_only_statement("INSERT INTO bots VALUES (1)"));
        assert!(!is_read_only_statement("drop table bots"));
    }
}
