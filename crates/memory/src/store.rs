//! Memory storage and management

use crate::embedding::{EmbeddingProvider, cosine_similarity};
use ravenbot_core::MemoryFact;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct MemoryStore {
    pool: SqlitePool,
    embedding_provider: Box<dyn EmbeddingProvider>,
}

impl MemoryStore {
    pub fn new(pool: SqlitePool, embedding_provider: Box<dyn EmbeddingProvider>) -> Self {
        Self {
            pool,
            embedding_provider,
        }
    }

    /// Add a new memory fact
    pub async fn add(
        &self,
        bot_id: Uuid,
        content: &str,
        importance: f32,
    ) -> Result<MemoryFact, String> {
        // Generate embedding
        let embedding = self.embedding_provider.embed(content).await
            .map_err(|e| e.to_string())?;

        let fact = MemoryFact {
            id: Uuid::new_v4(),
            bot_id,
            content: content.to_string(),
            embedding: Some(embedding),
            importance,
            access_count: 0,
            last_accessed: chrono::Utc::now(),
            created_at: chrono::Utc::now(),
        };

        // Store in database
        let embedding_bytes = serialize_embedding(fact.embedding.as_ref().unwrap());
        
        sqlx::query(
            "INSERT INTO memory_facts (id, bot_id, content, embedding, importance, access_count, last_accessed, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(fact.id.to_string())
        .bind(fact.bot_id.to_string())
        .bind(&fact.content)
        .bind(&embedding_bytes)
        .bind(fact.importance)
        .bind(fact.access_count)
        .bind(fact.last_accessed.to_rfc3339())
        .bind(fact.created_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        tracing::info!(fact_id = %fact.id, bot_id = %bot_id, "Memory fact added");
        Ok(fact)
    }

    /// Retrieve memories similar to a query
    pub async fn retrieve(
        &self,
        bot_id: Uuid,
        query: &str,
        limit: usize,
        threshold: f32,
    ) -> Result<Vec<(MemoryFact, f32)>, String> {
        // Generate query embedding
        let query_embedding = self.embedding_provider.embed(query).await
            .map_err(|e| e.to_string())?;

        // Fetch all memories for this bot
        let rows: Vec<(String, String, String, Option<Vec<u8>>, f32, i32, String, String)> = sqlx::query_as(
            "SELECT id, bot_id, content, embedding, importance, access_count, last_accessed, created_at
             FROM memory_facts
             WHERE bot_id = ?
             ORDER BY created_at DESC"
        )
        .bind(bot_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        // Calculate similarities
        let mut scored: Vec<(MemoryFact, f32)> = rows.iter()
            .filter_map(|row| {
                let embedding = row.3.as_ref().and_then(|e| deserialize_embedding(e))?;
                let similarity = cosine_similarity(&query_embedding, &embedding);
                
                if similarity < threshold {
                    return None;
                }

                let fact = MemoryFact {
                    id: Uuid::parse_str(&row.0).ok()?,
                    bot_id: Uuid::parse_str(&row.1).ok()?,
                    content: row.2.clone(),
                    embedding: Some(embedding),
                    importance: row.4,
                    access_count: row.5 as u32,
                    last_accessed: chrono::DateTime::parse_from_rfc3339(&row.6)
                        .ok()
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(chrono::Utc::now),
                    created_at: chrono::DateTime::parse_from_rfc3339(&row.7)
                        .ok()
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(chrono::Utc::now),
                };

                // Boost score by importance and recency
                let recency_boost = 1.0 / (1.0 + fact.access_count as f32 * 0.1);
                let final_score = similarity * (0.7 + 0.3 * fact.importance) * recency_boost;

                Some((fact, final_score))
            })
            .collect();

        // Sort by score descending
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Take top results
        let results: Vec<(MemoryFact, f32)> = scored.into_iter()
            .take(limit)
            .collect();

        // Update access counts for retrieved memories
        for (fact, _) in &results {
            let _ = sqlx::query(
                "UPDATE memory_facts SET access_count = access_count + 1, last_accessed = ? WHERE id = ?"
            )
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(fact.id.to_string())
            .execute(&self.pool)
            .await;
        }

        Ok(results)
    }

    /// Delete a memory fact
    pub async fn delete(&self, fact_id: Uuid) -> Result<(), String> {
        sqlx::query("DELETE FROM memory_facts WHERE id = ?")
            .bind(fact_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Get all memories for a bot
    pub async fn list_all(&self, bot_id: Uuid) -> Result<Vec<MemoryFact>, String> {
        let rows: Vec<(String, String, String, Option<Vec<u8>>, f32, i32, String, String)> = sqlx::query_as(
            "SELECT id, bot_id, content, embedding, importance, access_count, last_accessed, created_at
             FROM memory_facts
             WHERE bot_id = ?
             ORDER BY importance DESC, created_at DESC"
        )
        .bind(bot_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        let facts = rows.iter()
            .filter_map(|row| {
                Some(MemoryFact {
                    id: Uuid::parse_str(&row.0).ok()?,
                    bot_id: Uuid::parse_str(&row.1).ok()?,
                    content: row.2.clone(),
                    embedding: row.3.as_ref().and_then(|e| deserialize_embedding(e)),
                    importance: row.4,
                    access_count: row.5 as u32,
                    last_accessed: chrono::DateTime::parse_from_rfc3339(&row.6)
                        .ok()
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(chrono::Utc::now),
                    created_at: chrono::DateTime::parse_from_rfc3339(&row.7)
                        .ok()
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(chrono::Utc::now),
                })
            })
            .collect();

        Ok(facts)
    }

    /// Apply decay to old memories
    pub async fn apply_decay(&self, bot_id: Uuid, decay_factor: f32) -> Result<u64, String> {
        let result = sqlx::query(
            "UPDATE memory_facts 
             SET importance = importance * ?
             WHERE bot_id = ? AND importance > 0.01"
        )
        .bind(decay_factor)
        .bind(bot_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        Ok(result.rows_affected())
    }

    /// Get memory count for a bot
    pub async fn count(&self, bot_id: Uuid) -> Result<u64, String> {
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM memory_facts WHERE bot_id = ?")
            .bind(bot_id.to_string())
            .fetch_one(&self.pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(row.0 as u64)
    }

    /// Check if a similar memory already exists
    pub async fn has_similar_memory(
        &self,
        bot_id: Uuid,
        content: &str,
        threshold: f32,
    ) -> Result<bool, String> {
        let results = self.retrieve(bot_id, content, 1, threshold).await?;
        Ok(!results.is_empty())
    }
}

/// Serialize embedding vector to bytes
fn serialize_embedding(embedding: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(embedding.len() * 4);
    for &val in embedding {
        bytes.extend_from_slice(&val.to_le_bytes());
    }
    bytes
}

/// Deserialize embedding bytes to vector
fn deserialize_embedding(bytes: &[u8]) -> Option<Vec<f32>> {
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    
    let embedding: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();
    
    Some(embedding)
}
