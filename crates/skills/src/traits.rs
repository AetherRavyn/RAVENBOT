//! Skill trait and types

use async_trait::async_trait;
use ravenbot_core::Permission;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;
use uuid::Uuid;

/// Errors from skill execution
#[derive(Error, Debug)]
pub enum SkillError {
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    #[error("Execution error: {0}")]
    Execution(String),
    #[error("Invalid arguments: {0}")]
    InvalidArguments(String),
    #[error("Network error: {0}")]
    Network(String),
    #[error("IO error: {0}")]
    Io(String),
}

/// Result of a skill execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillResult {
    /// Whether the execution was successful
    pub success: bool,
    /// Output or result data
    pub output: serde_json::Value,
    /// Optional error message
    pub error: Option<String>,
}

impl SkillResult {
    /// Create a successful result
    pub fn success(output: serde_json::Value) -> Self {
        Self {
            success: true,
            output,
            error: None,
        }
    }

    /// Create a failure result
    pub fn failure(error: impl Into<String>) -> Self {
        let error = error.into();
        Self {
            success: false,
            output: serde_json::json!({ "error": &error }),
            error: Some(error),
        }
    }
}

/// Context for skill execution
pub struct SkillContext {
    /// Bot ID executing the skill
    pub bot_id: Uuid,
    /// Run ID for this execution
    pub run_id: Uuid,
    /// Thread ID for conversation context
    pub thread_id: Uuid,
    /// Isolation tier the bot runs under (drives shell/process sandboxing).
    pub sandbox_tier: ravenbot_core::SandboxTier,
    /// Project folders this run is allowed to work in. The first is the cwd.
    pub working_dirs: Vec<std::path::PathBuf>,
    /// Whether paths must stay inside `working_dirs`.
    ///
    /// True whenever folders are configured, so an office run is confined even
    /// if every root has since been deleted. It can also be set with no roots
    /// at all, which denies every path — the fail-closed answer to "this run
    /// should be confined but nobody worked out where".
    pub confined: bool,
}

impl SkillContext {
    /// Context for a bot with an explicit sandbox tier.
    pub fn new(bot_id: Uuid, run_id: Uuid, thread_id: Uuid, sandbox_tier: ravenbot_core::SandboxTier) -> Self {
        Self {
            bot_id,
            run_id,
            thread_id,
            sandbox_tier,
            working_dirs: Vec::new(),
            confined: false,
        }
    }

    /// Attach the project folders this run may work in.
    ///
    /// Configuring folders turns on confinement. A caller that wants an
    /// explicitly confined run with *no* folders uses [`confined_without_roots`]
    /// instead, because the empty list is ambiguous on its own.
    pub fn with_working_dirs(mut self, dirs: Vec<std::path::PathBuf>) -> Self {
        if !dirs.is_empty() {
            self.confined = true;
        }
        self.working_dirs = dirs;
        self
    }

    /// A run that may not touch any path, because no workspace was resolved.
    ///
    /// Every path is denied rather than falling through to the process
    /// directory. A missing workspace is a bug worth surfacing; silently
    /// reading the working directory of whatever launched the app is worse.
    pub fn confined_without_roots(
        bot_id: Uuid,
        run_id: Uuid,
        thread_id: Uuid,
        sandbox_tier: ravenbot_core::SandboxTier,
    ) -> Self {
        Self {
            bot_id,
            run_id,
            thread_id,
            sandbox_tier,
            working_dirs: Vec::new(),
            confined: true,
        }
    }

    /// Mark this run as confined, whether or not any roots resolved.
    ///
    /// Distinct from [`with_working_dirs`], which only turns confinement on
    /// when the list is non-empty. A caller that has decided the run must not
    /// leave its workspace says so here, so "the roots came back empty"
    /// resolves to denying every path rather than to no boundary.
    pub fn confined(mut self) -> Self {
        self.confined = true;
        self
    }

    /// Context with the default (OS-level) tier — for tests and headless paths.
    pub fn with_default_tier(bot_id: Uuid, run_id: Uuid, thread_id: Uuid) -> Self {
        Self::new(bot_id, run_id, thread_id, ravenbot_core::SandboxTier::OsLevel)
    }

