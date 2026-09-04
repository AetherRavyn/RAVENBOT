//! Shell execution skill

use async_trait::async_trait;
use ravenbot_core::Permission;
use tokio::process::Command;

use crate::traits::{Skill, SkillContext, SkillError, SkillResult};

pub struct ShellExecSkill;

impl ShellExecSkill {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Skill for ShellExecSkill {
    fn id(&self) -> &str {
        "shell_exec"
    }

    fn name(&self) -> &str {
        "Shell Execute"
    }

    fn description(&self) -> &str {
        "Execute a shell command"
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn required_permissions(&self) -> Vec<Permission> {
        vec![Permission::Shell]
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute"
                },
                "cwd": {
                    "type": "string",
                    "description": "Working directory (optional)"
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in seconds (default: 30, max: 300)"
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(
        &self,
        _context: &SkillContext,
        arguments: serde_json::Value,
    ) -> Result<SkillResult, SkillError> {
        let command = arguments
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'command' field".to_string()))?;

        let cwd = arguments.get("cwd").and_then(|v| v.as_str());

        let timeout = arguments
            .get("timeout")
            .and_then(|v| v.as_u64())
            .unwrap_or(30)
            .min(300);

        // Create command with proper lifetime handling
        let mut cmd = create_command(command);
        
        // Set working directory if provided
        if let Some(cwd) = cwd {
            cmd.current_dir(cwd);
        }

        // Execute with timeout
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(timeout),
            cmd.output(),
        )
        .await
        .map_err(|_| SkillError::Execution(format!("Command timed out after {}s", timeout)))?
        .map_err(|e| SkillError::Execution(e.to_string()))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        // Truncate large outputs
        let max_output = 100_000;
        let truncated_stdout = if stdout.len() > max_output {
            format!("{}...(truncated)", &stdout[..max_output])
        } else {
            stdout.clone()
        };
        let truncated_stderr = if stderr.len() > max_output {
            format!("{}...(truncated)", &stderr[..max_output])
        } else {
            stderr.clone()
        };

        let success = output.status.success();

        Ok(SkillResult::success(serde_json::json!({
            "command": command,
            "exit_code": output.status.code(),
            "stdout": truncated_stdout,
            "stderr": truncated_stderr,
            "success": success,
            "truncated": stdout.len() > max_output || stderr.len() > max_output
        })))
    }
}

fn create_command(command: &str) -> Command {
    if cfg!(target_os = "windows") {
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", command]);
        cmd
    } else {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", command]);
        cmd
    }
}

impl Default for ShellExecSkill {
    fn default() -> Self {
        Self::new()
    }
}
