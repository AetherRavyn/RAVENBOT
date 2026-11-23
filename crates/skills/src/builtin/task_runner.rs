//! Task runner — run project tasks, scripts, and build commands with output capture

use crate::traits::{Skill, SkillContext, SkillError, SkillResult};
use async_trait::async_trait;
use ravenbot_core::Permission;
use std::path::Path;
use uuid::Uuid;

pub struct TaskRunnerSkill;

impl TaskRunnerSkill {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Skill for TaskRunnerSkill {
    fn id(&self) -> &str {
        "task_runner"
    }
    fn name(&self) -> &str {
        "Task Runner"
    }
    fn description(&self) -> &str {
        "Run project tasks, scripts, and build commands: npm scripts, Makefile targets, cargo tasks, custom commands"
    }
    fn version(&self) -> &str {
        "1.0.0"
    }
    fn required_permissions(&self) -> Vec<Permission> {
        vec![
            Permission::Shell,
            Permission::FileSystem {
                paths: vec![".".to_string()],
            },
        ]
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["run", "list", "wait", "status", "processes", "stop", "watch", "schedule"],
                    "description": "Task action: run, list project tasks, wait for a background process, inspect status, list background processes, stop one, watch, or schedule"
                },
                "task": {"type": "string", "description": "Task name or command to run"},
                "cwd": {"type": "string", "description": "Working directory"},
                "timeout": {"type": "integer", "description": "Timeout in seconds (default: 60, max: 3600 for waits, 600 for runs)"},
                "env": {"type": "object", "description": "Additional environment variables"},
                "background": {"type": "boolean", "description": "Run in background and return a process id immediately (default: false)"},
                "process_id": {"type": "string", "description": "Background process id returned by a background run"},
                "pattern": {"type": "string", "description": "Wait until this text appears in the process output"},
                "schedule": {"type": "string", "description": "Cron expression for scheduling"}
            },
            "required": ["action"]
        })
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

        match action {
            "run" => self.run_task(ctx, args).await,
            "list" => self.list_tasks(ctx, args).await,
            "wait" => self.wait_task(args).await,
            "status" => self.status_task(args).await,
            "processes" => self.processes_task(ctx, args).await,
            "stop" => self.stop_task(args).await,
            "watch" => Ok(SkillResult::failure(
                "Watch mode not yet implemented; run in the background and use wait with a pattern instead."
                    .to_string(),
            )),
            "schedule" => Ok(SkillResult::failure(
                "Use the scheduler skill for cron scheduling".to_string(),
            )),
            _ => Err(SkillError::InvalidArguments(format!(
                "Unknown action: {}",
                action
            ))),
        }
    }
}

impl TaskRunnerSkill {
    async fn run_task(
        &self,
        ctx: &SkillContext,
        args: serde_json::Value,
    ) -> Result<SkillResult, SkillError> {
        let task = args
            .get("task")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'task'".into()))?;
        let raw_cwd = args.get("cwd").and_then(|v| v.as_str()).unwrap_or(".");
        let timeout = args
            .get("timeout")
            .and_then(|v| v.as_u64())
            .unwrap_or(60)
            .min(600);

        // Confine the directory first: it decides where the task runs, and
        // `resolve_task` probes for a manifest underneath it. It used to be
        // used raw, so `cwd: "/etc"` ran there and `cwd: "../../.."` ran at
        // the filesystem root.
        let dir = crate::exec::resolve_cwd(ctx, Some(raw_cwd))?;
        let cwd = dir.to_string_lossy().to_string();

        // Resolve the task from package.json, a Makefile, or a known runner.
        let command = self.resolve_task(task, &cwd).await;

        // A project task can be a shell line. It gets the same hard denylist as
        // shell_exec; "it came from package.json" is not a safety boundary.
        if let Some(reason) = crate::safety::dangerous_command_reason(&command, &ctx.working_dirs) {
            return Ok(SkillResult::failure(format!(
                "Blocked by RAVENBOT's safety denylist: {reason}. The task was not executed."
            )));
        }

        let runner = crate::exec::runner_for(ctx);
        let mut cmd = if cfg!(target_os = "windows") {
            let mut c = tokio::process::Command::new("cmd");
            c.args(["/C", &command]);
            c
        } else {
            // The shell is unavoidable here — a Makefile target and an npm
            // script are shell lines. The boundary is the sandbox plus the
            // confined cwd, not the absence of a shell.
            runner
                .command("sh", &["-c".to_string(), command.clone()], Some(&dir))
                .map_err(SkillError::Execution)?
        };

        cmd.current_dir(&dir);

        // Custom env vars. Keys are validated: an arbitrary key here is a way
        // to change how every child process behaves.
        if let Some(env_vars) = args.get("env").and_then(|v| v.as_object()) {
            for (k, v) in env_vars {
                let val = v.as_str().unwrap_or_default();
                let sane_key = !k.is_empty()
                    && k.len() <= 64
                    && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    && !k.starts_with(|c: char| c.is_ascii_digit());
                if !sane_key {
                    return Err(SkillError::InvalidArguments(format!(
                        "Invalid environment variable name {k:?}."
                    )));
                }
                if val.len() > 4096 {
                    return Err(SkillError::InvalidArguments(format!(
                        "Value for {k} is too long."
                    )));
                }
                cmd.env(k, val);
            }
        }

        if args
            .get("background")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
        {
            let snapshot = crate::processes::start(
                cmd,
                command.clone(),
                cwd.clone(),
                ctx.bot_id,
                ctx.thread_id,
                ctx.run_id,
            )
            .map_err(SkillError::Execution)?;
            return Ok(SkillResult::success(serde_json::json!({
                "task": task,
                "command": command,
                "background": true,
                "process_id": snapshot.process_id,
                "pid": snapshot.pid,
                "status": snapshot.status,
                "output": snapshot.output,
                "started_at": snapshot.started_at,
                "note": "Use action=wait with this process_id to release on exit, a pattern, or a timeout."
            })));
        }

        let start = std::time::Instant::now();
        let output = tokio::time::timeout(std::time::Duration::from_secs(timeout), cmd.output())
            .await
            .map_err(|_| SkillError::Execution(format!("Task timed out after {}s", timeout)))?
            .map_err(|e| SkillError::Execution(e.to_string()))?;

        let elapsed = start.elapsed();
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok(SkillResult::success(serde_json::json!({
            "task": task,
            "command": command,
            "exit_code": output.status.code(),
            "success": output.status.success(),
            "stdout": stdout,
            "stderr": stderr,
            "duration_ms": elapsed.as_millis()
        })))
    }

