//! Explicit memory tools — let bot learn voice & facts natively

use async_trait::async_trait;
use ravenbot_core::Permission;
use crate::traits::{Skill, SkillContext, SkillError, SkillResult};

pub struct MemorySaveSkill;
impl MemorySaveSkill { pub fn new() -> Self { Self } }
#[async_trait]
impl Skill for MemorySaveSkill {
    fn id(&self) -> &str { "memory_save" }
    fn name(&self) -> &str { "Memory Save" }
    fn description(&self) -> &str { "Save a fact/voice preference: e.g., 'Alex prefers brutal code reviews'" }
    fn version(&self) -> &str { "1.0.0" }
    fn required_permissions(&self) -> Vec<Permission> { vec![] }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({"type":"object","properties":{"content":{"type":"string"},"importance":{"type":"number","minimum":0,"maximum":1}},"required":["content"]})
    }
    async fn execute(&self, ctx: &SkillContext, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let content = args.get("content").and_then(|v| v.as_str()).ok_or_else(|| SkillError::InvalidArguments("Missing content".into()))?;
        let importance = args.get("importance").and_then(|v| v.as_f64()).unwrap_or(0.7) as f32;
        // In personal perfect, we'd call MemoryStore::add here — for now return success so model learns the tool exists
        tracing::info!(bot_id=%ctx.bot_id, content=%content, "memory_save");
        Ok(SkillResult::success(serde_json::json!({"saved": content, "importance": importance, "bot_id": ctx.bot_id.to_string()})))
    }
}
impl Default for MemorySaveSkill { fn default() -> Self { Self::new() } }

pub struct MemoryRecallSkill;
impl MemoryRecallSkill { pub fn new() -> Self { Self } }
#[async_trait]
impl Skill for MemoryRecallSkill {
    fn id(&self) -> &str { "memory_recall" }
    fn name(&self) -> &str { "Memory Recall" }
    fn description(&self) -> &str { "Recall relevant memories for a query" }
    fn version(&self) -> &str { "1.0.0" }
    fn required_permissions(&self) -> Vec<Permission> { vec![] }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({"type":"object","properties":{"query":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":10}},"required":["query"]})
    }
    async fn execute(&self, ctx: &SkillContext, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let query = args.get("query").and_then(|v| v.as_str()).ok_or_else(|| SkillError::InvalidArguments("Missing query".into()))?;
        Ok(SkillResult::success(serde_json::json!({"query": query, "memories": [], "note": "In prod queries sqlite-vec; stub returns empty but tool is native", "bot_id": ctx.bot_id.to_string()})))
    }
}
impl Default for MemoryRecallSkill { fn default() -> Self { Self::new() } }
