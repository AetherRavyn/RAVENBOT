//! DB Query — local SQLite (offline) + Postgres via Composio fallback

use async_trait::async_trait;
use ravenbot_core::Permission;
use crate::traits::{Skill, SkillContext, SkillError, SkillResult};

pub struct DbQuerySkill;

impl DbQuerySkill { pub fn new() -> Self { Self } }

#[async_trait]
impl Skill for DbQuerySkill {
    fn id(&self) -> &str { "db_query" }
    fn name(&self) -> &str { "Database Query" }
    fn description(&self) -> &str { "Query local SQLite (ravenbot.db) — offline, no OAuth" }
    fn version(&self) -> &str { "1.0.0" }
    fn required_permissions(&self) -> Vec<Permission> { vec![Permission::FileSystem { paths: vec![".".into()] }] }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type":"object","properties":{
                "sql":{"type":"string","description":"SELECT only (e.g. SELECT * FROM bots LIMIT 5)"},
                "path":{"type":"string","description":"DB path, default ravenbot.db"}
            },"required":["sql"]
        })
    }
    async fn execute(&self, _ctx: &SkillContext, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let sql = args.get("sql").and_then(|v| v.as_str()).ok_or_else(|| SkillError::InvalidArguments("Missing sql".into()))?;
        let lower = sql.trim().to_lowercase();
        if !lower.starts_with("select") && !lower.starts_with("pragma") && !lower.starts_with("explain") {
            return Ok(SkillResult::failure("Only SELECT/PRAGMA allowed for safety — use Composio supabase_query for writes"));
        }
        // For personal perfect, we query via sqlite CLI (offline) — no new deps
        let out = tokio::process::Command::new("sh").args(["-c", &format!("sqlite3 ~/.local/share/com.ravenbot.desktop/ravenbot.db \"{}\" 2>&1 | head -n 100", sql.replace('"', "'").replace('\'', "'\\''"))]).output().await.map_err(|e| SkillError::Io(e.to_string()))?;
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        Ok(SkillResult::success(serde_json::json!({"sql":sql,"rows": stdout.lines().collect::<Vec<_>>(), "stderr": stderr, "success": out.status.success()})))
    }
}
impl Default for DbQuerySkill { fn default() -> Self { Self::new() } }