    async fn resolve_task(&self, task: &str, cwd: &str) -> String {
        // Check package.json scripts
        let package_json = format!("{}/package.json", cwd);
        if Path::new(&package_json).exists() {
            if let Ok(content) = tokio::fs::read_to_string(&package_json).await {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(scripts) = json.get("scripts").and_then(|s| s.as_object()) {
                        if scripts.contains_key(task) {
                            if content.contains("pnpm") {
                                return format!("pnpm run {}", task);
                            } else if content.contains("yarn")
                                || Path::new(&format!("{}/yarn.lock", cwd)).exists()
                            {
                                return format!("yarn {}", task);
                            } else {
                                return format!("npm run {}", task);
                            }
                        }
                    }
                }
            }
        }

        // Check Makefile
        let makefile = format!("{}/Makefile", cwd);
        if Path::new(&makefile).exists() {
            if let Ok(content) = tokio::fs::read_to_string(&makefile).await {
                let target = format!("{}:", task);
                if content.contains(&target) {
                    return format!("make {}", task);
                }
            }
        }

        // Check Cargo.toml
        let cargo = format!("{}/Cargo.toml", cwd);
        if Path::new(&cargo).exists() {
            match task {
                "build" => return "cargo build".to_string(),
                "test" => return "cargo test".to_string(),
                "check" => return "cargo check".to_string(),
                "fmt" => return "cargo fmt".to_string(),
                "clippy" => return "cargo clippy".to_string(),
                "run" => return "cargo run".to_string(),
                "doc" => return "cargo doc --open".to_string(),
                "bench" => return "cargo bench".to_string(),
                "clean" => return "cargo clean".to_string(),
                _ => {}
            }
        }

        // Check pyproject.toml / setup.py
        if Path::new(&format!("{}/pyproject.toml", cwd)).exists()
            || Path::new(&format!("{}/setup.py", cwd)).exists()
        {
            match task {
                "test" => return "pytest".to_string(),
                "lint" => return "ruff check .".to_string(),
                "format" => return "ruff format .".to_string(),
                "typecheck" => return "mypy .".to_string(),
                _ => {}
            }
        }

