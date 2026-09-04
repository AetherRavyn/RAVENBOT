//! Retrieval-augmented generation (RAG)

use crate::store::MemoryStore;
use uuid::Uuid;

/// Memory retriever for RAG
pub struct MemoryRetriever {
    store: MemoryStore,
}

impl MemoryRetriever {
    pub fn new(store: MemoryStore) -> Self {
        Self { store }
    }

    /// Retrieve relevant memories and format as context
    pub async fn get_context(
        &self,
        bot_id: Uuid,
        query: &str,
        max_memories: usize,
    ) -> Result<String, String> {
        let memories = self.store.retrieve(bot_id, query, max_memories, 0.3).await?;

        if memories.is_empty() {
            return Ok(String::new());
        }

        let context: Vec<String> = memories.iter()
            .map(|(fact, score)| {
                format!("- [relevance: {:.2}] {}", score, fact.content)
            })
            .collect();

        Ok(format!(
            "Relevant memories:\n{}",
            context.join("\n")
        ))
    }

    /// Retrieve and summarize memories for a topic
    pub async fn summarize_for_topic(
        &self,
        bot_id: Uuid,
        topic: &str,
    ) -> Result<Option<String>, String> {
        let memories = self.store.retrieve(bot_id, topic, 5, 0.4).await?;

        if memories.is_empty() {
            return Ok(None);
        }

        let summary = memories.iter()
            .map(|(fact, _)| fact.content.as_str())
            .collect::<Vec<_>>()
            .join("; ");

        Ok(Some(format!(
            "Regarding '{}': {}",
            topic, summary
        )))
    }

    /// Check if any memories relate to a specific fact
    pub async fn has_similar_memory(
        &self,
        bot_id: Uuid,
        content: &str,
        threshold: f32,
    ) -> Result<bool, String> {
        let memories = self.store.retrieve(bot_id, content, 1, threshold).await?;
        Ok(!memories.is_empty())
    }
}
