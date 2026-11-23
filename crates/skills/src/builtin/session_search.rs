//! Transcript search tool.
//!
//! The skill declares the schema and risk tier; the runtime intercepts
//! `session_search` and runs the real FTS5 query. This entry exists so the
//! model sees the tool and the capability gate has a permission record. If it
//! is ever reached directly, it fails loudly rather than pretending to search.

use async_trait::async_trait;
use ravenbot_core::Permission;

use crate::traits::{Skill, SkillContext, SkillError, SkillResult};

pub struct SessionSearchSkill;

impl SessionSearchSkill {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SessionSearchSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Skill for SessionSearchSkill {
    fn id(&self) -> &str {
        "session_search"
    }

    fn name(&self) -> &str {
        "Search sessions"
    }

    fn description(&self) -> &str {
        "Search past conversations across every thread for decisions, file paths, errors, or prior work. Use this when a long project's context was compacted or when you need to recover what was tried earlier."
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn required_permissions(&self) -> Vec<Permission> {
        Vec::new()
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Words or a phrase to find in the transcript."
                },
                "thread_id": {
                    "type": "string",
                    "description": "Optional thread id to search within."
                },
                "limit": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 20,
                    "description": "Maximum hits to return."
                }
            },
            "required": ["query"]
        })
    }

    fn risk(&self) -> crate::traits::SkillRisk {
        crate::traits::SkillRisk::ReadOnly
    }

    async fn execute(
        &self,
        _context: &SkillContext,
        _arguments: serde_json::Value,
    ) -> Result<SkillResult, SkillError> {
        Err(SkillError::Execution(
            "session_search is implemented by the runtime, not the skill registry; \
             the dispatch order must intercept it before execute."
                .to_string(),
        ))
    }
}