        // Default: run as raw command
        task.to_string()
    }

    fn process_id(args: &serde_json::Value) -> Result<Uuid, SkillError> {
        args.get("process_id")
            .and_then(|value| value.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'process_id'".into()))
            .and_then(|value| {
                Uuid::parse_str(value.trim()).map_err(|_| {
                    SkillError::InvalidArguments("'process_id' is not a valid id".into())
                })
            })
    }

    async fn wait_task(&self, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let id = Self::process_id(&args)?;
        let timeout = args
            .get("timeout")
            .and_then(|value| value.as_u64())
            .unwrap_or(60)
            .min(3_600);
        let pattern = args.get("pattern").and_then(|value| value.as_str());
        let outcome = crate::processes::wait(id, timeout, pattern)
            .await
            .map_err(SkillError::Execution)?;
        Ok(SkillResult::success(serde_json::json!({
            "process": outcome.snapshot,
            "matched_pattern": outcome.matched_pattern,
            "timed_out": outcome.timed_out
        })))
    }

    async fn status_task(&self, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let id = Self::process_id(&args)?;
        Ok(SkillResult::success(serde_json::json!({
            "process": crate::processes::snapshot(id).map_err(SkillError::Execution)?
        })))
    }

    async fn processes_task(
        &self,
        ctx: &SkillContext,
        _args: serde_json::Value,
    ) -> Result<SkillResult, SkillError> {
        let processes = crate::processes::list(Some(ctx.thread_id), None);
        Ok(SkillResult::success(serde_json::json!({
            "processes": processes,
            "count": processes.len()
        })))
    }

    async fn stop_task(&self, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let id = Self::process_id(&args)?;
        Ok(SkillResult::success(serde_json::json!({
            "process": crate::processes::stop(id).await.map_err(SkillError::Execution)?
        })))
    }

    async fn list_tasks(
        &self,
        ctx: &SkillContext,
        args: serde_json::Value,
    ) -> Result<SkillResult, SkillError> {
        // Confined for the same reason as `run_task`: this reads manifests off
        // disk, so the directory is not just a label.
        let dir = crate::exec::resolve_cwd(ctx, args.get("cwd").and_then(|v| v.as_str()))?;
        let cwd = dir.to_string_lossy().to_string();
        let mut tasks = Vec::new();

        // package.json scripts
        let package_json = format!("{}/package.json", cwd);
        if Path::new(&package_json).exists() {
            if let Ok(content) = tokio::fs::read_to_string(&package_json).await {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(scripts) = json.get("scripts").and_then(|s| s.as_object()) {
                        for (name, cmd) in scripts {
                            tasks.push(serde_json::json!({
                                "name": name,
                                "command": cmd.as_str().unwrap_or(""),
                                "source": "package.json"
                            }));
                        }
                    }
                }
            }
        }

        // Makefile targets
        let makefile = format!("{}/Makefile", cwd);
        if Path::new(&makefile).exists() {
            if let Ok(content) = tokio::fs::read_to_string(&makefile).await {
                for line in content.lines() {
                    if line.contains(':') && !line.starts_with('\t') && !line.starts_with('#') {
                        let target = line.split(':').next().unwrap_or("").trim();
                        if !target.is_empty() {
                            tasks.push(serde_json::json!({
                                "name": target,
                                "command": format!("make {}", target),
                                "source": "Makefile"
                            }));
                        }
                    }
                }
            }
        }

        // Cargo tasks
        if Path::new(&format!("{}/Cargo.toml", cwd)).exists() {
            for task in &[
                "build", "test", "check", "run", "fmt", "clippy", "doc", "bench", "clean",
            ] {
                tasks.push(serde_json::json!({
                    "name": task,
                    "command": format!("cargo {}", task),
                    "source": "Cargo.toml"
                }));
            }
        }

        Ok(SkillResult::success(serde_json::json!({
            "tasks": tasks,
            "count": tasks.len()
        })))
    }
}

impl Default for TaskRunnerSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::Skill;

    #[cfg(unix)]
    fn context() -> (SkillContext, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("ravenbot-task-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let ctx = SkillContext::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            ravenbot_core::SandboxTier::Host,
        )
        .with_working_dirs(vec![dir.clone()])
        .confined();
        (ctx, dir)
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_background_task_can_be_waited_to_completion() {
        let (ctx, dir) = context();
        let skill = TaskRunnerSkill::new();
        let started = skill
            .execute(
                &ctx,
                serde_json::json!({
                    "action": "run",
                    "task": "printf 'READY\\n'; sleep 0.2; printf 'DONE\\n'",
                    "background": true
                }),
            )
            .await
            .expect("start");
        assert!(started.success, "{:?}", started.error);
        assert_eq!(started.output["background"], true);
        let id = started.output["process_id"]
            .as_str()
            .expect("process id")
            .to_string();

        let waited = skill
            .execute(
                &ctx,
                serde_json::json!({
                    "action": "wait",
                    "process_id": id,
                    "timeout": 5
                }),
            )
            .await
            .expect("wait");
        assert!(waited.success, "{:?}", waited.error);
        assert_eq!(waited.output["process"]["status"], "completed");
        assert!(waited.output["process"]["output"]
            .as_str()
            .unwrap_or_default()
            .contains("DONE"));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_pattern_releases_a_wait_while_the_process_still_runs() {
        let (ctx, dir) = context();
        let skill = TaskRunnerSkill::new();
        let started = skill
            .execute(
                &ctx,
                serde_json::json!({
                    "action": "run",
                    "task": "printf 'READY\\n'; sleep 20",
                    "background": true
                }),
            )
            .await
            .expect("start");
        let id = started.output["process_id"]
            .as_str()
            .expect("process id")
            .to_string();

        let waited = skill
            .execute(
                &ctx,
                serde_json::json!({
                    "action": "wait",
                    "process_id": id,
                    "pattern": "READY",
                    "timeout": 5
                }),
            )
            .await
            .expect("wait");
        assert!(waited.success);
        assert_eq!(waited.output["matched_pattern"], true);
        assert_eq!(waited.output["process"]["status"], "running");

        let stopped = skill
            .execute(
                &ctx,
                serde_json::json!({"action": "stop", "process_id": id}),
            )
            .await
            .expect("stop");
        assert!(stopped.success);
        assert_eq!(stopped.output["process"]["status"], "stopped");

        let listed = skill
            .execute(&ctx, serde_json::json!({"action": "processes"}))
            .await
            .expect("list");
        assert!(listed.output["count"].as_u64().unwrap_or(0) >= 1);

        let _ = std::fs::remove_dir_all(dir);
    }
}
