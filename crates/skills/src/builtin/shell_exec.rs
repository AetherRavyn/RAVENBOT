//! Shell execution skill — runs commands inside the bot's sandbox.
//!
//! Commands are executed through [`ravenbot_sandbox::SandboxRunner`]: on Linux
//! with bubblewrap available this gives real filesystem + PID/UTS/IPC (and
//! optional network) namespace isolation plus POSIX resource limits; otherwise
//! it degrades to limits-only and says so in the result.

use async_trait::async_trait;
use ravenbot_core::{Permission, SandboxTier};

use crate::traits::{Skill, SkillContext, SkillError, SkillResult};

pub struct ShellExecSkill;

impl ShellExecSkill {
    pub fn new() -> Self {
        Self
    }

    /// Interprets `RAVENBOT_SANDBOX_TIER` (os-level|docker|host) for headless
    /// runs where no per-bot config is threaded through.
    fn tier_from_env() -> SandboxTier {
        match std::env::var("RAVENBOT_SANDBOX_TIER")
            .unwrap_or_default()
            .trim()
            .to_lowercase()
            .as_str()
        {
            "host" => SandboxTier::Host,
            "docker" => SandboxTier::Docker,
            _ => SandboxTier::OsLevel,
        }
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
        "Run a build, test, git, or system command inside the sandbox. Not for browsing files or reading text — use file_tree, code_search, and file_read."
    }

    fn version(&self) -> &str {
        "1.1.0"
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
                    "description": "Command for a build, test, git, or process task. Do not use shell find/ls -R/grep -r to discover files; use file_tree, code_search, or file_read."
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
        context: &SkillContext,
        arguments: serde_json::Value,
    ) -> Result<SkillResult, SkillError> {
        let command = arguments
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'command' field".to_string()))?;

        // Office agents run autonomously. This denylist is the hard boundary:
        // a machine-destructive command is refused before the sandbox sees it,
        // regardless of approval mode.
        if let Some(reason) =
            crate::safety::dangerous_command_reason(command, &context.working_dirs)
        {
            return Ok(SkillResult::failure(format!(
                "Blocked by RAVENBOT's safety denylist: {reason}. The command was not executed. Choose a non-destructive action or work inside the office workspace."
            )));
        }

        // Tool discipline, not safety: a whole-machine `find /` burns the
        // round budget without producing an artifact. The workspace tools
        // (`file_tree`, `code_search`) answer the same question cheaply.
        if let Some(reason) = crate::safety::wasteful_scan_reason(command, &context.working_dirs) {
            let roots = context
                .working_dirs
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            return Ok(SkillResult::failure(format!(
                "Blocked by tool policy: {reason}. Work inside {roots} and use file_tree for paths or code_search for content. The command was not executed."
            )));
        }

        let cwd_arg = arguments.get("cwd").and_then(|v| v.as_str());

        let timeout = arguments
            .get("timeout")
            .and_then(|v| v.as_u64())
            .unwrap_or(30)
            .min(300);

        let tier = match context.sandbox_tier {
            // An explicit non-default tier on the context wins; otherwise fall
            // back to the environment (headless/CLI).
            SandboxTier::Host => SandboxTier::Host,
            SandboxTier::Docker => SandboxTier::Docker,
            SandboxTier::OsLevel => Self::tier_from_env(),
        };
        // Bind the office's workspace roots, not just the cwd. Without this a
        // command that does `cd /tmp` first loses access to the folder it is
        // working in, and a tool reading a sibling file by absolute path fails
        // even though the path is inside the office.
        let runner = crate::exec::runner_from_tier(tier, &context.working_dirs);

        let cwd = match cwd_arg {
            Some(arg) => Some(context.resolve_path(arg)?),
            None => context.primary_dir().map(|p| p.to_path_buf()),
        };
        let args = vec!["-c".to_string(), command.to_string()];
        let mut cmd = runner
            .command("sh", &args, cwd.as_deref())
            .map_err(SkillError::Execution)?;

        let output = tokio::time::timeout(std::time::Duration::from_secs(timeout), cmd.output())
            .await
            .map_err(|_| SkillError::Execution(format!("Command timed out after {}s", timeout)))?
            .map_err(|e| SkillError::Execution(e.to_string()))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        // Truncate large outputs
        let max_output = 100_000;
        let truncated_stdout = if stdout.len() > max_output {
            format!("{}...(truncated)", crate::clip_utf8(&stdout, max_output))
        } else {
            stdout.clone()
        };
        let truncated_stderr = if stderr.len() > max_output {
            format!("{}...(truncated)", crate::clip_utf8(&stderr, max_output))
        } else {
            stderr.clone()
        };

        let success = output.status.success();
        let report = runner.report();

        Ok(SkillResult::success(serde_json::json!({
            "command": command,
            "exit_code": output.status.code(),
            "stdout": truncated_stdout,
            "stderr": truncated_stderr,
            "success": success,
            "sandbox": {
                "backend": report.backend,
                "filesystem_isolated": report.filesystem_isolated,
                "network_isolated": report.network_isolated,
                "note": report.note,
            },
            "truncated": stdout.len() > max_output || stderr.len() > max_output
        })))
    }
}

