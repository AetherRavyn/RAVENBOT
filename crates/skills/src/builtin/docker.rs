use crate::traits::{Skill, SkillContext, SkillError, SkillResult};
use async_trait::async_trait;
use ravenbot_core::Permission;

pub struct DockerSkill;

impl DockerSkill {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Skill for DockerSkill {
    fn id(&self) -> &str {
        "docker"
    }
    fn name(&self) -> &str {
        "Docker"
    }
    fn description(&self) -> &str {
        "Docker operations: ps, images, build, run — local computer"
    }
    fn version(&self) -> &str {
        "1.0.0"
    }
    fn required_permissions(&self) -> Vec<Permission> {
        vec![Permission::Shell]
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({"type":"object","properties":{
            "action":{"type":"string","enum":["ps","images","build","run","logs"]},
            "args":{"type":"string","description":"Extra args e.g. 'myapp:latest .' for build"}
        },"required":["action"]})
    }
    async fn execute(
        &self,
        ctx: &SkillContext,
        args: serde_json::Value,
    ) -> Result<SkillResult, SkillError> {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("ps");
        let extra = args.get("args").and_then(|v| v.as_str()).unwrap_or("");

        // Docker can escape the workspace with a privileged container or a host
        // mount. Office agents are autonomous, so these flags are refused
        // outright rather than sent to an approval gate that will not exist.
        let lower = extra.to_ascii_lowercase();
        if lower.contains("--privileged")
            || lower.contains("--pid=host")
            || lower.contains("/var/run/docker.sock")
        {
            return Ok(SkillResult::failure(
                "Blocked: privileged or host-control docker flags are not allowed.",
            ));
        }
        let words: Vec<&str> = extra.split_whitespace().collect();
        for (i, word) in words.iter().enumerate() {
            let spec = if *word == "-v" || *word == "--volume" {
                words.get(i + 1).copied()
            } else if let Some(rest) = word.strip_prefix("--volume=") {
                Some(rest)
            } else if let Some(rest) = word.strip_prefix("-v") {
                Some(rest)
            } else {
                None
            };
            if let Some(spec) = spec {
                let src = spec.split(':').next().unwrap_or("").trim();
                if src.starts_with('/') {
                    let src = std::path::Path::new(src);
                    let inside = ctx.working_dirs.iter().any(|root| src.starts_with(root));
                    if !inside {
                        return Ok(SkillResult::failure(
                            "Blocked: docker volume mounts outside the office workspace are not allowed.",
                        ));
                    }
                }
            }
        }
        if let Some(pos) = lower.find("source=") {
            let src = lower[pos + 7..]
                .split(',')
                .next()
                .unwrap_or("")
                .trim()
                .trim_matches('"');
            if src.starts_with('/') {
                let inside = ctx
                    .working_dirs
                    .iter()
                    .any(|root| std::path::Path::new(src).starts_with(root));
                if !inside {
                    return Ok(SkillResult::failure(
                        "Blocked: docker mounts outside the office workspace are not allowed.",
                    ));
                }
            }
        }

        let cmd = match action {
            "ps" => "docker ps --format 'table {{.Names}}\\t{{.Status}}\\t{{.Ports}}' 2>&1 | head -n 30",
            "images" => "docker images --format 'table {{.Repository}}:{{.Tag}}\\t{{.Size}}' 2>&1 | head -n 30",
            "build" => &format!("docker build {} 2>&1 | tail -n 50", extra),
            "run" => &format!("docker run --rm {} 2>&1 | head -n 50", extra),
            "logs" => &format!("docker logs {} 2>&1 | tail -n 100", extra),
            _ => "docker ps",
        };
        if let Some(reason) = crate::safety::dangerous_command_reason(cmd, &ctx.working_dirs) {
            return Ok(SkillResult::failure(format!(
                "Blocked by RAVENBOT's safety denylist: {reason}. The docker command was not executed."
            )));
        }
        let out = tokio::process::Command::new("sh")
            .args(["-c", cmd])
            .output()
            .await
            .map_err(|e| SkillError::Io(e.to_string()))?;
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        Ok(SkillResult::success(
            serde_json::json!({"action": action, "stdout": stdout, "stderr": stderr, "success": out.status.success()}),
        ))
    }
}
impl Default for DockerSkill {
    fn default() -> Self {
        Self::new()
    }
}
