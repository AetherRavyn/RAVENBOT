use async_trait::async_trait;
use ravenbot_core::Permission;
use crate::traits::{Skill, SkillContext, SkillError, SkillResult, SkillRisk};

pub struct TodoSkill;

impl TodoSkill { pub fn new() -> Self { Self } }

#[async_trait]
impl Skill for TodoSkill {
    fn id(&self) -> &str { "todo" }
    fn name(&self) -> &str { "Todo" }
    fn description(&self) -> &str { "Per-bot todo list (DB-backed): add/list/done/clear — offline, no OAuth" }
    fn version(&self) -> &str { "2.0.0" }
    fn required_permissions(&self) -> Vec<Permission> { vec![] }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({"type":"object","properties":{
            "action":{"type":"string","enum":["add","list","done","clear"]},
            "task":{"type":"string"},
            "id":{"type":"string"}
        },"required":["action"]})
    }
    fn risk(&self) -> SkillRisk { SkillRisk::Low }

    async fn execute(&self, _ctx: &SkillContext, _args: serde_json::Value) -> Result<SkillResult, SkillError> {
        // Never reached in production: the runtime routes "todo" to exec_todo
        // (it owns the DB pool). This path only fires for standalone registry
        // users, who get an honest error instead of silent memory loss.
        Err(SkillError::Execution(
            "todo requires the agent runtime (DB-backed); standalone registry execution is unsupported".to_string(),
        ))
    }
}
impl Default for TodoSkill { fn default() -> Self { Self::new() } }
