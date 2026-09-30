//! Self-review system for learning from runs

use crate::store::MemoryStore;
use ravenbot_core::{Run, RunOutcome, Message, MessageRole};
use sqlx::SqlitePool;
use uuid::Uuid;

/// Review result from analyzing a run
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReviewResult {
    /// What went well
    pub successes: Vec<String>,
    /// What could be improved
    pub improvements: Vec<String>,
    /// Key facts learned
    pub facts: Vec<String>,
    /// Suggested memory updates
    pub memory_updates: Vec<MemoryUpdate>,
    /// Overall quality score (0-1)
    pub quality_score: f32,
}

/// Memory update suggestion
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MemoryUpdate {
    /// Content to remember
    pub content: String,
    /// Importance (0-1)
    pub importance: f32,
    /// Whether this is a new fact or update
    pub is_new: bool,
}

/// Self-reviewer for analyzing completed runs
pub struct SelfReviewer {
    pool: SqlitePool,
    memory_store: MemoryStore,
}

impl SelfReviewer {
    pub fn new(pool: SqlitePool, memory_store: MemoryStore) -> Self {
        Self {
            pool,
            memory_store,
        }
    }

    /// Review a completed run and extract learnings
    pub async fn review_run(&self, run: &Run) -> Result<ReviewResult, String> {
        // Get all messages from the run's thread
        let messages = self.get_thread_messages(run.thread_id).await?;

        // Analyze the conversation
        let mut successes = Vec::new();
        let mut improvements = Vec::new();
        let mut facts = Vec::new();
        let mut memory_updates = Vec::new();

        // Check if the run was successful
        match &run.outcome {
            Some(RunOutcome::Success { result }) => {
                successes.push(format!("Task completed: {}", result));
            }
            Some(RunOutcome::PartialSuccess { result, errors }) => {
                successes.push(format!("Partial success: {}", result));
                for error in errors {
                    improvements.push(format!("Error encountered: {}", error));
                }
            }
            Some(RunOutcome::Failure { error }) => {
                improvements.push(format!("Task failed: {}", error));
            }
            _ => {}
        }

        // Analyze tool usage
        let tool_calls: Vec<&Message> = messages.iter()
            .filter(|m| matches!(m.role, MessageRole::Tool))
            .collect();
        
        if !tool_calls.is_empty() {
            successes.push(format!("Used {} tools successfully", tool_calls.len()));
        }

        // Extract facts from assistant responses
        let assistant_msgs: Vec<&Message> = messages.iter()
            .filter(|m| matches!(m.role, MessageRole::Assistant))
            .collect();

        for msg in assistant_msgs {
            if let ravenbot_core::MessageContent::Text { text, .. } = &msg.content {
                // Simple fact extraction (in production, use NLP)
                if text.len() > 50 && text.contains('.') {
                    facts.push(text.clone());
                    
                    memory_updates.push(MemoryUpdate {
                        content: text.clone(),
                        importance: 0.6,
                        is_new: true,
                    });
                }
            }
        }

        // Calculate quality score
        let quality_score = if run.outcome.as_ref().map(|o| matches!(o, RunOutcome::Success { .. })).unwrap_or(false) {
            0.9
        } else if run.outcome.as_ref().map(|o| matches!(o, RunOutcome::PartialSuccess { .. })).unwrap_or(false) {
            0.6
        } else {
            0.3_f32
        };

        // Factor in efficiency (tokens used vs. expected)
        let efficiency_bonus: f32 = if run.tokens_consumed > 0 {
            // More tokens doesn't always mean worse, but extreme usage is concerning
            if run.tokens_consumed < 10000 { 0.1 } else { 0.0 }
        } else {
            0.0
        };

        let final_score = (quality_score + efficiency_bonus).min(1.0);

        Ok(ReviewResult {
            successes,
            improvements,
            facts,
            memory_updates,
            quality_score: final_score,
        })
    }

    /// Apply memory updates from a review
    pub async fn apply_updates(
        &self,
        bot_id: Uuid,
        updates: &[MemoryUpdate],
    ) -> Result<u32, String> {
        let mut applied = 0;

        for update in updates {
            // Check for similar existing memories
            let has_similar = self.memory_store.has_similar_memory(
                bot_id,
                &update.content,
                0.8,
            ).await?;

            if !has_similar || update.is_new {
                self.memory_store.add(
                    bot_id,
                    &update.content,
                    update.importance,
                ).await?;
                applied += 1;
            }
        }

        tracing::info!(
            bot_id = %bot_id,
            applied = applied,
            "Applied memory updates from review"
        );

        Ok(applied)
    }

    /// Get skill proficiency score for a bot
    pub async fn get_skill_proficiency(
        &self,
        _bot_id: Uuid,
        _skill_id: &str,
    ) -> Result<f32, String> {
        // In a full implementation, this would track success rates per skill
        // For now, return a default score
        Ok(0.7)
    }

    /// Update skill proficiency based on outcome
    pub async fn update_skill_proficiency(
        &self,
        bot_id: Uuid,
        skill_id: &str,
        success: bool,
    ) -> Result<(), String> {
        // In a full implementation, this would update a proficiency table
        tracing::debug!(
            bot_id = %bot_id,
            skill_id = skill_id,
            success = success,
            "Skill proficiency updated"
        );
        Ok(())
    }

    /// Get messages for a thread
    async fn get_thread_messages(&self, thread_id: Uuid) -> Result<Vec<Message>, String> {
        ravenbot_db::queries::MessageQueries::list_by_thread(&self.pool, thread_id)
            .await
            .map_err(|e| e.to_string())
    }

    /// Generate a golden run fixture from a successful run
    pub async fn create_golden_fixture(
        &self,
        run: &Run,
        description: &str,
    ) -> Result<serde_json::Value, String> {
        let messages = self.get_thread_messages(run.thread_id).await?;

        let fixture = serde_json::json!({
            "description": description,
            "run_id": run.id.to_string(),
            "bot_id": run.bot_id.to_string(),
            "messages": messages.iter().map(|m| {
                serde_json::json!({
                    "role": format!("{:?}", m.role),
                    "content": match &m.content {
                        ravenbot_core::MessageContent::Text { text, .. } => text,
                        _ => "...",
                    }
                })
            }).collect::<Vec<_>>(),
            "expected_outcome": format!("{:?}", run.outcome),
            "tokens_used": run.tokens_consumed,
            "created_at": run.created_at.to_rfc3339(),
        });

        Ok(fixture)
    }
}
