use async_trait::async_trait;
use ravenbot_core::Permission;
use crate::traits::{Skill, SkillContext, SkillError, SkillResult};

pub struct CodeEditSkill;

impl CodeEditSkill { pub fn new() -> Self { Self } }

#[async_trait]
impl Skill for CodeEditSkill {
    fn id(&self) -> &str { "code_edit" }
    fn name(&self) -> &str { "Code Edit" }
    fn description(&self) -> &str { "Apply unified diff patch to files — reviewable, git-aware" }
    fn version(&self) -> &str { "1.0.0" }
    fn required_permissions(&self) -> Vec<Permission> { vec![Permission::FileSystem { paths: vec![".".into()] }, Permission::Shell] }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({"type":"object","properties":{"patch":{"type":"string","description":"Unified diff"},"dry_run":{"type":"boolean"}},"required":["patch"]})
    }
    async fn execute(&self, _ctx: &SkillContext, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let patch = args.get("patch").and_then(|v| v.as_str()).ok_or_else(|| SkillError::InvalidArguments("Missing patch".into()))?;
        let dry = args.get("dry_run").and_then(|v| v.as_bool()).unwrap_or(false);
        let tmp = format!("/tmp/raven_patch_{}.diff", uuid::Uuid::new_v4());
        tokio::fs::write(&tmp, patch).await.map_err(|e| SkillError::Io(e.to_string()))?;
        let check = tokio::process::Command::new("sh").args(["-c", &format!("patch --dry-run -p1 < {} 2>&1 | head -n 100", shell_escape(&tmp))]).output().await.map_err(|e| SkillError::Io(e.to_string()))?;
        let check_out = String::from_utf8_lossy(&check.stdout).to_string() + &String::from_utf8_lossy(&check.stderr).to_string();
        if dry { let _ = tokio::fs::remove_file(&tmp).await; return Ok(SkillResult::success(serde_json::json!({"dry_run": true, "check": check_out}))); }
        if !check.status.success() { let _ = tokio::fs::remove_file(&tmp).await; return Ok(SkillResult::failure(format!("Patch check failed: {}", check_out))); }
        let apply = tokio::process::Command::new("sh").args(["-c", &format!("patch -p1 < {} 2>&1 | head -n 100", shell_escape(&tmp))]).output().await.map_err(|e| SkillError::Io(e.to_string()))?;
        let out = String::from_utf8_lossy(&apply.stdout).to_string() + &String::from_utf8_lossy(&apply.stderr).to_string();
        let _ = tokio::fs::remove_file(&tmp).await;
        Ok(SkillResult::success(serde_json::json!({"applied": apply.status.success(), "output": out, "check": check_out})))
    }
}
fn shell_escape(s: &str) -> String { format!("'{}'", s.replace('\'', "'\\''")) }
impl Default for CodeEditSkill { fn default() -> Self { Self::new() } }
