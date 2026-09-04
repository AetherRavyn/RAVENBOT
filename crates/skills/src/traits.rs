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
