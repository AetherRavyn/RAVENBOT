//! One way for a skill to run a program.
//!
//! Every tool that shells out used to build its own
//! `Command::new("sh").args(["-c", …])`, which meant each one independently
//! decided whether to sandbox, how to quote, and what the working directory
//! was — and most of them got it wrong. `git` and `code_edit` ignored the
//! sandbox entirely; `package_manager` and `task_runner` took a raw
//! `cwd` string from the model.
//!
//! This module is the seam. A skill asks for a program and its arguments, and
//! gets back a process that:
//!
//! - runs under the [`SandboxRunner`] for the context's tier, with the
//!   workspace roots bound read-write, and
//! - runs with a working directory that went through
//!   [`SkillContext::resolve_path`], so it is inside the office or refused.
//!
//! Programs are invoked as an argv, never through a shell. That removes shell
//! quoting as a class of bug entirely: a commit message containing a quote is
//! just a commit message. The one place a shell string is still needed is
//! `shell_exec`, which exists to run a command the model wrote on purpose.

use crate::traits::{SkillContext, SkillError};
use ravenbot_sandbox::SandboxRunner;
use std::path::{Path, PathBuf};
use std::process::Output;

/// Run `program args…` for this run, sandboxed and confined.
///
/// `cwd` is a model-supplied path and may be relative; it is resolved through
/// the context, so a value like `../../etc` is refused rather than followed.
/// `None` uses the office root.
pub async fn run(
    ctx: &SkillContext,
    program: &str,
    args: &[&str],
    cwd: Option<&str>,
) -> Result<Output, SkillError> {
    let dir = resolve_cwd(ctx, cwd)?;
    let runner = runner_for(ctx);
    let argv: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    runner
        .command(program, &argv, Some(&dir))
        .map_err(SkillError::Execution)?
        .output()
        .await
        .map_err(|e| SkillError::Io(e.to_string()))
}

/// The working directory for a command, as a path that exists.
///
/// An explicit `cwd` is resolved and confined. Without one, the office root is
/// used; if that has been deleted, `create_dir_all` puts it back, because a
/// tool that fails with "no such directory" tells the agent nothing about what
/// went wrong.
pub fn resolve_cwd(ctx: &SkillContext, cwd: Option<&str>) -> Result<PathBuf, SkillError> {
    let dir = match cwd.map(str::trim).filter(|c| !c.is_empty()) {
        Some(raw) => ctx.resolve_path(raw)?,
        None => ctx
            .primary_dir()
            .map(Path::to_path_buf)
            .or_else(|| {
                // Unconfined contexts have no root; fall back to the process
                // directory the way `resolve_path` does.
                std::env::current_dir().ok()
            })
            .ok_or_else(|| {
                SkillError::Execution("No working directory to run in.".into())
            })?,
    };

    if !dir.exists() {
        // Only recreate a directory we are allowed to be in.
        if ctx.contains(&dir) {
            let _ = std::fs::create_dir_all(&dir);
        }
    }
    Ok(dir)
}

/// A sandbox runner bound to this run's workspace roots.
pub fn runner_for(ctx: &SkillContext) -> SandboxRunner {
    SandboxRunner::from_tier_with_paths(ctx.sandbox_tier.clone(), &ctx.working_dirs)
}

/// A sandbox runner for `tier`, with `paths` bound read-write.
///
/// For the one skill that resolves its own tier from the environment
/// (`shell_exec`) rather than taking the context's.
pub fn runner_from_tier(tier: ravenbot_core::SandboxTier, paths: &[PathBuf]) -> SandboxRunner {
    SandboxRunner::from_tier_with_paths(tier, paths)
}

