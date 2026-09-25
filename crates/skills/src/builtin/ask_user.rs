//! ask_user — human-in-the-loop question card.
//!
//! The runtime intercepts this tool by name and parks the run on a question
//! card (see `Runtime::ask_user`), feeding the answer back as the tool result.
//! This skill exists so the model sees `ask_user` in its tool list and gets a
//! sensible fallback if it is ever executed outside the runtime.

use async_trait::async_trait;
use ravenbot_core::Permission;
use crate::traits::{Skill, SkillContext, SkillError, SkillResult, SkillRisk};

pub struct AskUserSkill;

impl AskUserSkill {
    pub fn new() -> Self { Self }
}

#[async_trait]
impl Skill for AskUserSkill {
    fn id(&self) -> &str { "ask_user" }
    fn name(&self) -> &str { "Ask the user" }
    fn description(&self) -> &str {
        "Ask the user a clarifying question mid-task and wait for their answer. \
         Use when you are genuinely blocked on a decision only the user can make. \
         Provide an optional short header and a list of suggested options."
    }
    fn version(&self) -> &str { "1.0.0" }
    fn required_permissions(&self) -> Vec<Permission> { vec![] }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "question": { "type": "string", "description": "The question to ask the user" },
                "header": { "type": "string", "description": "Short card title (e.g. 'Which database?')" },
                "options": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Suggested answers rendered as buttons"
                },
                "allow_custom": {
                    "type": "boolean",
                    "description": "Whether a free-text answer is allowed (default true)"
                }
            },
            "required": ["question"]
        })
    }
    fn risk(&self) -> SkillRisk { SkillRisk::ReadOnly }

    async fn execute(&self, _ctx: &SkillContext, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let question = args
            .get("question")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing question".into()))?;
        // Reached only when executed outside the runtime's interception path.
        Ok(SkillResult::success(serde_json::json!({
            "answer": null,
            "unavailable": true,
            "note": format!("No interactive channel is available to answer: {}", question),
        })))
    }
}

impl Default for AskUserSkill { fn default() -> Self { Self::new() } }
