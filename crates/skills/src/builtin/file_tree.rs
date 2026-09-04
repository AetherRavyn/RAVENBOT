use async_trait::async_trait;
use ravenbot_core::Permission;
use crate::traits::{Skill, SkillContext, SkillError, SkillResult};

pub struct FileTreeSkill;

impl FileTreeSkill { pub fn new() -> Self { Self } }

#[async_trait]
impl Skill for FileTreeSkill {
    fn id(&self) -> &str { "file_tree" }
    fn name(&self) -> &str { "File Tree" }
    fn description(&self) -> &str { "List directory tree, files and sizes — offline workspace explorer" }
    fn version(&self) -> &str { "1.0.0" }
    fn required_permissions(&self) -> Vec<Permission> { vec![Permission::FileSystem { paths: vec![".".into()] }] }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({"type":"object","properties":{"path":{"type":"string","description":"Root path, default ."},"depth":{"type":"integer","minimum":1,"maximum":5}},"required":[]})
    }
    async fn execute(&self, _ctx: &SkillContext, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let depth = args.get("depth").and_then(|v| v.as_u64()).unwrap_or(2) as usize;
        let out = tokio::process::Command::new("sh").args(["-c", &format!("find {} -maxdepth {} -type f -o -type d | head -n 200 2>&1", shell_escape(path), depth)]).output().await.map_err(|e| SkillError::Io(e.to_string()))?;
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        Ok(SkillResult::success(serde_json::json!({"path": path, "tree": stdout.lines().collect::<Vec<_>>()})))
    }
}
fn shell_escape(s: &str) -> String { format!("'{}'", s.replace('\'', "'\\''")) }
impl Default for FileTreeSkill { fn default() -> Self { Self::new() } }