    /// The primary working directory (first configured project folder).
    pub fn primary_dir(&self) -> Option<&std::path::Path> {
        self.working_dirs.first().map(|p| p.as_path())
    }

    /// Whether `path` is inside one of the configured roots.
    ///
    /// The comparison is lexical on already-normalized inputs: canonicalize
    /// first, then `starts_with`, which is component-wise. That is why
    /// `/home/u/office-evil` is *not* inside `/home/u/office`.
    pub fn contains(&self, path: &std::path::Path) -> bool {
        self.working_dirs.iter().any(|root| path.starts_with(root))
    }

    /// Resolve a tool-supplied path against the project folders.
    ///
    /// Three steps, in order, because each closes a hole the previous one
    /// leaves:
    ///
    /// 1. Resolve `..` and `.` **lexically**. This is what stops
    ///    `../../etc/passwd` — as a relative path it skips the absolute check
    ///    entirely, is joined onto the project root, and used unverified.
    /// 2. Canonicalize the deepest part of the path that exists, so a symlink
    ///    inside the workspace cannot point out of it. For a path that does not
    ///    exist yet — a file about to be written — the symlinked *parent* is
    ///    resolved instead, or `link_to_etc/newfile` would slip through.
    /// 3. Re-check containment on the real path, not the one we were handed.
    ///
    /// Returns the resolved path, which is what the skill should use. Callers
    /// that ignore it and touch `raw` directly have not been confined.
    pub fn resolve_path(&self, raw: &str) -> Result<std::path::PathBuf, SkillError> {
        let requested = std::path::Path::new(raw);

        if !self.confined {
            // Unconfined: the process directory is the base, as documented.
            return Ok(match self.primary_dir() {
                Some(root) => root.join(requested),
                None => std::env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .join(requested),
            });
        }

        if self.working_dirs.is_empty() {
            return Err(SkillError::PermissionDenied(format!(
                "Cannot access '{}': this run has no workspace, so every path is denied.",
                raw
            )));
        }

        // Absolute paths are taken as given (then normalized); relative ones
        // are anchored to the primary root, which is always inside it.
        let candidate = if requested.is_absolute() {
            requested.to_path_buf()
        } else {
            match self.primary_dir() {
                Some(root) => root.join(requested),
                None => requested.to_path_buf(),
            }
        };

        let resolved = resolve_real_path(&candidate);

        if self.contains(&resolved) {
            Ok(resolved)
        } else {
            Err(SkillError::PermissionDenied(format!(
                "Path '{}' resolves to '{}', which is outside this office's workspace ({}). \
                 Work inside the office folder, or ask the client for a different workspace.",
                raw,
                resolved.display(),
                self.roots_joined()
            )))
        }
    }

    /// A copy of this context for a skill that delegates to another skill.
    ///
    /// Carries the confinement, so the delegated call is bound by the same
    /// workspace as the original instead of silently becoming unconfined.
    pub fn clone_for_delegation(&self) -> SkillContext {
        SkillContext {
            bot_id: self.bot_id,
            run_id: self.run_id,
            thread_id: self.thread_id,
            sandbox_tier: self.sandbox_tier.clone(),
            working_dirs: self.working_dirs.clone(),
            confined: self.confined,
        }
    }

    /// The configured roots, for an error message that tells the agent where it
    /// *is* allowed to go.
    fn roots_joined(&self) -> String {
        self.working_dirs
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Collapse `.` and `..` without touching the filesystem.
///
/// `..` is resolved against the accumulated components, so `a/b/../c` becomes
/// `a/c` and `../../x` at the top becomes `x` rather than escaping above the
/// root. An absolute path keeps its prefix and root; a relative path with more
/// `..` than components keeps the leading `..` so the caller's join still lands
/// somewhere real and the containment check decides.
fn lexically_normalize(path: &std::path::Path) -> std::path::PathBuf {
    use std::path::Component;
    let mut out: Vec<Component> = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match out.last() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                Some(Component::RootDir) | Some(Component::Prefix(_)) => {}
                // A relative path that walks above its own start keeps the
                // `..`; there is nothing to pop.
                _ => out.push(component),
            },
            other => out.push(other),
        }
    }
    if out.is_empty() {
        return PathBuf::from(".");
    }
    out.iter().collect()
}

