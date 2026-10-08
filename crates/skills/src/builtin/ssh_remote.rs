//! SSH remote — execute commands on remote machines via SSH

use async_trait::async_trait;
use ravenbot_core::Permission;
use crate::traits::{Skill, SkillContext, SkillError, SkillResult};

pub struct SshRemoteSkill;

impl SshRemoteSkill { pub fn new() -> Self { Self } }

#[async_trait]
impl Skill for SshRemoteSkill {
    fn id(&self) -> &str { "ssh_remote" }
    fn name(&self) -> &str { "SSH Remote" }
    fn description(&self) -> &str { "Execute commands on remote machines via SSH, transfer files via SCP" }
    fn version(&self) -> &str { "1.0.0" }
    fn required_permissions(&self) -> Vec<Permission> {
        vec![Permission::Shell, Permission::Network { domains: vec!["*".to_string()] }]
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["exec", "copy_to", "copy_from", "tunnel", "test"],
                    "description": "SSH action to perform"
                },
                "host": {
                    "type": "string",
                    "description": "Remote host (user@hostname or hostname)"
                },
                "command": {
                    "type": "string",
                    "description": "Command to execute remotely (for exec)"
                },
                "port": {
                    "type": "integer",
                    "description": "SSH port (default: 22)"
                },
                "identity": {
                    "type": "string",
                    "description": "Path to SSH private key (optional)"
                },
                "source": {
                    "type": "string",
                    "description": "Source path (for copy)"
                },
                "destination": {
                    "type": "string",
                    "description": "Destination path (for copy)"
                },
                "local_port": {
                    "type": "integer",
                    "description": "Local port for tunnel"
                },
                "remote_port": {
                    "type": "integer",
                    "description": "Remote port for tunnel"
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in seconds (default: 30, max: 300)"
                }
            },
            "required": ["action", "host"]
        })
    }

    async fn execute(&self, _ctx: &SkillContext, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let action = args.get("action").and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'action'".into()))?;
        let host = args.get("host").and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'host'".into()))?;
        let port = args.get("port").and_then(|v| v.as_u64()).unwrap_or(22);
        let identity = args.get("identity").and_then(|v| v.as_str());
        let timeout = args.get("timeout").and_then(|v| v.as_u64()).unwrap_or(30).min(300);

        let mut ssh_opts = format!("-o ConnectTimeout={} -o StrictHostKeyChecking=accept-new -p {}", timeout, port);
        if let Some(key) = identity {
            ssh_opts.push_str(&format!(" -i {}", key));
        }

        let cmd = match action {
            "exec" => {
                let command = args.get("command").and_then(|v| v.as_str())
                    .ok_or_else(|| SkillError::InvalidArguments("Missing 'command' for exec".into()))?;
                format!("ssh {} {} '{}'", ssh_opts, host, command.replace('\'', "'\\''"))
            }
            "copy_to" => {
                let source = args.get("source").and_then(|v| v.as_str())
                    .ok_or_else(|| SkillError::InvalidArguments("Missing 'source'".into()))?;
                let dest = args.get("destination").and_then(|v| v.as_str())
                    .ok_or_else(|| SkillError::InvalidArguments("Missing 'destination'".into()))?;
                format!("scp -P {} -o StrictHostKeyChecking=accept-new {} {}:{}", port, source, host, dest)
            }
            "copy_from" => {
                let source = args.get("source").and_then(|v| v.as_str())
                    .ok_or_else(|| SkillError::InvalidArguments("Missing 'source'".into()))?;
                let dest = args.get("destination").and_then(|v| v.as_str())
                    .ok_or_else(|| SkillError::InvalidArguments("Missing 'destination'".into()))?;
                format!("scp -P {} -o StrictHostKeyChecking=accept-new {}:{} {}", port, host, source, dest)
            }
            "tunnel" => {
                let local = args.get("local_port").and_then(|v| v.as_u64())
                    .ok_or_else(|| SkillError::InvalidArguments("Missing 'local_port'".into()))?;
                let remote = args.get("remote_port").and_then(|v| v.as_u64())
                    .ok_or_else(|| SkillError::InvalidArguments("Missing 'remote_port'".into()))?;
                format!("ssh -N -L {}:localhost:{} {} {}", local, remote, ssh_opts, host)
            }
            "test" => {
                format!("ssh {} {} 'echo OK'", ssh_opts, host)
            }
            _ => return Err(SkillError::InvalidArguments(format!("Unknown action: {}", action))),
        };

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(timeout),
            async {
                tokio::process::Command::new("sh")
                    .args(["-c", &cmd])
                    .output()
                    .await
            }
        ).await
        .map_err(|_| SkillError::Execution(format!("SSH timed out after {}s", timeout)))?
        .map_err(|e| SkillError::Execution(e.to_string()))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok(SkillResult::success(serde_json::json!({
            "action": action,
            "host": host,
            "stdout": stdout,
            "stderr": stderr,
            "success": output.status.success(),
            "exit_code": output.status.code()
        })))
    }
}

impl Default for SshRemoteSkill {
    fn default() -> Self { Self::new() }
}
