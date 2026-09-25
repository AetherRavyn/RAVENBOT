//! Package manager — install, update, search, and manage packages (npm, pip, cargo, apt, etc.)

use async_trait::async_trait;
use ravenbot_core::Permission;
use crate::traits::{Skill, SkillContext, SkillError, SkillResult};

pub struct PackageManagerSkill;

impl PackageManagerSkill { pub fn new() -> Self { Self } }

#[async_trait]
impl Skill for PackageManagerSkill {
    fn id(&self) -> &str { "package_manager" }
    fn name(&self) -> &str { "Package Manager" }
    fn description(&self) -> &str { "Install, update, search, and manage packages via npm, pip, cargo, apt, brew, etc." }
    fn version(&self) -> &str { "1.0.0" }
    fn required_permissions(&self) -> Vec<Permission> {
        vec![Permission::Shell, Permission::Network { domains: vec!["*".to_string()] }]
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "manager": {
                    "type": "string",
                    "enum": ["npm", "pip", "cargo", "apt", "brew", "yarn", "pnpm", "gem", "go", "auto"],
                    "description": "Package manager to use (auto-detects from project if 'auto')"
                },
                "action": {
                    "type": "string",
                    "enum": ["install", "uninstall", "update", "search", "list", "outdated", "info", "audit", "init"],
                    "description": "Action to perform"
                },
                "packages": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Package names (for install/uninstall/info)"
                },
                "global": {
                    "type": "boolean",
                    "description": "Install globally (default: false)"
                },
                "cwd": {
                    "type": "string",
                    "description": "Working directory (for auto-detection)"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, _ctx: &SkillContext, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let action = args.get("action").and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'action' field".into()))?;

        let manager = args.get("manager").and_then(|v| v.as_str());
        let packages: Vec<String> = args.get("packages")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let global = args.get("global").and_then(|v| v.as_bool()).unwrap_or(false);
        let cwd = args.get("cwd").and_then(|v| v.as_str());

        // Auto-detect manager if needed
        let manager = match manager {
            Some("auto") | None => self.detect_manager(cwd).await,
            Some(m) => m.to_string(),
        };

        if manager.is_empty() {
            return Ok(SkillResult::failure("Could not auto-detect package manager. Please specify one.".to_string()));
        }

        let cmd = self.build_command(&manager, action, &packages, global);

        let mut command = tokio::process::Command::new("sh");
        command.args(["-c", &cmd]);
        if let Some(cwd) = cwd {
            command.current_dir(cwd);
        }

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(120),
            command.output(),
        ).await
        .map_err(|_| SkillError::Execution("Command timed out after 120s".into()))?
        .map_err(|e| SkillError::Execution(e.to_string()))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok(SkillResult::success(serde_json::json!({
            "manager": manager,
            "action": action,
            "stdout": stdout,
            "stderr": stderr,
            "success": output.status.success(),
            "exit_code": output.status.code()
        })))
    }
}

impl PackageManagerSkill {
    async fn detect_manager(&self, cwd: Option<&str>) -> String {
        let dir = cwd.unwrap_or(".");
        let pnpm_lock = format!("{}/pnpm-lock.yaml", dir);
        let package_json = format!("{}/package.json", dir);
        let yarn_lock = format!("{}/yarn.lock", dir);
        let cargo_toml = format!("{}/Cargo.toml", dir);
        let requirements = format!("{}/requirements.txt", dir);
        let pyproject = format!("{}/pyproject.toml", dir);
        let gemfile = format!("{}/Gemfile", dir);
        let go_mod = format!("{}/go.mod", dir);

        if tokio::fs::metadata(&package_json).await.is_ok() {
            if tokio::fs::metadata(&pnpm_lock).await.is_ok() {
                "pnpm".to_string()
            } else if tokio::fs::metadata(&yarn_lock).await.is_ok() {
                "yarn".to_string()
            } else {
                "npm".to_string()
            }
        } else if tokio::fs::metadata(&cargo_toml).await.is_ok() {
            "cargo".to_string()
        } else if tokio::fs::metadata(&pyproject).await.is_ok() || tokio::fs::metadata(&requirements).await.is_ok() {
            "pip".to_string()
        } else if tokio::fs::metadata(&gemfile).await.is_ok() {
            "gem".to_string()
        } else if tokio::fs::metadata(&go_mod).await.is_ok() {
            "go".to_string()
        } else {
            String::new()
        }
    }

