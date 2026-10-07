use crate::exec;
use crate::traits::{Skill, SkillContext, SkillError, SkillResult};
use async_trait::async_trait;
use ravenbot_core::Permission;

pub struct CodeEditSkill;

impl CodeEditSkill { pub fn new() -> Self { Self } }

#[async_trait]
impl Skill for CodeEditSkill {
    fn id(&self) -> &str { "code_edit" }
    fn name(&self) -> &str { "Code Edit" }
    fn description(&self) -> &str { "Apply unified diff patch to files — reviewable, git-aware" }
    fn version(&self) -> &str { "1.1.0" }
    fn required_permissions(&self) -> Vec<Permission> { vec![Permission::FileSystem { paths: vec![".".into()] }, Permission::Shell] }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({"type":"object","properties":{
            "patch":{"type":"string","description":"Unified diff"},
            "dry_run":{"type":"boolean","description":"Check the patch without applying it"},
            "path":{"type":"string","description":"Directory inside the office to apply in, relative to its root"}
        },"required":["patch"]})
    }

    /// The patch is validated before it runs, not just the cwd.
    ///
    /// `patch` resolves the filenames in the diff itself, so a patch naming
    /// `../../etc/passwd` writes there regardless of the directory it was
    /// started in — the cwd check alone proves nothing about where a patch
    /// lands. Every `---`/`+++` target goes through the confinement check
    /// first.
    async fn execute(&self, ctx: &SkillContext, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let patch = args
            .get("patch")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing patch".into()))?;
        let path = args.get("path").and_then(|v| v.as_str());
        let dry = args.get("dry_run").and_then(|v| v.as_bool()).unwrap_or(false);

        exec::validate_patch_targets(ctx, patch)?;

        // The scratch file lives in the run's own temp directory rather than
        // a predictable `/tmp` name, so a symlink planted there cannot make
        // the patch overwrite something else. Under bubblewrap `/tmp` is a
        // private tmpfs, which is why the sandbox binds it separately.
        let tmp = std::env::temp_dir()
            .join(format!("raven_patch_{}.diff", uuid::Uuid::new_v4()));
        tokio::fs::write(&tmp, patch)
            .await
            .map_err(|e| SkillError::Io(e.to_string()))?;

        let tmp_arg = tmp.to_string_lossy().to_string();
        let check = run_patch(ctx, &["--dry-run", "-p1"], &tmp_arg, path).await;
        let check = match check {
            Ok(out) => out,
            Err(e) => {
                let _ = tokio::fs::remove_file(&tmp).await;
                return Err(e);
            }
        };
        let check_out = combined(&check);

        if dry {
            let _ = tokio::fs::remove_file(&tmp).await;
            return Ok(SkillResult::success(serde_json::json!({
                "dry_run": true,
                "check": check_out,
            })));
        }
        if !check.status.success() {
            let _ = tokio::fs::remove_file(&tmp).await;
            return Ok(SkillResult::failure(format!(
                "Patch check failed, nothing was changed:\n{check_out}"
            )));
        }

        let applied = run_patch(ctx, &["-p1"], &tmp_arg, path).await;
        let _ = tokio::fs::remove_file(&tmp).await;
        let out = match applied {
            Ok(o) => combined(&o),
            Err(e) => {
                return Ok(SkillResult::failure(format!(
                    "Patch passed the check but could not be applied: {e}"
                )))
            }
        };
        // Per-file, not one total: a patch touching four files is only useful
        // split, because "changed 40 lines" does not say which file to look at.
        // The patch text comes along with it so the ledger can show the hunks,
        // not only their arithmetic.
        let file_changes: Vec<serde_json::Value> = crate::diff::patch_file_diffs(patch)
            .into_iter()
            .map(|p| {
                serde_json::json!({
                    "path": p.path,
                    "lines_added": p.added,
                    "lines_deleted": p.deleted,
                    "diff": p.text,
                })
            })
            .collect();

        Ok(SkillResult::success(serde_json::json!({
            "applied": true,
            "output": out,
            "check": check_out,
            "file_changes": file_changes,
        })))
    }
}

async fn run_patch(
    ctx: &SkillContext,
    flags: &[&str],
    patch_file: &str,
    path: Option<&str>,
) -> Result<std::process::Output, SkillError> {
    // `sh -c` with a redirect: this is the one place a shell is genuinely
    // needed, because `patch` reads the diff on stdin. The filename is
    // quoted, and it is our own generated path rather than model input.
    let script = format!(
        "patch {} < {} 2>&1",
        flags.join(" "),
        exec::shell_quote(patch_file)
    );
    let dir = exec::resolve_cwd(ctx, path)?;
    exec::runner_for(ctx)
        .command("sh", &["-c".to_string(), script], Some(&dir))
        .map_err(SkillError::Execution)?
        .output()
        .await
        .map_err(|e| SkillError::Io(e.to_string()))
}

fn combined(out: &std::process::Output) -> String {
    let mut s = String::from_utf8_lossy(&out.stdout).to_string();
    s.push_str(&String::from_utf8_lossy(&out.stderr));
    s
}

impl Default for CodeEditSkill { fn default() -> Self { Self::new() } }

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
        let root = std::env::temp_dir().join(format!("rb-edit-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.txt"), "one\n").unwrap();
        root
    }

    #[test]
    fn a_patch_escaping_the_office_is_refused_before_it_runs() {
        let root = sandbox();
        let skill = CodeEditSkill::new();
        let patch = "--- a/../../../etc/passwd\n+++ b/../../../etc/passwd\n@@ -1 +1 @@\n-a\n+b\n";
        let err = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(skill.execute(&ctx(&root), serde_json::json!({"patch": patch})))
            .unwrap_err();
        assert!(err.to_string().contains("not a path inside the office"), "{err}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_dry_run_patch_inside_the_office_reports_without_applying() {
        let root = sandbox();
        let skill = CodeEditSkill::new();
        let patch = "--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-one\n+two\n";
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(skill.execute(
                &ctx(&root),
                serde_json::json!({"patch": patch, "dry_run": true}),
            ))
            .expect("dry run should succeed");
        assert_eq!(result.output["dry_run"], true);
        // Untouched.
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "one\n");
        let _ = std::fs::remove_dir_all(&root);
    }
}
