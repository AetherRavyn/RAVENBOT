use async_trait::async_trait;
use ravenbot_core::Permission;
use crate::traits::{Skill, SkillContext, SkillError, SkillResult};
use std::sync::OnceLock;
use tokio::sync::Mutex;
use std::collections::HashMap;

static TODOS: OnceLock<Mutex<HashMap<String, Vec<serde_json::Value>>>> = OnceLock::new();
fn todos() -> &'static Mutex<HashMap<String, Vec<serde_json::Value>>> {
    TODOS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub struct TodoSkill;

impl TodoSkill { pub fn new() -> Self { Self } }

#[async_trait]
impl Skill for TodoSkill {
    fn id(&self) -> &str { "todo" }
    fn name(&self) -> &str { "Todo" }
    fn description(&self) -> &str { "Local todo list per bot: add/list/done — offline, no OAuth" }
    fn version(&self) -> &str { "1.0.0" }
    fn required_permissions(&self) -> Vec<Permission> { vec![] }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({"type":"object","properties":{
            "action":{"type":"string","enum":["add","list","done","clear"]},
            "task":{"type":"string"},
            "id":{"type":"string"}
        },"required":["action"]})
    }
    async fn execute(&self, ctx: &SkillContext, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("list");
        let key = ctx.bot_id.to_string();
        let mut map = todos().lock().await;
        let list = map.entry(key.clone()).or_default();
        match action {
            "add" => {
                let task = args.get("task").and_then(|v| v.as_str()).ok_or_else(|| SkillError::InvalidArguments("Missing task".into()))?;
                let id = uuid::Uuid::new_v4().to_string();
                list.push(serde_json::json!({"id": id, "task": task, "done": false}));
                Ok(SkillResult::success(serde_json::json!({"added": task, "count": list.len()})))
            },
            "list" => Ok(SkillResult::success(serde_json::json!({"todos": list}))),
            "done" => {
                let id = args.get("id").and_then(|v| v.as_str()).ok_or_else(|| SkillError::InvalidArguments("Missing id".into()))?;
                for t in list.iter_mut() { if t.get("id").and_then(|v| v.as_str()) == Some(id) { t["done"] = serde_json::Value::Bool(true); } }
                Ok(SkillResult::success(serde_json::json!({"done": id})))
            },
            "clear" => { list.clear(); Ok(SkillResult::success(serde_json::json!({"cleared": true}))) },
            _ => Err(SkillError::InvalidArguments("Unknown action".into())),
        }
    }
}
impl Default for TodoSkill { fn default() -> Self { Self::new() } }
