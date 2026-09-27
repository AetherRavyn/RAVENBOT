//! Git — status, diff, log, commit, branch, push

use crate::exec;
use crate::traits::{Skill, SkillContext, SkillError, SkillResult};
use async_trait::async_trait;
use ravenbot_core::Permission;

pub struct GitSkill;

impl GitSkill { pub fn new() -> Self { Self } }

#[async_trait]
impl Skill for GitSkill {
    fn id(&self) -> &str { "git" }
    fn name(&self) -> &str { "Git" }
    fn description(&self) -> &str { "Git operations: status, diff, log, commit, branch, push, create_branch" }
    fn version(&self) -> &str { "1.1.0" }
    fn required_permissions(&self) -> Vec<Permission> {
        vec![Permission::Shell, Permission::FileSystem { paths: vec![".".to_string()] }]
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type":"object","properties":{
                "action":{"type":"string","enum":["status","diff","log","commit","branch","push","create_branch"],"description":"Git action"},
                "message":{"type":"string","description":"Commit message (for commit)"},
                "branch":{"type":"string","description":"Branch name (for create_branch)"},
                "path":{"type":"string","description":"Repository path inside the office, relative to its root"},
                "args":{"type":"string","description":"Extra args"}
            },"required":["action"]
        })
    }

    /// Git actions are argv calls, not shell strings.
    ///
    /// The previous version built `sh -c "git add -A && git commit -m '<msg>'"`,
    /// which meant the sandbox could not see inside it and a `path` argument
    /// was never resolved. Running `git` directly means the workspace roots
    /// are bound, the cwd went through the confinement check, and a commit
    /// message containing a quote is just a commit message.
    async fn execute(&self, ctx: &SkillContext, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let action = args
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing action".into()))?;
        let path = args.get("path").and_then(|v| v.as_str());

        match action {
            "status" => run(ctx, &["status", "--porcelain", "--branch"], path, action).await,
            "diff" => {
                let out = run(ctx, &["diff", "--stat"], path, "diff_stat").await?;
                let full = run(ctx, &["diff"], path, "diff").await?;
                Ok(SkillResult::success(serde_json::json!({
                    "action": action,
                    "stat": out.output.get("stdout"),
                    "stdout": full.output.get("stdout"),
                    "stderr": full.output.get("stderr"),
                    "success": out.output.get("success").and_then(|v| v.as_bool()).unwrap_or(false)
                        && full.output.get("success").and_then(|v| v.as_bool()).unwrap_or(false),
                })))
            }
            "log" => run(ctx, &["log", "--oneline", "-20"], path, action).await,
            "branch" => run(ctx, &["branch", "--all"], path, action).await,
            "commit" => {
                let message = args
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("chore: update");
                // Two calls rather than `&&`, so a failure to stage is
                // reported as itself instead of hiding behind a short-circuit.
                let staged = run(ctx, &["add", "-A"], path, "add").await?;
                if !staged.output["success"].as_bool().unwrap_or(false) {
                    return Ok(staged);
                }
                run(ctx, &["commit", "-m", message], path, action).await
            }
            "create_branch" => {
                let branch = args
                    .get("branch")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| SkillError::InvalidArguments("Missing branch".into()))?;
                run(ctx, &["checkout", "-b", branch], path, action).await
            }
            "push" => run(ctx, &["push"], path, action).await,
            other => Err(SkillError::InvalidArguments(format!(
                "Unknown action {other}"
            ))),
        }
    }
}

async fn run(
    ctx: &SkillContext,
    argv: &[&str],
    path: Option<&str>,
    action: &str,
) -> Result<SkillResult, SkillError> {
    let out = exec::run(ctx, "git", argv, path).await?;
    Ok(exec::output_to_result(action, &out))
}

impl Default for GitSkill { fn default() -> Self { Self::new() } }

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use uuid::Uuid;

    fn ctx(root: &Path) -> SkillContext {
        SkillContext::with_default_tier(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4())
            .with_working_dirs(vec![root.to_path_buf()])
    }

    fn sandbox() -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("rb-git-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn a_repo_path_outside_the_office_is_refused() {
        let root = sandbox();
        let skill = GitSkill::new();
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(skill.execute(
                &ctx(&root),
                serde_json::json!({"action":"status","path":"/etc"}),
            ));
        assert!(result.is_err(), "an outside repo path was accepted");
        assert!(result.unwrap_err().to_string().contains("outside"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn status_against_a_real_repository_reports_cleanly() {
        let root = sandbox();
        let skill = GitSkill::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            exec::run(&ctx(&root), "git", &["init", "-q"], None)
                .await
                .unwrap();
        });
        let result = rt
            .block_on(skill.execute(&ctx(&root), serde_json::json!({"action":"status"})))
            .expect("status should run");
        // A non-zero exit is reported, not treated as a skill failure.
        assert!(result.output.get("stdout").is_some(), "{:?}", result.output);
        let _ = std::fs::remove_dir_all(&root);
    }
}