/// Resolve symlinks as far as the filesystem allows, then re-attach the parts
/// that do not exist yet.
///
/// Canonicalizing a whole path fails when a leaf is missing, which is the
/// normal case for a file about to be written. Walking up to the deepest
/// existing ancestor, canonicalizing that, and rejoining the tail resolves a
/// symlinked parent while still working for a new file.
fn resolve_real_path(path: &std::path::Path) -> PathBuf {
    let normalized = lexically_normalize(path);
    if let Ok(real) = normalized.canonicalize() {
        return real;
    }

    let mut base = normalized.as_path();
    let mut tail: Vec<&std::ffi::OsStr> = Vec::new();
    loop {
        let Some(parent) = base.parent() else {
            break;
        };
        let Some(name) = base.file_name() else {
            break;
        };
        tail.push(name);
        base = parent;
        if base.as_os_str().is_empty() {
            break;
        }
        if let Ok(real) = base.canonicalize() {
            let mut out = real;
            for part in tail.iter().rev() {
                out.push(part);
            }
            return out;
        }
    }
    // Nothing along the path exists. The lexical form is all we have; the
    // containment check still runs on it.
    normalized
}

/// Category distinguishing how a skill is consumed by the model.
///
/// - `Tool`: A function-calling tool. Emitted in the tool definitions list,
///   counted toward the tool cap, and invoked via the model's tool-call mechanism.
/// - `Workflow`: A prompt-based workflow guide. Its instructions are injected
///   into the system prompt so the LLM follows it as a multi-turn process.
///   NOT counted toward the tool cap and NOT exposed as a function call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillKind {
    Tool,
    Workflow,
}

/// Risk level of a skill — drives the approval gate ("bots ask before they
/// act"). Read-only skills run silently in `ask` mode; anything that
/// mutates state, touches the network beyond search, or shells out pauses
/// for an Allow/Deny decision first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillRisk {
    /// Read-only: file_read, search, recall, analysis — never gated.
    ReadOnly,
    /// Low-stakes writes: memory_save, notes, todos — gated in `ask` mode.
    Low,
    /// High-stakes: shell, file writes/edits, git, docker, deploy-ish MCP —
    /// always gated unless the bot is in `full` mode.
    High,
}

impl SkillRisk {
    pub fn as_str(&self) -> &'static str {
        match self {
            SkillRisk::ReadOnly => "read_only",
            SkillRisk::Low => "low",
            SkillRisk::High => "high",
        }
    }
}

/// The core skill trait that all skills must implement
#[async_trait]
pub trait Skill: Send + Sync {
    /// Unique identifier for the skill
    fn id(&self) -> &str;

    /// Human-readable name
    fn name(&self) -> &str;

    /// Description of what the skill does
    fn description(&self) -> &str;

    /// Version of the skill
    fn version(&self) -> &str;

    /// Permissions required by this skill
    fn required_permissions(&self) -> Vec<Permission>;

    /// JSON schema for the skill's input arguments
    fn input_schema(&self) -> serde_json::Value;

    /// Whether this skill is a function-calling tool or a workflow guide.
    /// Default is `Tool`. Prompt-based skills override this to `Workflow`.
    fn kind(&self) -> SkillKind { SkillKind::Tool }

    /// Risk level for the approval gate. Defaults to `High` (fail-closed:
    /// unknown skills ask, they don't run silent). Read-only skills override
    /// to `ReadOnly`.
    fn risk(&self) -> SkillRisk { SkillRisk::High }

    /// For workflow skills: return the full prompt/instructions that should
    /// be injected into the system prompt. Default returns None.
    fn prompt(&self) -> Option<&str> { None }

    /// Execute the skill with the given arguments
    async fn execute(
        &self,
        context: &SkillContext,
        arguments: serde_json::Value,
    ) -> Result<SkillResult, SkillError>;

