//! Inter-bot delegation skill

use async_trait::async_trait;
use ravenbot_core::Permission;
use uuid::Uuid;

use crate::traits::{Skill, SkillContext, SkillError, SkillResult};

pub struct DelegateSkill;

impl DelegateSkill {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Skill for DelegateSkill {
    fn id(&self) -> &str {
        "delegate"
    }

    fn name(&self) -> &str {
        "Delegate"
    }

    fn description(&self) -> &str {
        "Delegate a task to another bot"
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn required_permissions(&self) -> Vec<Permission> {
        vec![Permission::Delegation]
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "bot_id": {
                    "type": "string",
                    "description": "ID of the bot to delegate to"
                },
                "instruction": {
                    "type": "string",
                    "description": "The instruction to give the bot"
                },
                "context": {
                    "type": "string",
                    "description": "Additional context for the task"
                }
            },
            "required": ["bot_id", "instruction"]
        })
    }

    async fn execute(
        &self,
        context: &SkillContext,
        arguments: serde_json::Value,
    ) -> Result<SkillResult, SkillError> {
        let bot_id_str = arguments
            .get("bot_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'bot_id' field".to_string()))?;

        let bot_id = Uuid::parse_str(bot_id_str)
            .map_err(|e| SkillError::InvalidArguments(format!("Invalid bot_id: {}", e)))?;

        let instruction = arguments
            .get("instruction")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'instruction' field".to_string()))?;

        let _context_text = arguments
            .get("context")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // In a full implementation, this would:
        // 1. Look up the target bot
        // 2. Create a new thread with the target bot
        // 3. Send the instruction as a message
        // 4. Wait for and return the response

        // For now, return a placeholder indicating delegation was initiated
        tracing::info!(
            from_bot = %context.bot_id,
            to_bot = %bot_id,
            instruction = instruction,
            "Delegation initiated"
        );

        Ok(SkillResult::success(serde_json::json!({
            "status": "delegation_initiated",
            "target_bot_id": bot_id_str,
            "instruction": instruction,
            "message": format!("Task delegated to bot {}. In a full implementation, this would execute the task and return the result.", bot_id_str)
        })))
    }
}

impl Default for DelegateSkill {
    fn default() -> Self {
        Self::new()
    }
}
