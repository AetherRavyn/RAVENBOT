use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Shared office memory (team knowledge)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfficeMemory {
    pub id: Uuid,
    pub chatroom_id: Uuid,
    pub content: String,
    pub embedding: Option<Vec<f32>>,
    pub importance: f32,
    pub category: String,
    pub created_by: Option<Uuid>,
    pub access_count: u32,
    pub last_accessed: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

impl OfficeMemory {
    pub fn new(chatroom_id: Uuid, content: impl Into<String>, category: impl Into<String>, created_by: Option<Uuid>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            chatroom_id,
            content: content.into(),
            embedding: None,
            importance: 0.6,
            category: category.into(),
            created_by,
            access_count: 0,
            last_accessed: now,
            created_at: now,
        }
    }
}

/// Agent learning entry (how agent gets smarter)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentLearning {
    pub id: Uuid,
    pub bot_id: Uuid,
    pub chatroom_id: Option<Uuid>,
    pub learning_type: String,
    pub content: String,
    pub context: Option<String>,
    pub success_rate: f32,
    pub tasks_completed: u32,
    pub tasks_failed: u32,
    pub created_at: DateTime<Utc>,
}

/// Intelligence snapshot — how smart agent is today
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentIntelligence {
    pub bot_id: Uuid,
    pub total_memories: u32,
    pub office_memories: u32,
    pub learnings_count: u32,
    pub intelligence_score: f32,
    pub tasks_today: u32,
    pub success_streak: u32,
    pub last_active: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}