/// Turn a command's output into a skill result, keeping the failure honest.
///
/// A non-zero exit is a real result, not a skill failure: `git diff` against a
/// dirty tree or `grep` with no matches exits non-zero, and reporting that as
/// an error would train agents to distrust the tool. The exit code travels in
/// the payload so the model can see it.
pub fn output_to_result(action: &str, out: &Output) -> crate::traits::SkillResult {
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    crate::traits::SkillResult::success(serde_json::json!({
        "action": action,
        "stdout": stdout,
        "stderr": stderr,
        "success": out.status.success(),
        "exit_code": out.status.code(),
    }))
}

/// Reject a unified diff whose targets leave the workspace.
///
/// `patch` resolves the filenames in the diff itself, so the directory the
/// command runs in says nothing about where the patch lands. The only defence
/// is to read the diff and check every file it touches.
///
/// Rather than model how `patch -p1` counts path components — which quietly
/// turns `/etc/shadow` into the relative `etc/shadow`, so a check that mirrors
/// it accepts a diff it should not — this requires the conventional git form
/// and rejects anything else outright. A diff that names its files as
/// `a/…` and `b/…` relative to the office root is the only shape accepted:
///
/// - an absolute target is refused;
/// - any `..` component is refused;
/// - what is left must resolve inside the workspace.
///
/// A line that cannot be read as a target is not treated as harmless, because
/// a target we cannot vouch for is a target we cannot allow.
pub fn validate_patch_targets(ctx: &SkillContext, patch: &str) -> Result<(), SkillError> {
    for line in patch.lines() {
        let Some(rest) = line.strip_prefix("--- ").or_else(|| line.strip_prefix("+++ ")) else {
            continue;
        };
        // Strip the trailing timestamp column git adds.
        let target = rest.split('\t').next().unwrap_or(rest).trim();
        if target.is_empty() || target == "/dev/null" {
            continue;
        }

        // Refuse before touching anything. An earlier version normalised the
        // target first, and because `Path` components of `/etc/shadow` are
        // `["", "etc", "shadow"]`, dropping the empty leading component turned
        // an absolute path into a relative one that resolved inside the
        // office and was accepted. Checking absoluteness on the raw string
        // means no amount of clever rewriting can hide it.
        if Path::new(target).is_absolute()
            || target.starts_with('\\')
            || target.contains('\\')
            || target.split('/').any(|part| part == "..")
        {
            return Err(SkillError::PermissionDenied(format!(
                "This patch names '{}', which is not a path inside the office. \
                 Use unified-diff headers relative to the workspace root, as \
                 `--- a/deliverables/file.md` and `+++ b/deliverables/file.md`.",
                target
            )));
        }

        // `a/x` and `b/x` name the same file, so one leading component goes.
        let relative = target
            .strip_prefix("a/")
            .or_else(|| target.strip_prefix("b/"))
            .unwrap_or(target);
        if relative.is_empty() {
            continue;
        }

        if let Err(e) = ctx.resolve_path(relative) {
            return Err(SkillError::PermissionDenied(format!(
                "This patch touches '{}', which is outside the workspace. {}",
                target, e
            )));
        }
    }
    Ok(())
}

