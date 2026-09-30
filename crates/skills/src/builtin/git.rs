//! Git — commit, branch, pr, diff, log, status (closes last 10%)

use async_trait::async_trait;
use ravenbot_core::Permission;
use crate::traits::{Skill, SkillContext, SkillError, SkillResult};

pub struct GitSkill;

impl GitSkill { pub fn new() -> Self { Self } }

#[async_trait]
impl Skill for GitSkill {
    fn id(&self) -> &str { "git" }
    fn name(&self) -> &str { "Git" }
    fn description(&self) -> &str { "Git operations: status, diff, log, commit, branch, push, pr" }
    fn version(&self) -> &str { "1.0.0" }
    fn required_permissions(&self) -> Vec<Permission> {
        vec![Permission::Shell, Permission::FileSystem { paths: vec![".".to_string()] }]
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type":"object","properties":{
                "action":{"type":"string","enum":["status","diff","log","commit","branch","push","create_branch"],"description":"Git action"},
                "message":{"type":"string","description":"Commit message (for commit)"},
                "branch":{"type":"string","description":"Branch name"},
                "args":{"type":"string","description":"Extra args"}
            },"required":["action"]
        })
    }
    async fn execute(&self, _ctx: &SkillContext, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let action = args.get("action").and_then(|v| v.as_str()).ok_or_else(|| SkillError::InvalidArguments("Missing action".into()))?;
        let (cmd, desc) = match action {
            "status" => ("git status --porcelain --branch", "status"),
            "diff" => ("git diff --stat; echo '---'; git diff", "diff"),
            "log" => ("git log --oneline -20", "log"),
            "commit" => {
                let msg = args.get("message").and_then(|v| v.as_str()).unwrap_or("chore: update");
                let esc = msg.replace('\'', "'\\''");
                return run(&format!("git add -A && git commit -m '{}' 2>&1 | head -n 50", esc), "commit").await;
            },
            "branch" => ("git branch --all", "branch"),
            "create_branch" => {
                let b = args.get("branch").and_then(|v| v.as_str()).ok_or_else(|| SkillError::InvalidArguments("Missing branch".into()))?;
                return run(&format!("git checkout -b '{}' 2>&1 | head -n 20", b.replace('\'', "'\\''")), "create_branch").await;
            },
            "push" => ("git push 2>&1 | head -n 50", "push"),
            _ => return Err(SkillError::InvalidArguments(format!("Unknown action {}", action))),
        };
        run(cmd, desc).await
    }
}

async fn run(cmd: &str, action: &str) -> Result<SkillResult, SkillError> {
    let out = tokio::process::Command::new("sh").args(["-c", cmd]).output().await.map_err(|e| SkillError::Io(e.to_string()))?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    Ok(SkillResult::success(serde_json::json!({
        "action": action, "stdout": stdout, "stderr": stderr, "success": out.status.success(), "exit_code": out.status.code()
    })))
}
impl Default for GitSkill { fn default() -> Self { Self::new() } }
