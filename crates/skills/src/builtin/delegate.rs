//! Inter-bot delegation skill

use async_trait::async_trait;
use ravenbot_core::Permission;

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

    /// **Not implemented, and it fails on purpose.**
    ///
    /// The runtime intercepts `delegate` in `execute_tool_call` and runs the real
    /// `exec_delegation` before the registry is ever consulted, so this body is
    /// unreachable on the main path. It stays registered because it is what
    /// declares the tool's `Permission::Delegation` requirement and supplies the
    /// input schema the model reads — remove it and delegation silently stops
    /// being gated.
    ///
    /// It used to return `{"status": "delegation_initiated"}` with a note that a
    /// full implementation would do the work. That is the worst possible failure
    /// mode for a stub: an unreachable code path that reports success is a trap
    /// for whoever adds the path that reaches it. Any future caller — MCP, a
    /// second dispatch site, a reordering of the interceptor — would get a
    /// cheerful acknowledgement that no work was done and no agent was contacted,
    /// and the model would carry on believing the task was handed off.
    ///
    /// So it errors, and says exactly why. A stub that shouts is a bug report; a
    /// stub that lies is a silent one.
    async fn execute(
        &self,
        context: &SkillContext,
        arguments: serde_json::Value,
    ) -> Result<SkillResult, SkillError> {
        let bot_id = arguments
            .get("bot_id")
            .and_then(|v| v.as_str())
            .unwrap_or("<none>");
        Err(SkillError::Execution(format!(
            "Delegation is implemented by the runtime, not the skill registry, and \
             this entry point was reached directly — so the task was not delegated. \
             Bot {} asked bot {}. This is a bug in the dispatch order in \
             execute_tool_call, not a configuration problem: the runtime's \
             exec_delegation must intercept 'delegate' before the registry.",
            context.bot_id, bot_id
        )))
    }
}

impl Default for DelegateSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use uuid::Uuid;

    fn ctx() -> SkillContext {
        SkillContext::new(
            Uuid::nil(),
            Uuid::nil(),
            Uuid::nil(),
            ravenbot_core::SandboxTier::OsLevel,
        )
    }

    /// The stub must not report success.
    ///
    /// It used to return `{"status": "delegation_initiated"}` with a note that a
    /// full implementation would do the work. The runtime intercepts `delegate`
    /// before the registry, so that body was unreachable — which is exactly why it
    /// was dangerous. Any new dispatch path that reached it would get a cheerful
    /// acknowledgement that no work was done and no agent contacted, and the model
    /// would carry on believing the task was handed off.
    ///
    /// The assertion is that it errors *and names the cause*, because "this is not
    /// implemented" sends someone looking for a feature flag while "the dispatch
    /// order is wrong" sends them to the actual bug.
    #[tokio::test]
    async fn it_refuses_rather_than_pretending_to_delegate() {
        let err = DelegateSkill::new()
            .execute(&ctx(), json!({ "bot_id": Uuid::nil().to_string(), "instruction": "do it" }))
            .await
            .expect_err("the stub must not report success");

        let text = err.to_string();
        assert!(text.contains("was not delegated"), "unhelpful error: {text}");
        assert!(text.contains("dispatch"), "does not name the cause: {text}");
    }

    /// The schema and the permission stay, because they are the reason this entry
    /// is registered at all.
    ///
    /// The runtime's `exec_delegation` is gated on `Permission::Delegation`, read
    /// from the registry via this skill's `required_permissions`. Delete the
    /// registration and delegation stops being permission-checked — silently,
    /// with no test failing anywhere else.
    #[test]
    fn it_still_declares_the_permission_and_the_schema() {
        let skill = DelegateSkill::new();
        assert_eq!(skill.id(), "delegate");
        assert_eq!(skill.required_permissions(), vec![Permission::Delegation]);

        let schema = skill.input_schema();
        let required = schema["required"].as_array().expect("required list");
        assert!(required.iter().any(|v| v == "bot_id"));
        assert!(required.iter().any(|v| v == "instruction"));
        // `context` is optional but must be declared: the runtime forwards it, and
        // an undeclared field is a field a model has no reason to send.
        assert!(schema["properties"].get("context").is_some());
    }
}