    /// Validate arguments without executing
    fn validate_arguments(&self, arguments: &serde_json::Value) -> Result<(), SkillError> {
        let schema = self.input_schema();
        // Basic validation - check required fields exist
        if let Some(required) = schema.get("required") {
            if let Some(fields) = required.as_array() {
                for field in fields {
                    if let Some(field_name) = field.as_str() {
                        if arguments.get(field_name).is_none() {
                            return Err(SkillError::InvalidArguments(
                                format!("Missing required field: {}", field_name)
                            ));
                        }
                    }
                }
            }
        }
        Ok(())
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    /// A real directory tree, because the symlink and `..` cases cannot be
    /// tested against paths that do not exist.
    struct Sandbox {
        root: PathBuf,
    }

    impl Sandbox {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("rb-confine-{}", Uuid::new_v4()));
            let office = root.join("office");
            for d in [office.join("deliverables"), office.join("Shared"), root.join("outside")] {
                std::fs::create_dir_all(&d).unwrap();
            }
            std::fs::write(office.join("deliverables/ok.txt"), "fine").unwrap();
            std::fs::write(root.join("outside/secret.txt"), "hunter2").unwrap();
            Self { root }
        }

        fn office(&self) -> PathBuf {
            self.root.join("office")
        }

        fn ctx(&self) -> SkillContext {
            SkillContext::with_default_tier(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4())
                .with_working_dirs(vec![self.office()])
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn relative_paths_resolve_into_the_project_folder() {
        let s = Sandbox::new();
        let ctx = s.ctx();
        let resolved = ctx.resolve_path("deliverables/ok.txt").unwrap();
        assert!(resolved.ends_with("office/deliverables/ok.txt"), "{}", resolved.display());
        // It comes back real, not as a bare join, so a skill can use it.
        assert!(resolved.exists());
    }

    #[test]
    fn absolute_paths_inside_the_office_are_allowed() {
        let s = Sandbox::new();
        let target = s.office().join("deliverables/ok.txt");
        assert_eq!(s.ctx().resolve_path(target.to_str().unwrap()).unwrap(), target);
    }

    #[test]
    fn absolute_paths_outside_the_office_are_rejected() {
        let s = Sandbox::new();
        let ctx = s.ctx();
        let secret = s.root.join("outside/secret.txt");
        let err = ctx.resolve_path(secret.to_str().unwrap()).unwrap_err();
        assert!(err.to_string().contains("outside this office's workspace"), "{err}");
        // The message names where the agent *can* work, so it can correct itself.
        assert!(err.to_string().contains("office"), "{err}");
    }

    #[test]
    fn a_relative_dotdot_cannot_climb_out() {
        // The hole this closes: a relative path skips the absolute check, is
        // joined onto the root, and used unverified.
        let s = Sandbox::new();
        let ctx = s.ctx();
        let secret = s.root.join("outside/secret.txt");
        let relative = format!("../outside/secret.txt");
        assert!(ctx.resolve_path(&relative).is_err(), "escaped via {relative}");

        let deeper = format!("deliverables/../../../outside/secret.txt");
        assert!(ctx.resolve_path(&deeper).is_err(), "escaped via {deeper}");
        assert!(secret.exists(), "the file must still be there");
    }

    #[test]
    fn an_absolute_dotdot_cannot_climb_out() {
        let s = Sandbox::new();
        let ctx = s.ctx();
        let sneaky = format!("{}/../outside/secret.txt", s.office().display());
        assert!(ctx.resolve_path(&sneaky).is_err(), "escaped via {sneaky}");
    }

    #[test]
    fn a_dotdot_that_stays_inside_still_works() {
        let s = Sandbox::new();
        let ctx = s.ctx();
        let inside = "deliverables/../Shared/scratch.md";
        let resolved = ctx.resolve_path(inside).unwrap();
        assert!(resolved.ends_with("office/Shared/scratch.md"), "{}", resolved.display());
    }

    #[test]
    fn a_symlink_out_of_the_office_is_followed_then_rejected() {
        let s = Sandbox::new();
        let link = s.office().join("escape");
        std::os::unix::fs::symlink(s.root.join("outside"), &link).unwrap();
        let ctx = s.ctx();

        assert!(ctx.resolve_path("escape/secret.txt").is_err(), "symlinked dir escaped");
        assert!(ctx
            .resolve_path(link.join("secret.txt").to_str().unwrap())
            .is_err());
    }

    #[test]
    fn a_symlinked_parent_is_rejected_for_a_file_that_does_not_exist_yet() {
        // The subtle one: the leaf does not exist, so a plain canonicalize
        // fails and the symlinked parent would be used unverified.
        let s = Sandbox::new();
        let link = s.office().join("escape");
        std::os::unix::fs::symlink(s.root.join("outside"), &link).unwrap();
        let ctx = s.ctx();

        assert!(
            ctx.resolve_path("escape/brand-new.txt").is_err(),
            "a new file under a symlinked parent escaped"
        );
        assert!(!s.root.join("outside/brand-new.txt").exists());
    }

    #[test]
    fn a_symlink_that_stays_inside_is_allowed() {
        let s = Sandbox::new();
        let link = s.office().join("shortcut");
        std::os::unix::fs::symlink(s.office().join("deliverables"), &link).unwrap();
        let resolved = s.ctx().resolve_path("shortcut/ok.txt").unwrap();
        assert!(resolved.exists(), "{}", resolved.display());
    }

    #[test]
    fn a_sibling_with_a_shared_prefix_is_not_inside() {
        // `starts_with` on a Path is component-wise, so this must not pass.
        let s = Sandbox::new();
        let evil = s.root.join("office-evil");
        std::fs::create_dir_all(&evil).unwrap();
        assert!(s.ctx().resolve_path(evil.join("x").to_str().unwrap()).is_err());
    }

    #[test]
    fn a_confined_run_with_no_workspace_denies_everything() {
        let ctx = SkillContext::confined_without_roots(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            ravenbot_core::SandboxTier::OsLevel,
        );
        let err = ctx.resolve_path("/etc/passwd").unwrap_err();
        assert!(err.to_string().contains("no workspace"), "{err}");
        assert!(ctx.resolve_path("anything").is_err());
    }

    #[test]
    fn an_unconfined_run_still_allows_absolute_paths() {
        // The escape hatch kept for tests and the CLI: no folders, no boundary.
        let ctx = SkillContext::with_default_tier(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        assert!(ctx.resolve_path("/etc/hosts").is_ok());
        assert!(ctx.primary_dir().is_none());
        assert!(!ctx.confined);
    }

    #[test]
    fn several_roots_are_all_reachable_and_the_first_is_the_cwd() {
        let s = Sandbox::new();
        let shared = s.office().join("Shared");
        let ctx = s.ctx().with_working_dirs(vec![s.office(), shared.clone()]);
        assert_eq!(ctx.primary_dir(), Some(s.office().as_path()));
        assert!(ctx.resolve_path("deliverables/ok.txt").is_ok());
        // The second root is a valid target even though it is not the cwd.
        let scratch = format!("{}/note.md", shared.display());
        assert!(ctx.resolve_path(&scratch).is_ok());
    }

    #[test]
    fn lexical_normalization_is_itself_correct() {
        use std::path::Path;
        assert_eq!(lexically_normalize(Path::new("/a/b/../c")), PathBuf::from("/a/c"));
        assert_eq!(lexically_normalize(Path::new("/a/./b/")), PathBuf::from("/a/b"));
        // Cannot climb above an absolute root.
        assert_eq!(lexically_normalize(Path::new("/../etc")), PathBuf::from("/etc"));
        assert_eq!(lexically_normalize(Path::new("/a/b/../..")), PathBuf::from("/"));
        // A relative path that walks above its start keeps the `..`.
        assert_eq!(lexically_normalize(Path::new("../x")), PathBuf::from("../x"));
        assert_eq!(lexically_normalize(Path::new("")), PathBuf::from("."));
    }

    #[test]
    fn resolve_real_path_handles_a_path_that_does_not_exist() {
        let s = Sandbox::new();
        let missing = s.office().join("deliverables/not/created/yet.txt");
        let resolved = resolve_real_path(&missing);
        assert!(resolved.ends_with("deliverables/not/created/yet.txt"), "{}", resolved.display());
        assert!(!resolved.exists());
        // The existing prefix is still real, not a string join.
        assert!(resolved.starts_with(&s.office()));
    }
}
