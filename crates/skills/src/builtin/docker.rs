use async_trait::async_trait;
use ravenbot_core::Permission;
use crate::traits::{Skill, SkillContext, SkillError, SkillResult};

pub struct DockerSkill;

impl DockerSkill { pub fn new() -> Self { Self } }

#[async_trait]
impl Skill for DockerSkill {
    fn id(&self) -> &str { "docker" }
    fn name(&self) -> &str { "Docker" }
    fn description(&self) -> &str { "Docker operations: ps, images, build, run — local computer" }
    fn version(&self) -> &str { "1.0.0" }
    fn required_permissions(&self) -> Vec<Permission> { vec![Permission::Shell] }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({"type":"object","properties":{
            "action":{"type":"string","enum":["ps","images","build","run","logs"]},
            "args":{"type":"string","description":"Extra args e.g. 'myapp:latest .' for build"}
        },"required":["action"]})
    }
    async fn execute(&self, _ctx: &SkillContext, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("ps");
        let extra = args.get("args").and_then(|v| v.as_str()).unwrap_or("");
        let cmd = match action {
            "ps" => "docker ps --format 'table {{.Names}}\\t{{.Status}}\\t{{.Ports}}' 2>&1 | head -n 30",
            "images" => "docker images --format 'table {{.Repository}}:{{.Tag}}\\t{{.Size}}' 2>&1 | head -n 30",
            "build" => &format!("docker build {} 2>&1 | tail -n 50", extra),
            "run" => &format!("docker run --rm {} 2>&1 | head -n 50", extra),
            "logs" => &format!("docker logs {} 2>&1 | tail -n 100", extra),
            _ => "docker ps",
        };
        let out = tokio::process::Command::new("sh").args(["-c", cmd]).output().await.map_err(|e| SkillError::Io(e.to_string()))?;
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        Ok(SkillResult::success(serde_json::json!({"action": action, "stdout": stdout, "stderr": stderr, "success": out.status.success()})))
    }
}
impl Default for DockerSkill { fn default() -> Self { Self::new() } }