/// Quote a value for interpolation into a `sh -c` string.
///
/// Only for skills that genuinely need a shell pipeline. Anything taking a
/// structured command should use [`run`] with an argv instead.
pub fn shell_quote(value: &str) -> String {
    // `'` closes the quoted string, `\'` reopens it, and a literal quote is
    // `'"'"'`. Order matters: the backslash form cannot appear inside single
    // quotes at all, which is why the sequence is written as close-escape-
    // literal-close.
    format!("'{}'", value.replace('\'', r#"'"'"'"#))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use uuid::Uuid;

    fn sandbox() -> PathBuf {
        let root = std::env::temp_dir().join(format!("rb-exec-{}", Uuid::new_v4()));
        let office = root.join("office");
        std::fs::create_dir_all(office.join("src")).unwrap();
        std::fs::write(office.join("src/a.txt"), "hello").unwrap();
        root
    }

    fn ctx(root: &Path) -> SkillContext {
        SkillContext::with_default_tier(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4())
            .with_working_dirs(vec![root.join("office")])
    }

    #[test]
    fn shell_quote_survives_a_quote_in_the_value() {
        assert_eq!(shell_quote("plain"), "'plain'");
        let quoted = shell_quote("it's; rm -rf /");
        // The dangerous part is inside single quotes, so a shell reads it whole.
        assert!(quoted.starts_with('\'') && quoted.ends_with('\''));
        assert!(!quoted[1..quoted.len() - 1].contains("'; rm"), "leaked a quote");
    }

    #[test]
    fn a_cwd_outside_the_workspace_is_refused() {
        let root = sandbox();
        let ctx = ctx(&root);
        let err = resolve_cwd(&ctx, Some("/etc")).unwrap_err();
        assert!(err.to_string().contains("outside"), "{err}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_relative_cwd_cannot_climb_out() {
        let root = sandbox();
        let ctx = ctx(&root);
        assert!(resolve_cwd(&ctx, Some("../")).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_inside_cwd_resolves() {
        let root = sandbox();
        let ctx = ctx(&root);
        let dir = resolve_cwd(&ctx, Some("src")).unwrap();
        assert!(dir.exists());
        assert!(dir.ends_with("office/src"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_office_root_is_recreated_rather_than_failing() {
        let root = sandbox();
        let ctx = ctx(&root);
        assert!(resolve_cwd(&ctx, Some("gone")).unwrap().is_dir());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_patch_inside_the_workspace_is_accepted() {
        let root = sandbox();
        let ctx = ctx(&root);
        let patch = "--- a/src/a.txt\n+++ b/src/a.txt\n@@ -1 +1 @@\n-hello\n+world\n";
        assert!(validate_patch_targets(&ctx, patch).is_ok());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_patch_escaping_via_dotdot_is_rejected() {
        let root = sandbox();
        let ctx = ctx(&root);
        let patch = "--- a/../../etc/passwd\n+++ b/../../etc/passwd\n@@ -1 +1 @@\n-a\n+b\n";
        let err = validate_patch_targets(&ctx, patch).unwrap_err();
        assert!(err.to_string().contains("not a path inside the office"), "{err}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_patch_naming_an_absolute_path_is_rejected() {
        // The regression this guards: `/etc/shadow` splits into components
        // `["", "etc", "shadow"]`, so normalising away the empty leading part
        // used to yield the relative `etc/shadow` and pass the check.
        let root = sandbox();
        let ctx = ctx(&root);
        for patch in [
            "--- /etc/shadow\n+++ /etc/shadow\n@@ -1 +1 @@\n-a\n+b\n",
            "--- a//etc/shadow\n+++ b//etc/shadow\n@@ -1 +1 @@\n-a\n+b\n",
        ] {
            assert!(
                validate_patch_targets(&ctx, patch).is_err(),
                "accepted: {patch}"
            );
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_windows_style_target_is_rejected() {
        let root = sandbox();
        let ctx = ctx(&root);
        let patch = "--- ..\\..\\windows\\system32\\config\n+++ ..\\..\\windows\n@@ -1 +1 @@\n-a\n+b\n";
        assert!(validate_patch_targets(&ctx, patch).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_creation_patch_with_dev_null_is_allowed() {
        let root = sandbox();
        let ctx = ctx(&root);
        let patch = "--- /dev/null\n+++ b/src/new.rs\n@@ -0,0 +1 @@\n+fn main() {}\n";
        assert!(validate_patch_targets(&ctx, patch).is_ok());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_runner_binds_the_workspace_roots() {
        let root = sandbox();
        let ctx = ctx(&root);
        let runner = runner_for(&ctx);
        let bound: Vec<String> = runner
            .config()
            .allowed_paths
            .iter()
            .map(|p| Path::new(p).display().to_string())
            .collect();
        assert!(
            bound.iter().any(|p| p.ends_with("office")),
            "office root not bound: {bound:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
