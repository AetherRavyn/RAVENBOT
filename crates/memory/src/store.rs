//! Memory storage and management

use crate::embedding::{cosine_similarity, EmbeddingProvider};
use ravenbot_core::MemoryFact;
use sqlx::{Connection, SqlitePool};
use uuid::Uuid;

/// A fact, not a document. Without a cap an agent could store an entire report
/// as one memory and inject it into every later prompt.
const MAX_MEMORY_CHARS: usize = 2000;

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

    /// Add a new memory fact with unknown provenance.
    pub async fn add(
        &self,
        bot_id: Uuid,
        content: &str,
        importance: f32,
    ) -> Result<MemoryFact, String> {
        self.add_with_provenance(bot_id, content, importance, "unknown", None, None)
            .await
    }

    /// Add a fact and record where it came from.
    pub async fn add_with_provenance(
        &self,
        bot_id: Uuid,
        content: &str,
        importance: f32,
        source: &str,
        source_thread_id: Option<Uuid>,
        source_run_id: Option<Uuid>,
    ) -> Result<MemoryFact, String> {
        let _guard = crate::MEMORY_INDEX_LOCK.read().await;
        let content = normalize_content(content);
        if content.is_empty() {
            return Err("Memory content cannot be empty.".to_string());
        }
        if !importance.is_finite() || !(0.0..=1.0).contains(&importance) {
            return Err("Memory importance must be between 0 and 1.".to_string());
        }
        let source = normalize_source(source);
        // Generate embedding
        let embedding = self
            .embedding_provider
            .embed(&content)
            .await
            .map_err(|e| e.to_string())?;

        let now = chrono::Utc::now();
        let fact = MemoryFact {
            id: Uuid::new_v4(),
            bot_id,
            content: content.to_string(),
            embedding: Some(embedding),
            importance,
            access_count: 0,
            last_accessed: now,
            created_at: now,
            source,
            source_thread_id,
            source_run_id,
            updated_at: now,
        };

        // Store in database
        let embedding_bytes = serialize_embedding(fact.embedding.as_ref().unwrap());

        sqlx::query(
            "INSERT INTO memory_facts
             (id, bot_id, content, embedding, importance, access_count, last_accessed, created_at,
              source, source_thread_id, source_run_id, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(fact.id.to_string())
        .bind(fact.bot_id.to_string())
        .bind(&fact.content)
        .bind(&embedding_bytes)
        .bind(fact.importance)
        .bind(fact.access_count)
        .bind(fact.last_accessed.to_rfc3339())
        .bind(fact.created_at.to_rfc3339())
        .bind(&fact.source)
        .bind(fact.source_thread_id.map(|id| id.to_string()))
        .bind(fact.source_run_id.map(|id| id.to_string()))
        .bind(fact.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        tracing::info!(fact_id = %fact.id, bot_id = %bot_id, source = %fact.source, "Memory fact added");
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
        let query_embedding = self
            .embedding_provider
            .embed(query)
            .await
            .map_err(|e| e.to_string())?;

        // Fetch all memories for this bot
        let rows: Vec<FactRow> = sqlx::query_as(
            "SELECT id, bot_id, content, embedding, importance, access_count, last_accessed, created_at,
                    source, source_thread_id, source_run_id, updated_at
             FROM memory_facts
             WHERE bot_id = ?
             ORDER BY created_at DESC"
        )
        .bind(bot_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        // Calculate similarities
        let mut scored: Vec<(MemoryFact, f32)> = rows
            .iter()
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
                    source: row.8.clone(),
                    source_thread_id: row
                        .9
                        .as_deref()
                        .and_then(|value| Uuid::parse_str(value).ok()),
                    source_run_id: row
                        .10
                        .as_deref()
                        .and_then(|value| Uuid::parse_str(value).ok()),
                    updated_at: chrono::DateTime::parse_from_rfc3339(&row.11)
                        .ok()
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(chrono::Utc::now),
                };

                // Frequently useful memories should not be demoted: the old
                // `1/(1+access_count*0.1)` made the best facts rank lower every
                // time they were recalled. A small bounded boost instead.
                let usage_boost = 1.0 + fact.access_count.min(20) as f32 * 0.01;
                let final_score = similarity * (0.7 + 0.3 * fact.importance) * usage_boost;

                Some((fact, final_score))
            })
            .collect();

        // Sort by score descending
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Take top results
        let results: Vec<(MemoryFact, f32)> = scored.into_iter().take(limit).collect();

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

    /// Delete a fact only when it belongs to the given bot.
    pub async fn delete_for_bot(&self, bot_id: Uuid, fact_id: Uuid) -> Result<bool, String> {
        let result = sqlx::query("DELETE FROM memory_facts WHERE id = ? AND bot_id = ?")
            .bind(fact_id.to_string())
            .bind(bot_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
        Ok(result.rows_affected() > 0)
    }

    /// Fetch one fact scoped to its owner.
    pub async fn get(&self, bot_id: Uuid, fact_id: Uuid) -> Result<Option<MemoryFact>, String> {
        let row: Option<FactRow> = sqlx::query_as(
            "SELECT id, bot_id, content, embedding, importance, access_count, last_accessed, created_at,
                    source, source_thread_id, source_run_id, updated_at
             FROM memory_facts
             WHERE id = ? AND bot_id = ?",
        )
        .bind(fact_id.to_string())
        .bind(bot_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        Ok(row.and_then(fact_from_row))
    }

    /// Update a fact's content and importance, then return the stored row.
    pub async fn update(
        &self,
        bot_id: Uuid,
        fact_id: Uuid,
        content: &str,
        importance: f32,
    ) -> Result<Option<MemoryFact>, String> {
        let _guard = crate::MEMORY_INDEX_LOCK.read().await;
        let content = normalize_content(content);
        if content.is_empty() {
            return Err("Memory content cannot be empty.".to_string());
        }
        if !importance.is_finite() || !(0.0..=1.0).contains(&importance) {
            return Err("Memory importance must be between 0 and 1.".to_string());
        }
        let embedding = self
            .embedding_provider
            .embed(&content)
            .await
            .map_err(|e| e.to_string())?;
        let bytes = serialize_embedding(&embedding);
        let result = sqlx::query(
            "UPDATE memory_facts
             SET content = ?, embedding = ?, importance = ?, updated_at = ?
             WHERE id = ? AND bot_id = ?",
        )
        .bind(&content)
        .bind(&bytes)
        .bind(importance)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(fact_id.to_string())
        .bind(bot_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        if result.rows_affected() == 0 {
            return Ok(None);
        }
        self.get(bot_id, fact_id).await
    }

    /// Replace several facts with one summary in a single transaction.
    pub async fn replace_with_summary(
        &self,
        bot_id: Uuid,
        source_ids: &[Uuid],
        content: &str,
        importance: f32,
    ) -> Result<MemoryFact, String> {
        let _guard = crate::MEMORY_INDEX_LOCK.read().await;
        if !(2..=20).contains(&source_ids.len()) {
            return Err("Choose between 2 and 20 memories to summarize.".to_string());
        }
        let mut seen = std::collections::HashSet::new();
        if source_ids.iter().any(|id| !seen.insert(*id)) {
            return Err("The summary contains the same memory twice.".to_string());
        }
        let content = normalize_content(content);
        if content.is_empty() {
            return Err("The summary cannot be empty.".to_string());
        }
        if !importance.is_finite() || !(0.0..=1.0).contains(&importance) {
            return Err("Memory importance must be between 0 and 1.".to_string());
        }
        let embedding = self
            .embedding_provider
            .embed(&content)
            .await
            .map_err(|e| e.to_string())?;
        let now = chrono::Utc::now();
        let fact = MemoryFact {
            id: Uuid::new_v4(),
            bot_id,
            content: content.to_string(),
            embedding: Some(embedding.clone()),
            importance,
            access_count: 0,
            last_accessed: now,
            created_at: now,
            source: "summary".to_string(),
            source_thread_id: None,
            source_run_id: None,
            updated_at: now,
        };
        let mut tx = self.pool.begin().await.map_err(|e| e.to_string())?;
        for id in source_ids {
            let row: Option<(String,)> =
                sqlx::query_as("SELECT id FROM memory_facts WHERE id = ? AND bot_id = ?")
                    .bind(id.to_string())
                    .bind(bot_id.to_string())
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(|e| e.to_string())?;
            if row.is_none() {
                return Err("One of those memories no longer exists.".to_string());
            }
        }
        sqlx::query(
            "INSERT INTO memory_facts
             (id, bot_id, content, embedding, importance, access_count, last_accessed, created_at,
              source, source_thread_id, source_run_id, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(fact.id.to_string())
        .bind(fact.bot_id.to_string())
        .bind(&fact.content)
        .bind(serialize_embedding(&embedding))
        .bind(fact.importance)
        .bind(fact.access_count)
        .bind(fact.last_accessed.to_rfc3339())
        .bind(fact.created_at.to_rfc3339())
        .bind(&fact.source)
        .bind(Option::<String>::None)
        .bind(Option::<String>::None)
        .bind(fact.updated_at.to_rfc3339())
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        for id in source_ids {
            sqlx::query("DELETE FROM memory_facts WHERE id = ? AND bot_id = ?")
                .bind(id.to_string())
                .bind(bot_id.to_string())
                .execute(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
        }
        tx.commit().await.map_err(|e| e.to_string())?;
        Ok(fact)
    }

    /// List the most important facts for a bot, capped for the management UI.
    pub async fn list_limited(
        &self,
        bot_id: Uuid,
        limit: usize,
    ) -> Result<Vec<MemoryFact>, String> {
        let limit = limit.clamp(1, 2000) as i64;
        let rows: Vec<FactRow> = sqlx::query_as(
            "SELECT id, bot_id, content, embedding, importance, access_count, last_accessed, created_at,
                    source, source_thread_id, source_run_id, updated_at
             FROM memory_facts
             WHERE bot_id = ?
             ORDER BY importance DESC, created_at DESC
             LIMIT ?",
        )
        .bind(bot_id.to_string())
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        Ok(rows.into_iter().filter_map(fact_from_row).collect())
    }

    /// Re-embed every fact with a new provider in one transaction.
    ///
    /// Used when the embedding backend changes. Vectors from different models
    /// are not comparable, so this replaces all of them rather than leaving a
    /// mixed index behind. Nothing is written unless every embedding succeeds.
    pub async fn reindex_with(&self, provider: &dyn EmbeddingProvider) -> Result<u64, String> {
        let _guard = crate::MEMORY_INDEX_LOCK.write().await;
        self.reindex_locked(provider).await
    }

    pub(crate) async fn reindex_locked(
        &self,
        provider: &dyn EmbeddingProvider,
    ) -> Result<u64, String> {
        let mut conn = self.pool.acquire().await.map_err(|e| e.to_string())?;
        sqlx::query(
            "CREATE TEMP TABLE IF NOT EXISTS memory_reindex_stage (
                id TEXT PRIMARY KEY NOT NULL,
                embedding BLOB NOT NULL
            )",
        )
        .execute(&mut *conn)
        .await
        .map_err(|e| e.to_string())?;
        sqlx::query("DELETE FROM memory_reindex_stage")
            .execute(&mut *conn)
            .await
            .map_err(|e| e.to_string())?;
        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT id, content FROM memory_facts ORDER BY created_at ASC")
                .fetch_all(&mut *conn)
                .await
                .map_err(|e| e.to_string())?;
        let mut dimension = None;
        for chunk in rows.chunks(64) {
            let texts: Vec<&str> = chunk.iter().map(|(_, content)| content.as_str()).collect();
            let batch = provider
                .embed_batch(&texts)
                .await
                .map_err(|e| e.to_string())?;
            if batch.len() != chunk.len() {
                return Err(
                    "The embedding provider returned the wrong number of vectors.".to_string(),
                );
            }
            for ((id, _), embedding) in chunk.iter().zip(batch) {
                if embedding.is_empty() {
                    return Err("The embedding provider returned an empty vector.".to_string());
                }
                if let Some(expected) = dimension {
                    if embedding.len() != expected {
                        return Err("The embedding provider returned inconsistent vector sizes."
                            .to_string());
                    }
                } else {
                    dimension = Some(embedding.len());
                }
                sqlx::query("INSERT INTO memory_reindex_stage (id, embedding) VALUES (?, ?)")
                    .bind(id)
                    .bind(serialize_embedding(&embedding))
                    .execute(&mut *conn)
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }
        let mut tx = conn.begin().await.map_err(|e| e.to_string())?;
        sqlx::query(
            "UPDATE memory_facts
             SET embedding = (
                SELECT embedding FROM memory_reindex_stage
                WHERE memory_reindex_stage.id = memory_facts.id
             )
             WHERE id IN (SELECT id FROM memory_reindex_stage)",
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        sqlx::query("DELETE FROM memory_reindex_stage")
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        tx.commit().await.map_err(|e| e.to_string())?;
        Ok(rows.len() as u64)
    }
    /// Get all memories for a bot
    pub async fn list_all(&self, bot_id: Uuid) -> Result<Vec<MemoryFact>, String> {
        let rows: Vec<FactRow> = sqlx::query_as(
            "SELECT id, bot_id, content, embedding, importance, access_count, last_accessed, created_at,
                    source, source_thread_id, source_run_id, updated_at
             FROM memory_facts
             WHERE bot_id = ?
             ORDER BY importance DESC, created_at DESC"
        )
        .bind(bot_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        let facts = rows
            .iter()
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
                    source: row.8.clone(),
                    source_thread_id: row
                        .9
                        .as_deref()
                        .and_then(|value| Uuid::parse_str(value).ok()),
                    source_run_id: row
                        .10
                        .as_deref()
                        .and_then(|value| Uuid::parse_str(value).ok()),
                    updated_at: chrono::DateTime::parse_from_rfc3339(&row.11)
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
             SET importance = importance * ?, updated_at = ?
             WHERE bot_id = ? AND importance > 0.01",
        )
        .bind(decay_factor)
        .bind(chrono::Utc::now().to_rfc3339())
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

fn normalize_content(content: &str) -> String {
    let content = content.trim();
    if content.chars().count() > MAX_MEMORY_CHARS {
        content.chars().take(MAX_MEMORY_CHARS).collect::<String>()
    } else {
        content.to_string()
    }
}

type FactRow = (
    String,
    String,
    String,
    Option<Vec<u8>>,
    f32,
    i32,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
);

fn fact_from_row(row: FactRow) -> Option<MemoryFact> {
    Some(MemoryFact {
        id: Uuid::parse_str(&row.0).ok()?,
        bot_id: Uuid::parse_str(&row.1).ok()?,
        content: row.2,
        embedding: row
            .3
            .as_ref()
            .and_then(|bytes| deserialize_embedding(bytes)),
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
        source: row.8,
        source_thread_id: row
            .9
            .as_deref()
            .and_then(|value| Uuid::parse_str(value).ok()),
        source_run_id: row
            .10
            .as_deref()
            .and_then(|value| Uuid::parse_str(value).ok()),
        updated_at: chrono::DateTime::parse_from_rfc3339(&row.11)
            .ok()
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(chrono::Utc::now),
    })
}

fn normalize_source(source: &str) -> String {
    let source = source.trim();
    if source.is_empty() {
        "unknown".to_string()
    } else {
        source.chars().take(40).collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedding::LocalEmbedding;
    use ravenbot_core::Bot;
    use ravenbot_db::{queries, Database};

    async fn test_db(name: &str) -> (Database, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "ravenbot-memory-{name}-{}-{}.db",
            std::process::id(),
            Uuid::new_v4()
        ));
        let db = Database::new(&path).await.expect("temp db");
        (db, path)
    }

    fn cleanup(path: &std::path::Path) {
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }

    struct FixedEmbedding(Vec<f32>);

    #[async_trait::async_trait]
    impl EmbeddingProvider for FixedEmbedding {
        async fn embed(&self, _text: &str) -> Result<Vec<f32>, crate::embedding::EmbeddingError> {
            Ok(self.0.clone())
        }
        async fn embed_batch(
            &self,
            texts: &[&str],
        ) -> Result<Vec<Vec<f32>>, crate::embedding::EmbeddingError> {
            Ok(texts.iter().map(|_| self.0.clone()).collect())
        }
        fn dimension(&self) -> usize {
            self.0.len()
        }
    }

    #[tokio::test]
    async fn reindex_replaces_every_stored_vector() {
        let (db, path) = test_db("reindex").await;
        let bot = Bot::new("Reindex Bot", "test");
        let other = Bot::new("Other Reindex Bot", "test");
        queries::BotQueries::insert(db.pool(), &bot).await.unwrap();
        queries::BotQueries::insert(db.pool(), &other)
            .await
            .unwrap();
        let store = MemoryStore::new(db.pool().clone(), Box::new(LocalEmbedding::new(64)));
        let first = store.add(bot.id, "first fact", 0.5).await.unwrap();
        let second = store.add(other.id, "second fact", 0.5).await.unwrap();
        assert_eq!(first.embedding.as_ref().map(Vec::len), Some(64));

        let updated = store
            .reindex_with(&FixedEmbedding(vec![0.1, 0.2, 0.3]))
            .await
            .unwrap();
        assert_eq!(updated, 2);
        let first = store.get(bot.id, first.id).await.unwrap().unwrap();
        let second = store.get(other.id, second.id).await.unwrap().unwrap();
        assert_eq!(first.embedding.as_ref().map(Vec::len), Some(3));
        assert_eq!(second.embedding.as_ref().map(Vec::len), Some(3));

        drop(db);
        cleanup(&path);
    }

    #[tokio::test]
    async fn provenance_is_persisted_and_preserved_on_update() {
        let (db, path) = test_db("provenance").await;
        let bot = Bot::new("Provenance Bot", "test");
        queries::BotQueries::insert(db.pool(), &bot).await.unwrap();
        let store = MemoryStore::new(db.pool().clone(), Box::new(LocalEmbedding::new(64)));
        let thread = Uuid::new_v4();
        let run = Uuid::new_v4();
        let fact = store
            .add_with_provenance(
                bot.id,
                "Learned during a run",
                0.6,
                "agent",
                Some(thread),
                Some(run),
            )
            .await
            .expect("add fact");
        assert_eq!(fact.source, "agent");
        assert_eq!(fact.source_thread_id, Some(thread));
        assert_eq!(fact.source_run_id, Some(run));

        let updated = store
            .update(bot.id, fact.id, "Learned during a signed run", 0.8)
            .await
            .expect("update")
            .expect("fact exists");
        assert_eq!(updated.source, "agent");
        assert_eq!(updated.source_thread_id, Some(thread));
        assert_eq!(updated.source_run_id, Some(run));
        assert!(updated.updated_at >= fact.created_at);

        let listed = store.list_limited(bot.id, 10).await.unwrap();
        assert_eq!(listed[0].source, "agent");
        assert_eq!(listed[0].source_thread_id, Some(thread));

        drop(db);
        cleanup(&path);
    }

    #[tokio::test]
    async fn summary_replaces_selected_facts_atomically() {
        let (db, path) = test_db("summary").await;
        let bot = Bot::new("Summary Bot", "test");
        let other = Bot::new("Other Summary Bot", "test");
        queries::BotQueries::insert(db.pool(), &bot).await.unwrap();
        queries::BotQueries::insert(db.pool(), &other)
            .await
            .unwrap();
        let store = MemoryStore::new(db.pool().clone(), Box::new(LocalEmbedding::new(64)));
        let first = store.add(bot.id, "first fact", 0.5).await.unwrap();
        let second = store.add(bot.id, "second fact", 0.7).await.unwrap();
        let foreign = store.add(other.id, "other fact", 0.5).await.unwrap();

        assert!(store
            .replace_with_summary(bot.id, &[first.id, foreign.id], "cross-scope summary", 0.8)
            .await
            .is_err());
        assert_eq!(store.list_limited(bot.id, 10).await.unwrap().len(), 2);
        assert!(store.get(other.id, foreign.id).await.unwrap().is_some());

        let summary = store
            .replace_with_summary(bot.id, &[first.id, second.id], "A combined fact", 0.8)
            .await
            .unwrap();
        assert_eq!(summary.source, "summary");
        assert_eq!(summary.content, "A combined fact");
        assert!(store.get(bot.id, first.id).await.unwrap().is_none());
        assert!(store.get(bot.id, second.id).await.unwrap().is_none());
        let facts = store.list_limited(bot.id, 10).await.unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].id, summary.id);
        assert!(store.get(other.id, foreign.id).await.unwrap().is_some());

        drop(db);
        cleanup(&path);
    }

    #[tokio::test]
    async fn facts_are_updated_and_deleted_only_for_their_bot() {
        let (db, path) = test_db("facts").await;
        let bot = Bot::new("Memory Keeper", "test");
        let other = Bot::new("Other Keeper", "test");
        queries::BotQueries::insert(db.pool(), &bot).await.unwrap();
        queries::BotQueries::insert(db.pool(), &other)
            .await
            .unwrap();
        let store = MemoryStore::new(db.pool().clone(), Box::new(LocalEmbedding::new(64)));

        let fact = store
            .add(bot.id, "  Prefer small releases  ", 0.6)
            .await
            .expect("add fact");
        assert_eq!(fact.content, "Prefer small releases");
        assert!(!fact.content.contains("  "));

        let updated = store
            .update(bot.id, fact.id, "Prefer small, signed releases", 0.9)
            .await
            .expect("update fact")
            .expect("fact exists");
        assert_eq!(updated.content, "Prefer small, signed releases");
        assert_eq!(updated.importance, 0.9);

        assert!(
            store
                .update(other.id, fact.id, "stolen", 0.5)
                .await
                .expect("scoped update")
                .is_none(),
            "another bot must not update the fact"
        );
        assert!(
            !store
                .delete_for_bot(other.id, fact.id)
                .await
                .expect("scoped delete"),
            "another bot must not delete the fact"
        );

        let listed = store.list_limited(bot.id, 10).await.expect("list facts");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].content, "Prefer small, signed releases");

        assert!(store
            .delete_for_bot(bot.id, fact.id)
            .await
            .expect("delete fact"));
        assert!(store
            .get(bot.id, fact.id)
            .await
            .expect("get fact")
            .is_none());

        drop(db);
        cleanup(&path);
    }
}
