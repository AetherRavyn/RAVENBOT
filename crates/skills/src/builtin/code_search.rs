//! Code search — ripgrep-like semantic file search (local, offline)

use async_trait::async_trait;
use ravenbot_core::Permission;
use crate::traits::{Skill, SkillContext, SkillError, SkillResult};

pub struct CodeSearchSkill;

impl CodeSearchSkill { pub fn new() -> Self { Self } }

#[async_trait]
impl Skill for CodeSearchSkill {
    fn id(&self) -> &str { "code_search" }
    fn name(&self) -> &str { "Code Search" }
    fn description(&self) -> &str { "Search codebase for pattern (ripgrep-like), returns file:line matches" }
    fn version(&self) -> &str { "1.0.0" }
    fn required_permissions(&self) -> Vec<Permission> {
        vec![Permission::FileSystem { paths: vec![".".to_string()] }]
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type":"object","properties":{
                "pattern":{"type":"string","description":"Regex or text to search"},
                "path":{"type":"string","description":"Root path (default: .)"},
                "globs":{"type":"string","description":"Glob filter e.g. *.rs,*.ts"},
                "max_results":{"type":"integer","minimum":1,"maximum":100}
            },"required":["pattern"]
        })
    }
    async fn execute(&self, _ctx: &SkillContext, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let pattern = args.get("pattern").and_then(|v| v.as_str()).ok_or_else(|| SkillError::InvalidArguments("Missing pattern".into()))?;
        let root = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let globs = args.get("globs").and_then(|v| v.as_str()).unwrap_or("");
        let max = args.get("max_results").and_then(|v| v.as_u64()).unwrap_or(20) as usize;

        // Use shell grep -r for simplicity (offline, no extra deps)
        let mut cmd = tokio::process::Command::new("sh");
        let grep = if globs.is_empty() {
            format!("grep -rn -- -- {} {} 2>/dev/null | head -n {}", shell_escape(pattern), shell_escape(root), max)
        } else {
            format!("grep -rn --include='{}' -- -- {} {} 2>/dev/null | head -n {}", globs, shell_escape(pattern), shell_escape(root), max)
        };
        cmd.args(["-c", &grep]);
        let out = cmd.output().await.map_err(|e| SkillError::Io(e.to_string()))?;
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        let lines: Vec<_> = stdout.lines().map(|l| l.to_string()).collect();
        Ok(SkillResult::success(serde_json::json!({
            "pattern": pattern, "path": root, "matches": lines, "count": lines.len(), "stderr": stderr
        })))
    }
}

fn shell_escape(s: &str) -> String { format!("'{}'", s.replace('\'', "'\\''")) }
impl Default for CodeSearchSkill { fn default() -> Self { Self::new() } }