impl Default for ShellExecSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::SkillContext;
    use uuid::Uuid;

    fn ctx(tier: SandboxTier) -> SkillContext {
        SkillContext::new(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), tier)
    }

    #[tokio::test]
    async fn runs_command_and_reports_sandbox() {
        let skill = ShellExecSkill::new();
        let result = skill
            .execute(
                &ctx(SandboxTier::OsLevel),
                serde_json::json!({"command": "echo hi"}),
            )
            .await
            .expect("execute");
        assert!(result.success);
        assert_eq!(
            result.output.get("stdout").and_then(|v| v.as_str()),
            Some("hi\n")
        );
        // The result always tells the caller what isolation was applied.
        assert!(result.output.get("sandbox").is_some());
    }

    #[tokio::test]
    async fn a_whole_machine_scan_is_refused_in_a_custom_workspace() {
        let skill = ShellExecSkill::new();
        let root = std::env::temp_dir().join(format!("rb-scan-office-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let context = ctx(SandboxTier::Host).with_working_dirs(vec![root.clone()]);

        let blocked = skill
            .execute(
                &context,
                serde_json::json!({"command": "find / -maxdepth 2 -type d"}),
            )
            .await
            .expect("execute");
        assert!(!blocked.success, "a whole-machine scan must be refused");
        assert!(
            blocked
                .error
                .as_deref()
                .unwrap_or("")
                .contains("tool policy"),
            "expected a tool-policy message, got {blocked:?}"
        );

        let allowed = skill
            .execute(
                &context,
                serde_json::json!({"command": "find . -maxdepth 1"}),
            )
            .await
            .expect("execute");
        assert!(
            allowed.success,
            "workspace-scoped find should run: {allowed:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn a_custom_workspace_blocks_its_own_root_but_allows_a_subfolder() {
        let skill = ShellExecSkill::new();
        // The folder name and location are arbitrary; nothing is hardcoded.
        let root = std::env::temp_dir().join(format!("rb-custom-office-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let context = ctx(SandboxTier::Host).with_working_dirs(vec![root.clone()]);

        let blocked = skill
            .execute(
                &context,
                serde_json::json!({"command": format!("rm -rf {}", root.display())}),
            )
            .await
            .expect("execute");
        assert!(!blocked.success, "deleting the office root must be refused");
        assert!(
            blocked
                .error
                .as_deref()
                .unwrap_or("")
                .contains("safety denylist"),
            "expected a denylist message, got {blocked:?}"
        );
        assert!(root.exists(), "the blocked command must not run");

        let build = root.join("build");
        std::fs::create_dir_all(&build).unwrap();
        let allowed = skill
            .execute(
                &context,
                serde_json::json!({"command": format!("rm -rf {}", build.display())}),
            )
            .await
            .expect("execute");
        assert!(
            allowed.success,
            "a delete inside the office should run: {allowed:?}"
        );
        assert!(!build.exists());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn host_tier_reports_no_isolation() {
        let skill = ShellExecSkill::new();
        let result = skill
            .execute(
                &ctx(SandboxTier::Host),
                serde_json::json!({"command": "true"}),
            )
            .await
            .expect("execute");
        let backend = result
            .output
            .pointer("/sandbox/backend")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert_eq!(backend, "none");
    }

    #[tokio::test]
    async fn missing_command_is_invalid_arguments() {
        let skill = ShellExecSkill::new();
        let err = skill
            .execute(&ctx(SandboxTier::OsLevel), serde_json::json!({}))
            .await
            .unwrap_err();
        assert!(matches!(err, SkillError::InvalidArguments(_)));
    }
}
