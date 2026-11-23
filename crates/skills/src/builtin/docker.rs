//! Docker operations, executed as argv.
//!
//! The old version interpolated the model's `args` string into `sh -c`, which
//! made `args="alpine; curl …"` a host command even when Docker was not
//! installed. Docker is now invoked with an argv, and the few flags that can
//! expose the host are validated before the process starts.

use crate::traits::{Skill, SkillContext, SkillError, SkillResult};
use async_trait::async_trait;
use ravenbot_core::Permission;
use std::path::Path;

pub struct DockerSkill;

impl DockerSkill {
    pub fn new() -> Self {
        Self
    }
}

/// Split a model-authored argument string into argv without invoking a shell.
///
/// Quotes group words; backslashes escape the next character. Shell control
/// characters are refused because this tool has no shell to interpret them and
/// accepting them would only hide a mistake until Docker rejects it.
pub fn split_args(raw: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for ch in raw.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' && quote != Some('\'') {
            escaped = true;
            continue;
        }
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            } else {
                current.push(ch);
            }
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            c if c.is_whitespace() => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
            }
            ';' | '|' | '&' | '$' | '`' | '<' | '>' => {
                return Err(format!("'{ch}' is not allowed in Docker arguments"));
            }
            c if c.is_control() => {
                return Err("control characters are not allowed in Docker arguments".to_string());
            }
            _ => current.push(ch),
        }
    }
    if quote.is_some() {
        return Err("unterminated quote in Docker arguments".to_string());
    }
    if !current.is_empty() {
        out.push(current);
    }
    Ok(out)
}

fn volume_specs(args: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for (index, arg) in args.iter().enumerate() {
        if arg == "-v" || arg == "--volume" || arg == "--mount" {
            if let Some(spec) = args.get(index + 1) {
                out.push(spec.clone());
            }
        } else if let Some(spec) = arg.strip_prefix("--volume=") {
            out.push(spec.to_string());
        } else if let Some(spec) = arg.strip_prefix("--mount=") {
            out.push(spec.to_string());
        } else if let Some(spec) = arg.strip_prefix("-v") {
            if !spec.is_empty() {
                out.push(spec.to_string());
            }
        }
    }
    out
}

fn mount_source(spec: &str) -> &str {
    if spec.contains("source=") {
        spec.split(',')
            .find_map(|part| part.trim().strip_prefix("source="))
            .unwrap_or(spec)
    } else {
        spec.split(':').next().unwrap_or(spec)
    }
    .trim()
}

fn validate(ctx: &SkillContext, extra: &[String]) -> Result<(), String> {
    let lower: Vec<String> = extra
        .iter()
        .map(|value| value.to_ascii_lowercase())
        .collect();
    if lower.iter().any(|value| {
        value == "--privileged"
            || value.starts_with("--cap-add")
            || value == "--pid=host"
            || value == "--ipc=host"
            || value == "--userns=host"
            || value.starts_with("--device")
            || value.contains("docker.sock")
    }) {
        return Err("privileged containers, host namespaces, devices, and the Docker socket are not allowed".to_string());
    }

    for spec in volume_specs(extra) {
        let source = mount_source(&spec);
        if source.is_empty() || !source.starts_with('/') {
            // A named volume or relative path is not a host bind mount.
            continue;
        }
        let source = Path::new(source);
        let inside = ctx.working_dirs.iter().any(|root| source.starts_with(root));
        if !inside || source == Path::new("/") {
            return Err(format!(
                "docker bind mount '{}' is outside the office workspace",
                source.display()
            ));
        }
    }
    Ok(())
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
impl Skill for DockerSkill {
    fn id(&self) -> &str {
        "docker"
    }
    fn name(&self) -> &str {
        "Docker"
    }
    fn description(&self) -> &str {
        "Docker operations: ps, images, build, run, logs. Arguments are passed to the docker CLI as an argv; privileged containers and host mounts are refused."
    }
    fn version(&self) -> &str {
        "1.1.0"
    }
    fn required_permissions(&self) -> Vec<Permission> {
        vec![Permission::Shell]
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({"type":"object","properties":{
            "action":{"type":"string","enum":["ps","images","build","run","logs"]},
            "args":{"type":"string","description":"Extra arguments, e.g. 'myapp:latest .' for build. Quoted words are supported; shell operators are refused."}
        },"required":["action"]})
    }
    fn risk(&self) -> crate::traits::SkillRisk {
        crate::traits::SkillRisk::High
    }
    async fn execute(
        &self,
        ctx: &SkillContext,
        args: serde_json::Value,
    ) -> Result<SkillResult, SkillError> {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("ps");
        let raw = args.get("args").and_then(|v| v.as_str()).unwrap_or("");
        let extra = split_args(raw).map_err(SkillError::InvalidArguments)?;
        validate(ctx, &extra).map_err(SkillError::PermissionDenied)?;

        let mut argv: Vec<String> = vec![action.to_string()];
        match action {
            "ps" => {
                argv.push("--format".to_string());
                argv.push("table {{.Names}}\t{{.Status}}\t{{.Ports}}".to_string());
            }
            "images" => {
                argv.push("--format".to_string());
                argv.push("table {{.Repository}}:{{.Tag}}\t{{.Size}}".to_string());
            }
            "build" => argv.extend(extra),
            "run" => {
                argv.push("--rm".to_string());
                argv.extend(extra);
            }
            "logs" => argv.extend(extra),
            other => {
                return Err(SkillError::InvalidArguments(format!(
                    "Unknown docker action '{other}'. Use ps, images, build, run, or logs."
                )));
            }
        }

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(300),
            tokio::process::Command::new("docker").args(&argv).output(),
        )
        .await
        .map_err(|_| SkillError::Execution("docker timed out after 300s".to_string()))?
        .map_err(|error| SkillError::Io(error.to_string()))?;

        let stdout = clip(String::from_utf8_lossy(&output.stdout).to_string());
        let stderr = clip(String::from_utf8_lossy(&output.stderr).to_string());
        Ok(SkillResult::success(serde_json::json!({
            "action": action,
            "argv": argv,
            "stdout": stdout,
            "stderr": stderr,
            "success": output.status.success(),
            "exit_code": output.status.code()
        })))
    }
}

impl Default for DockerSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn context(root: &Path) -> SkillContext {
        SkillContext::with_default_tier(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4())
            .with_working_dirs(vec![root.to_path_buf()])
    }

    #[test]
    fn shell_operators_and_quotes_are_handled_explicitly() {
        assert_eq!(
            split_args("alpine echo hi").unwrap(),
            vec!["alpine", "echo", "hi"]
        );
        assert_eq!(
            split_args("'my image:latest' .").unwrap(),
            vec!["my image:latest", "."]
        );
        for bad in [
            "alpine; curl x",
            "alpine | sh",
            "x $(id)",
            "x `id`",
            "a > b",
        ] {
            assert!(split_args(bad).is_err(), "{bad} must be refused");
        }
    }

    #[test]
    fn host_mounts_outside_the_workspace_are_refused() {
        let root = std::env::temp_dir().join(format!("rb-docker-{}", Uuid::new_v4()));
        std::fs::create_dir_all(root.join("build")).unwrap();
        let ctx = context(&root);
        assert!(validate(&ctx, &split_args("-v /etc:/host alpine").unwrap()).is_err());
        assert!(validate(
            &ctx,
            &split_args("--mount=type=bind,source=/etc,target=/host alpine").unwrap()
        )
        .is_err());
        assert!(validate(
            &ctx,
            &split_args(&format!("-v {}:/work alpine", root.join("build").display())).unwrap()
        )
        .is_ok());
    }
}