    fn build_command(&self, manager: &str, action: &str, packages: &[String], global: bool) -> String {
        let pkgs = packages.join(" ");
        match (manager, action) {
            ("npm", "install") => if pkgs.is_empty() { "npm install 2>&1".to_string() } else { format!("npm install {} 2>&1", pkgs) },
            ("npm", "uninstall") => format!("npm uninstall {} 2>&1", pkgs),
            ("npm", "update") => "npm update 2>&1".to_string(),
            ("npm", "search") => format!("npm search {} 2>&1 | head -n 20", pkgs),
            ("npm", "list") => if global { "npm list -g --depth=0 2>&1".to_string() } else { "npm list --depth=0 2>&1".to_string() },
            ("npm", "outdated") => "npm outdated 2>&1".to_string(),
            ("npm", "audit") => "npm audit 2>&1".to_string(),
            ("npm", "info") => format!("npm info {} 2>&1", pkgs),
            ("pip", "install") => if global { format!("pip install {} 2>&1", pkgs) } else { format!("pip install --user {} 2>&1", pkgs) },
            ("pip", "uninstall") => format!("pip uninstall -y {} 2>&1", pkgs),
            ("pip", "update") => format!("pip install --upgrade {} 2>&1", pkgs),
            ("pip", "search") => format!("pip search {} 2>&1 | head -n 20", pkgs),
            ("pip", "list") => "pip list 2>&1".to_string(),
            ("pip", "outdated") => "pip list --outdated 2>&1".to_string(),
            ("pip", "info") => format!("pip show {} 2>&1", pkgs),
            ("pip", "init") => "pip install pip-tools 2>&1".to_string(),
            ("cargo", "install") => format!("cargo install {} 2>&1", pkgs),
            ("cargo", "update") => "cargo update 2>&1".to_string(),
            ("cargo", "search") => format!("cargo search {} 2>&1 | head -n 10", pkgs),
            ("cargo", "list") => "cargo install --list 2>&1".to_string(),
            ("cargo", "info") => format!("cargo info {} 2>&1", pkgs),
            ("cargo", "audit") => "cargo audit 2>&1".to_string(),
            ("cargo", "init") => "cargo init 2>&1".to_string(),
            ("yarn", "install") => if pkgs.is_empty() { "yarn install 2>&1".to_string() } else { format!("yarn add {} 2>&1", pkgs) },
            ("yarn", "uninstall") => format!("yarn remove {} 2>&1", pkgs),
            ("yarn", "update") => "yarn upgrade 2>&1".to_string(),
            ("pnpm", "install") => if pkgs.is_empty() { "pnpm install 2>&1".to_string() } else { format!("pnpm add {} 2>&1", pkgs) },
            ("pnpm", "uninstall") => format!("pnpm remove {} 2>&1", pkgs),
            ("apt", "install") => format!("sudo apt install -y {} 2>&1", pkgs),
            ("apt", "update") => "sudo apt update && sudo apt upgrade -y 2>&1".to_string(),
            ("apt", "search") => format!("apt search {} 2>&1 | head -n 20", pkgs),
            ("apt", "list") => "dpkg -l 2>&1 | tail -n +6 | head -n 50".to_string(),
            ("brew", "install") => format!("brew install {} 2>&1", pkgs),
            ("brew", "update") => "brew update && brew upgrade 2>&1".to_string(),
            ("brew", "search") => format!("brew search {} 2>&1", pkgs),
            ("brew", "list") => "brew list 2>&1".to_string(),
            ("brew", "outdated") => "brew outdated 2>&1".to_string(),
            ("go", "install") => format!("go install {} 2>&1", pkgs),
            ("go", "update") => "go get -u ./... 2>&1".to_string(),
            _ => format!("echo 'Unsupported: {} {}'", manager, action),
        }
    }
}

impl Default for PackageManagerSkill {
    fn default() -> Self { Self::new() }
}
