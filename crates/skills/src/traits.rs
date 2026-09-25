//! Skill trait and types

use async_trait::async_trait;
use ravenbot_core::Permission;
use serde::{Deserialize, Serialize};
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
    /// Project folders this run is allowed to work in. When non-empty, file
    /// and shell tools are confined to these roots; the first is the cwd.
    pub working_dirs: Vec<std::path::PathBuf>,
}

impl SkillContext {
    /// Context for a bot with an explicit sandbox tier.
    pub fn new(bot_id: Uuid, run_id: Uuid, thread_id: Uuid, sandbox_tier: ravenbot_core::SandboxTier) -> Self {
        Self { bot_id, run_id, thread_id, sandbox_tier, working_dirs: Vec::new() }
    }

    /// Attach the project folders this run may work in.
    pub fn with_working_dirs(mut self, dirs: Vec<std::path::PathBuf>) -> Self {
        self.working_dirs = dirs;
        self
    }

    /// Context with the default (OS-level) tier — for tests and headless paths.
    pub fn with_default_tier(bot_id: Uuid, run_id: Uuid, thread_id: Uuid) -> Self {
        Self { bot_id, run_id, thread_id, sandbox_tier: ravenbot_core::SandboxTier::OsLevel, working_dirs: Vec::new() }
    }

    /// The primary working directory (first configured project folder).
    pub fn primary_dir(&self) -> Option<&std::path::Path> {
        self.working_dirs.first().map(|p| p.as_path())
    }

    /// Resolve a tool-supplied path against the project folders.
    ///
    /// - Absolute paths must live inside one of the configured roots (when
    ///   roots are configured); otherwise they are rejected.
    /// - Relative paths resolve against the primary working directory (or the
    ///   process cwd when no folders are configured).
    pub fn resolve_path(&self, raw: &str) -> Result<std::path::PathBuf, SkillError> {
        let path = std::path::Path::new(raw);
        if path.is_absolute() {
            if self.working_dirs.is_empty()
                || self.working_dirs.iter().any(|root| path.starts_with(root))
            {
                return Ok(path.to_path_buf());
            }
            return Err(SkillError::PermissionDenied(format!(
                "Path '{}' is outside this office's project folders",
                raw
            )));
        }
        if let Some(root) = self.primary_dir() {
            return Ok(root.join(path));
        }
        Ok(std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join(path))
    }
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
    use std::path::PathBuf;

    fn ctx_with(root: &str) -> SkillContext {
        SkillContext::with_default_tier(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4())
            .with_working_dirs(vec![PathBuf::from(root)])
    }

    #[test]
    fn relative_paths_resolve_into_the_project_folder() {
        let ctx = ctx_with("/tmp/office");
        let resolved = ctx.resolve_path("src/main.rs").unwrap();
        assert_eq!(resolved, PathBuf::from("/tmp/office/src/main.rs"));
    }

    #[test]
    fn absolute_paths_outside_the_project_are_rejected() {
        let ctx = ctx_with("/tmp/office");
        assert!(ctx.resolve_path("/etc/passwd").is_err());
        assert_eq!(
            ctx.resolve_path("/tmp/office/data.json").unwrap(),
            PathBuf::from("/tmp/office/data.json")
        );
    }

    #[test]
    fn no_folders_means_no_confinement() {
        let ctx = SkillContext::with_default_tier(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        assert!(ctx.resolve_path("/etc/hosts").is_ok());
        assert!(ctx.primary_dir().is_none());
    }
}
