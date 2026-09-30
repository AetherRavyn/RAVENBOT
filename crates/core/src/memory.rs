use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use uuid::Uuid;

/// A memory fact stored by a bot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryFact {
    /// Unique identifier
    pub id: Uuid,
    /// Bot that owns this memory
    pub bot_id: Uuid,
    /// The fact content
    pub content: String,
    /// Embedding vector (for similarity search)
    pub embedding: Option<Vec<f32>>,
    /// Importance score (0.0 - 1.0)
    pub importance: f32,
    /// Access count (for decay)
    pub access_count: u32,
    /// When this fact was last accessed
    pub last_accessed: DateTime<Utc>,
    /// Created timestamp
    pub created_at: DateTime<Utc>,
}

impl MemoryFact {
    /// Create a new memory fact
    pub fn new(bot_id: Uuid, content: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            bot_id,
            content: content.into(),
            embedding: None,
            importance: 0.5,
            access_count: 0,
            last_accessed: now,
            created_at: now,
        }
    }

    /// Mark as accessed
    pub fn touch(&mut self) {
        self.access_count += 1;
        self.last_accessed = Utc::now();
    }

    /// Calculate decay factor based on age and access count
    pub fn decay_factor(&self) -> f32 {
        let age_days = (Utc::now() - self.created_at).num_days() as f32;
        let recency = (Utc::now() - self.last_accessed).num_days() as f32;
        let base = self.importance;
        let access_boost = (self.access_count as f32).ln_1p() * 0.1;
        let age_penalty = age_days * 0.01;
        let recency_boost = 1.0 / (1.0 + recency * 0.1) ;
        (base + access_boost - age_penalty) * recency_boost
    }
}

/// Query for memory retrieval
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryQuery {
    /// The query text
    pub query: String,
    /// Query embedding (computed from query text)
    pub embedding: Option<Vec<f32>>,
    /// Maximum number of results
    pub limit: usize,
    /// Minimum similarity threshold
    pub threshold: f32,
}

impl Default for MemoryQuery {
    fn default() -> Self {
        Self {
            query: String::new(),
            embedding: None,
            limit: 10,
            threshold: 0.5,
        }
    }
}
