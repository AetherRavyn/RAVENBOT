use async_trait::async_trait;
use ravenbot_core::Permission;
use crate::traits::{Skill, SkillContext, SkillError, SkillResult};
use std::collections::HashMap;
use std::sync::OnceLock;
use tokio::sync::Mutex;
use chrono::Utc;
use uuid::Uuid;

static CAL: OnceLock<Mutex<HashMap<String, Vec<serde_json::Value>>>> = OnceLock::new();
fn cal() -> &'static Mutex<HashMap<String, Vec<serde_json::Value>>> { CAL.get_or_init(|| Mutex::new(HashMap::new())) }

pub struct CalendarSkill;
impl CalendarSkill { pub fn new() -> Self { Self } }

#[async_trait]
impl Skill for CalendarSkill {
    fn id(&self) -> &str { "calendar" }
    fn name(&self) -> &str { "Calendar" }
    fn description(&self) -> &str { "Local calendar: create/list events — offline, CTO lane" }
    fn version(&self) -> &str { "1.0.0" }
    fn required_permissions(&self) -> Vec<Permission> { vec![] }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({"type":"object","properties":{
            "action":{"type":"string","enum":["create","list","delete"]},
            "title":{"type":"string"},
            "when":{"type":"string","description":"ISO8601 or natural e.g. 'tomorrow 9am'"},
            "id":{"type":"string"}
        },"required":["action"]})
    }
    async fn execute(&self, ctx: &SkillContext, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("list");
        let key = ctx.bot_id.to_string();
        let mut map = cal().lock().await;
        let list = map.entry(key.clone()).or_default();
        match action {
            "create" => {
                let title = args.get("title").and_then(|v| v.as_str()).ok_or_else(|| SkillError::InvalidArguments("Missing title".into()))?;
                let when = args.get("when").and_then(|v| v.as_str()).unwrap_or("tomorrow 09:00");
                let ev = serde_json::json!({"id": Uuid::new_v4().to_string(), "title": title, "when": when, "created_at": Utc::now().to_rfc3339()});
                list.push(ev.clone());
                Ok(SkillResult::success(serde_json::json!({"created": ev})))
            },
            "list" => Ok(SkillResult::success(serde_json::json!({"events": list}))),
            "delete" => {
                let id = args.get("id").and_then(|v| v.as_str()).ok_or_else(|| SkillError::InvalidArguments("Missing id".into()))?;
                list.retain(|e| e.get("id").and_then(|v| v.as_str()) != Some(id));
                Ok(SkillResult::success(serde_json::json!({"deleted": id})))
            },
            _ => Err(SkillError::InvalidArguments("Unknown action".into())),
        }
    }
}
impl Default for CalendarSkill { fn default() -> Self { Self::new() } }
