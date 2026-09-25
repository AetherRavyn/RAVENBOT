//! Task runner — run project tasks, scripts, and build commands with output capture

use async_trait::async_trait;
use ravenbot_core::Permission;
use crate::traits::{Skill, SkillContext, SkillError, SkillResult};
use std::path::Path;

pub struct TaskRunnerSkill;

impl TaskRunnerSkill { pub fn new() -> Self { Self } }

#[async_trait]
impl Skill for TaskRunnerSkill {
    fn id(&self) -> &str { "task_runner" }
    fn name(&self) -> &str { "Task Runner" }
    fn description(&self) -> &str { "Run project tasks, scripts, and build commands: npm scripts, Makefile targets, cargo tasks, custom commands" }
    fn version(&self) -> &str { "1.0.0" }
    fn required_permissions(&self) -> Vec<Permission> {
        vec![Permission::Shell, Permission::FileSystem { paths: vec![".".to_string()] }]
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["run", "list", "watch", "stop", "schedule"],
                    "description": "Task action"
                },
                "task": {"type": "string", "description": "Task name or command to run"},
                "cwd": {"type": "string", "description": "Working directory"},
                "timeout": {"type": "integer", "description": "Timeout in seconds (default: 60, max: 600)"},
                "env": {"type": "object", "description": "Additional environment variables"},
                "background": {"type": "boolean", "description": "Run in background (default: false)"},
                "schedule": {"type": "string", "description": "Cron expression for scheduling"}
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, _ctx: &SkillContext, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let action = args.get("action").and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'action'".into()))?;

        match action {
            "run" => self.run_task(args).await,
            "list" => self.list_tasks(args).await,
            "watch" => Ok(SkillResult::failure("Watch mode not yet implemented".to_string())),
            "stop" => Ok(SkillResult::failure("Stop requires background task tracking".to_string())),
            "schedule" => Ok(SkillResult::failure("Use the scheduler skill for cron scheduling".to_string())),
            _ => Err(SkillError::InvalidArguments(format!("Unknown action: {}", action))),
        }
    }
}

impl TaskRunnerSkill {
    async fn run_task(&self, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let task = args.get("task").and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'task'".into()))?;
        let cwd = args.get("cwd").and_then(|v| v.as_str()).unwrap_or(".");
        let timeout = args.get("timeout").and_then(|v| v.as_u64()).unwrap_or(60).min(600);

        // Resolve task from package.json, Makefile, or run as raw command
        let command = self.resolve_task(task, cwd).await;

        let mut cmd = if cfg!(target_os = "windows") {
            let mut c = tokio::process::Command::new("cmd");
            c.args(["/C", &command]);
            c
        } else {
            let mut c = tokio::process::Command::new("sh");
            c.args(["-c", &command]);
            c
        };

        cmd.current_dir(cwd);

        // Add custom env vars
        if let Some(env_vars) = args.get("env").and_then(|v| v.as_object()) {
            for (k, v) in env_vars {
                if let Some(val) = v.as_str() {
                    cmd.env(k, val);
                }
            }
        }

        let start = std::time::Instant::now();
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(timeout),
            cmd.output(),
        ).await
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
                            } else if content.contains("yarn") || Path::new(&format!("{}/yarn.lock", cwd)).exists() {
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
        if Path::new(&format!("{}/pyproject.toml", cwd)).exists() || Path::new(&format!("{}/setup.py", cwd)).exists() {
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

    async fn list_tasks(&self, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let cwd = args.get("cwd").and_then(|v| v.as_str()).unwrap_or(".");
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
            for task in &["build", "test", "check", "run", "fmt", "clippy", "doc", "bench", "clean"] {
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
    fn default() -> Self { Self::new() }
}
