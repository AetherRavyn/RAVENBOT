use crate::embedding::{cosine_similarity, EmbeddingProvider};
use ravenbot_core::OfficeMemory;
use sqlx::{Connection, SqlitePool};
use uuid::Uuid;

/// Shared office knowledge is still a set of facts, not a document store.
const MAX_MEMORY_CHARS: usize = 2000;
const MAX_CATEGORY_CHARS: usize = 40;

pub struct OfficeMemoryStore {
    pool: SqlitePool,
    embedding_provider: Box<dyn EmbeddingProvider>,
}

impl OfficeMemoryStore {
    pub fn new(pool: SqlitePool, embedding_provider: Box<dyn EmbeddingProvider>) -> Self {
        Self {
            pool,
            embedding_provider,
        }
    }

    pub async fn add(
        &self,
        chatroom_id: Uuid,
        content: &str,
        category: &str,
        created_by: Option<Uuid>,
        importance: f32,
    ) -> Result<OfficeMemory, String> {
        self.add_with_provenance(
            chatroom_id,
            content,
            category,
            created_by,
            importance,
            "unknown",
            None,
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn add_with_provenance(
        &self,
        chatroom_id: Uuid,
        content: &str,
        category: &str,
        created_by: Option<Uuid>,
        importance: f32,
        source: &str,
        source_thread_id: Option<Uuid>,
        source_run_id: Option<Uuid>,
    ) -> Result<OfficeMemory, String> {
        let _guard = crate::MEMORY_INDEX_LOCK.read().await;
        let content = normalize_content(content);
        if content.is_empty() {
            return Err("Memory content cannot be empty.".to_string());
        }
        if !importance.is_finite() || !(0.0..=1.0).contains(&importance) {
            return Err("Memory importance must be between 0 and 1.".to_string());
        }
        let category = normalize_category(category);
        let source = normalize_source(source);
        let embedding = self
            .embedding_provider
            .embed(&content)
            .await
            .map_err(|e| e.to_string())?;
        let now = chrono::Utc::now();
        let mem = OfficeMemory {
            id: Uuid::new_v4(),
            chatroom_id,
            content,
            embedding: Some(embedding.clone()),
            importance,
            category: category.to_string(),
            created_by,
            access_count: 0,
            last_accessed: now,
            created_at: now,
            source,
            source_thread_id,
            source_run_id,
            updated_at: now,
        };
        let bytes = serialize(&embedding);
        sqlx::query(
            "INSERT INTO office_memories
             (id, chatroom_id, content, embedding, importance, category, created_by,
              access_count, last_accessed, created_at, source, source_thread_id,
              source_run_id, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(mem.id.to_string())
        .bind(mem.chatroom_id.to_string())
        .bind(&mem.content)
        .bind(&bytes)
        .bind(mem.importance)
        .bind(&mem.category)
        .bind(mem.created_by.map(|u| u.to_string()))
        .bind(mem.access_count as i32)
        .bind(mem.last_accessed.to_rfc3339())
        .bind(mem.created_at.to_rfc3339())
        .bind(&mem.source)
        .bind(mem.source_thread_id.map(|id| id.to_string()))
        .bind(mem.source_run_id.map(|id| id.to_string()))
        .bind(mem.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        tracing::info!(office=%chatroom_id, source = %mem.source, "office memory added");
        Ok(mem)
    }
    /// Fetch one office memory scoped to its office.
    pub async fn get(
        &self,
        chatroom_id: Uuid,
        memory_id: Uuid,
    ) -> Result<Option<OfficeMemory>, String> {
        let row: Option<OfficeMemoryRow> = sqlx::query_as(
            "SELECT id, chatroom_id, content, embedding, importance, category, created_by,
                    access_count, last_accessed, created_at, source, source_thread_id,
                    source_run_id, updated_at
             FROM office_memories
             WHERE id = ? AND chatroom_id = ?",
        )
        .bind(memory_id.to_string())
        .bind(chatroom_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        Ok(row.and_then(memory_from_row))
    }

    /// Update shared knowledge in place and re-embed the new content.
    pub async fn update(
        &self,
        chatroom_id: Uuid,
        memory_id: Uuid,
        content: &str,
        category: &str,
        importance: f32,
    ) -> Result<Option<OfficeMemory>, String> {
        let _guard = crate::MEMORY_INDEX_LOCK.read().await;
        let content = normalize_content(content);
        if content.is_empty() {
            return Err("Memory content cannot be empty.".to_string());
        }
        if !importance.is_finite() || !(0.0..=1.0).contains(&importance) {
            return Err("Memory importance must be between 0 and 1.".to_string());
        }
        let category = normalize_category(category);
        let embedding = self
            .embedding_provider
            .embed(&content)
            .await
            .map_err(|e| e.to_string())?;
        let bytes = serialize(&embedding);
        let result = sqlx::query(
            "UPDATE office_memories
             SET content = ?, embedding = ?, category = ?, importance = ?, updated_at = ?
             WHERE id = ? AND chatroom_id = ?",
        )
        .bind(&content)
        .bind(&bytes)
        .bind(&category)
        .bind(importance)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(memory_id.to_string())
        .bind(chatroom_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        if result.rows_affected() == 0 {
            return Ok(None);
        }
        self.get(chatroom_id, memory_id).await
    }

    /// Replace several shared memories with one summary in a single transaction.
    pub async fn replace_with_summary(
        &self,
        chatroom_id: Uuid,
        source_ids: &[Uuid],
        content: &str,
        importance: f32,
        category: &str,
    ) -> Result<OfficeMemory, String> {
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
        let category = normalize_category(category);
        let embedding = self
            .embedding_provider
            .embed(&content)
            .await
            .map_err(|e| e.to_string())?;
        let now = chrono::Utc::now();
        let memory = OfficeMemory {
            id: Uuid::new_v4(),
            chatroom_id,
            content,
            embedding: Some(embedding.clone()),
            importance,
            category: category.clone(),
            created_by: None,
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
                sqlx::query_as("SELECT id FROM office_memories WHERE id = ? AND chatroom_id = ?")
                    .bind(id.to_string())
                    .bind(chatroom_id.to_string())
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(|e| e.to_string())?;
            if row.is_none() {
                return Err("One of those memories no longer exists.".to_string());
            }
        }
        sqlx::query(
            "INSERT INTO office_memories
             (id, chatroom_id, content, embedding, importance, category, created_by,
              access_count, last_accessed, created_at, source, source_thread_id,
              source_run_id, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(memory.id.to_string())
        .bind(memory.chatroom_id.to_string())
        .bind(&memory.content)
        .bind(serialize(&embedding))
        .bind(memory.importance)
        .bind(&memory.category)
        .bind(Option::<String>::None)
        .bind(memory.access_count)
        .bind(memory.last_accessed.to_rfc3339())
        .bind(memory.created_at.to_rfc3339())
        .bind(&memory.source)
        .bind(Option::<String>::None)
        .bind(Option::<String>::None)
        .bind(memory.updated_at.to_rfc3339())
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        for id in source_ids {
            sqlx::query("DELETE FROM office_memories WHERE id = ? AND chatroom_id = ?")
                .bind(id.to_string())
                .bind(chatroom_id.to_string())
                .execute(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
        }
        tx.commit().await.map_err(|e| e.to_string())?;
        Ok(memory)
    }

    /// Delete shared knowledge only when it belongs to the given office.
    pub async fn delete(&self, chatroom_id: Uuid, memory_id: Uuid) -> Result<bool, String> {
        let _guard = crate::MEMORY_INDEX_LOCK.read().await;
        let result = sqlx::query("DELETE FROM office_memories WHERE id = ? AND chatroom_id = ?")
            .bind(memory_id.to_string())
            .bind(chatroom_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
        Ok(result.rows_affected() > 0)
    }

    /// Re-embed every shared memory with a new provider in one transaction.
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
            "CREATE TEMP TABLE IF NOT EXISTS office_memory_reindex_stage (
                id TEXT PRIMARY KEY NOT NULL,
                embedding BLOB NOT NULL
            )",
        )
        .execute(&mut *conn)
        .await
        .map_err(|e| e.to_string())?;
        sqlx::query("DELETE FROM office_memory_reindex_stage")
            .execute(&mut *conn)
            .await
            .map_err(|e| e.to_string())?;
        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT id, content FROM office_memories ORDER BY created_at ASC")
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
                sqlx::query(
                    "INSERT INTO office_memory_reindex_stage (id, embedding) VALUES (?, ?)",
                )
                .bind(id)
                .bind(serialize(&embedding))
                .execute(&mut *conn)
                .await
                .map_err(|e| e.to_string())?;
            }
        }
        let mut tx = conn.begin().await.map_err(|e| e.to_string())?;
        sqlx::query(
            "UPDATE office_memories
             SET embedding = (
                SELECT embedding FROM office_memory_reindex_stage
                WHERE office_memory_reindex_stage.id = office_memories.id
             )
             WHERE id IN (SELECT id FROM office_memory_reindex_stage)",
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        sqlx::query("DELETE FROM office_memory_reindex_stage")
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        tx.commit().await.map_err(|e| e.to_string())?;
        Ok(rows.len() as u64)
    }
    pub async fn retrieve(
        &self,
        chatroom_id: Uuid,
        query: &str,
        limit: usize,
        threshold: f32,
    ) -> Result<Vec<(OfficeMemory, f32)>, String> {
        let qemb = self
            .embedding_provider
            .embed(query)
            .await
            .map_err(|e| e.to_string())?;
        let rows: Vec<OfficeMemoryRow> = sqlx::query_as(
            "SELECT id, chatroom_id, content, embedding, importance, category, created_by, access_count, last_accessed, created_at, source, source_thread_id, source_run_id, updated_at FROM office_memories WHERE chatroom_id = ?"
        ).bind(chatroom_id.to_string()).fetch_all(&self.pool).await.map_err(|e| e.to_string())?;
        let mut scored: Vec<(OfficeMemory, f32)> = rows
            .into_iter()
            .filter_map(|r| {
                let emb = r.3.as_ref().and_then(|b| deserialize(b))?;
                let sim = cosine_similarity(&qemb, &emb);
                if sim < threshold {
                    return None;
                }
                let mem = OfficeMemory {
                    id: Uuid::parse_str(&r.0).ok()?,
                    chatroom_id: Uuid::parse_str(&r.1).ok()?,
                    content: r.2,
                    embedding: Some(emb),
                    importance: r.4,
                    category: r.5,
                    created_by: r.6.and_then(|s| Uuid::parse_str(&s).ok()),
                    access_count: r.7 as u32,
                    last_accessed: chrono::DateTime::parse_from_rfc3339(&r.8)
                        .ok()
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(chrono::Utc::now),
                    created_at: chrono::DateTime::parse_from_rfc3339(&r.9)
                        .ok()
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(chrono::Utc::now),
                    source: r.10,
                    source_thread_id: r
                        .11
                        .as_deref()
                        .and_then(|value| Uuid::parse_str(value).ok()),
                    source_run_id: r
                        .12
                        .as_deref()
                        .and_then(|value| Uuid::parse_str(value).ok()),
                    updated_at: chrono::DateTime::parse_from_rfc3339(&r.13)
                        .ok()
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(chrono::Utc::now),
                };
                let score = sim * (0.6 + 0.4 * mem.importance);
                Some((mem, score))
            })
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let res: Vec<_> = scored.into_iter().take(limit).collect();
        for (m, _) in &res {
            let _ = sqlx::query("UPDATE office_memories SET access_count=access_count+1, last_accessed=? WHERE id=?")
                .bind(chrono::Utc::now().to_rfc3339()).bind(m.id.to_string()).execute(&self.pool).await;
        }
        Ok(res)
    }

    pub async fn list(&self, chatroom_id: Uuid) -> Result<Vec<OfficeMemory>, String> {
        let rows: Vec<OfficeMemoryRow> = sqlx::query_as(
            "SELECT id, chatroom_id, content, embedding, importance, category, created_by, access_count, last_accessed, created_at, source, source_thread_id, source_run_id, updated_at FROM office_memories WHERE chatroom_id = ? ORDER BY importance DESC, created_at DESC"
        ).bind(chatroom_id.to_string()).fetch_all(&self.pool).await.map_err(|e| e.to_string())?;
        Ok(rows.into_iter().filter_map(memory_from_row).collect())
    }

    pub async fn count(&self, chatroom_id: Uuid) -> Result<u64, String> {
        let (c,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM office_memories WHERE chatroom_id = ?")
                .bind(chatroom_id.to_string())
                .fetch_one(&self.pool)
                .await
                .map_err(|e| e.to_string())?;
        Ok(c as u64)
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

fn normalize_category(category: &str) -> String {
    let category = category.trim();
    if category.is_empty() {
        return "general".to_string();
    }
    if category.chars().count() > MAX_CATEGORY_CHARS {
        category
            .chars()
            .take(MAX_CATEGORY_CHARS)
            .collect::<String>()
    } else {
        category.to_string()
    }
}

type OfficeMemoryRow = (
    String,
    String,
    String,
    Option<Vec<u8>>,
    f32,
    String,
    Option<String>,
    i32,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
);

fn memory_from_row(row: OfficeMemoryRow) -> Option<OfficeMemory> {
    Some(OfficeMemory {
        id: Uuid::parse_str(&row.0).ok()?,
        chatroom_id: Uuid::parse_str(&row.1).ok()?,
        content: row.2,
        embedding: row.3.as_ref().and_then(|bytes| deserialize(bytes)),
        importance: row.4,
        category: row.5,
        created_by: row.6.and_then(|value| Uuid::parse_str(&value).ok()),
        access_count: row.7 as u32,
        last_accessed: chrono::DateTime::parse_from_rfc3339(&row.8)
            .ok()
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(chrono::Utc::now),
        created_at: chrono::DateTime::parse_from_rfc3339(&row.9)
            .ok()
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(chrono::Utc::now),
        source: row.10,
        source_thread_id: row
            .11
            .as_deref()
            .and_then(|value| Uuid::parse_str(value).ok()),
        source_run_id: row
            .12
            .as_deref()
            .and_then(|value| Uuid::parse_str(value).ok()),
        updated_at: chrono::DateTime::parse_from_rfc3339(&row.13)
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

fn serialize(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}
fn deserialize(b: &[u8]) -> Option<Vec<f32>> {
    if !b.len().is_multiple_of(4) {
        return None;
    }
    Some(
        b.chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedding::LocalEmbedding;
    use ravenbot_db::{queries, Database};

    async fn test_db(name: &str) -> (Database, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "ravenbot-office-memory-{name}-{}-{}.db",
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
    async fn reindex_replaces_shared_vectors() {
        let (db, path) = test_db("reindex").await;
        let office = ravenbot_core::ChatRoom::new("Office", "test", "custom");
        queries::ChatRoomQueries::create(db.pool(), &office)
            .await
            .unwrap();
        let store = OfficeMemoryStore::new(db.pool().clone(), Box::new(LocalEmbedding::new(64)));
        let memory = store
            .add(office.id, "shared fact", "fact", None, 0.5)
            .await
            .unwrap();
        assert_eq!(memory.embedding.as_ref().map(Vec::len), Some(64));

        assert_eq!(
            store
                .reindex_with(&FixedEmbedding(vec![0.1, 0.2, 0.3]))
                .await
                .unwrap(),
            1
        );
        let updated = store.get(office.id, memory.id).await.unwrap().unwrap();
        assert_eq!(updated.embedding.as_ref().map(Vec::len), Some(3));

        drop(db);
        cleanup(&path);
    }

    #[tokio::test]
    async fn office_provenance_is_persisted_and_preserved_on_update() {
        let (db, path) = test_db("provenance").await;
        let office = ravenbot_core::ChatRoom::new("Office", "test", "custom");
        queries::ChatRoomQueries::create(db.pool(), &office)
            .await
            .unwrap();
        let store = OfficeMemoryStore::new(db.pool().clone(), Box::new(LocalEmbedding::new(64)));
        let memory = store
            .add_with_provenance(
                office.id,
                "Added by hand",
                "rule",
                None,
                0.6,
                "user",
                None,
                None,
            )
            .await
            .expect("add memory");
        assert_eq!(memory.source, "user");

        let updated = store
            .update(
                office.id,
                memory.id,
                "Added by hand and edited",
                "rule",
                0.8,
            )
            .await
            .expect("update")
            .expect("memory exists");
        assert_eq!(updated.source, "user");
        assert!(updated.updated_at >= memory.created_at);

        let listed = store.list(office.id).await.unwrap();
        assert_eq!(listed[0].source, "user");

        drop(db);
        cleanup(&path);
    }

    #[tokio::test]
    async fn summary_replaces_selected_office_memories_atomically() {
        let (db, path) = test_db("summary").await;
        let office = ravenbot_core::ChatRoom::new("Office", "test", "custom");
        let other = ravenbot_core::ChatRoom::new("Other Office", "test", "custom");
        queries::ChatRoomQueries::create(db.pool(), &office)
            .await
            .unwrap();
        queries::ChatRoomQueries::create(db.pool(), &other)
            .await
            .unwrap();
        let store = OfficeMemoryStore::new(db.pool().clone(), Box::new(LocalEmbedding::new(64)));
        let first = store
            .add(office.id, "first shared fact", "rule", None, 0.5)
            .await
            .unwrap();
        let second = store
            .add(office.id, "second shared fact", "process", None, 0.8)
            .await
            .unwrap();
        let foreign = store
            .add(other.id, "other office fact", "rule", None, 0.5)
            .await
            .unwrap();

        assert!(store
            .replace_with_summary(
                office.id,
                &[first.id, foreign.id],
                "cross-office summary",
                0.8,
                "rule",
            )
            .await
            .is_err());
        assert_eq!(store.list(office.id).await.unwrap().len(), 2);

        let summary = store
            .replace_with_summary(
                office.id,
                &[first.id, second.id],
                "A combined shared fact",
                0.8,
                "process",
            )
            .await
            .unwrap();
        assert_eq!(summary.source, "summary");
        assert_eq!(summary.category, "process");
        assert!(store.get(office.id, first.id).await.unwrap().is_none());
        assert!(store.get(office.id, second.id).await.unwrap().is_none());
        let memories = store.list(office.id).await.unwrap();
        assert_eq!(memories.len(), 1);
        assert_eq!(memories[0].id, summary.id);
        assert!(store.get(other.id, foreign.id).await.unwrap().is_some());

        drop(db);
        cleanup(&path);
    }

    #[tokio::test]
    async fn office_memories_are_updated_and_deleted_only_in_their_office() {
        let (db, path) = test_db("shared").await;
        let office = ravenbot_core::ChatRoom::new("Office", "test", "custom");
        let other = ravenbot_core::ChatRoom::new("Other Office", "test", "custom");
        queries::ChatRoomQueries::create(db.pool(), &office)
            .await
            .unwrap();
        queries::ChatRoomQueries::create(db.pool(), &other)
            .await
            .unwrap();
        let store = OfficeMemoryStore::new(db.pool().clone(), Box::new(LocalEmbedding::new(64)));

        let memory = store
            .add(office.id, "  Sign every release  ", "rule", None, 0.9)
            .await
            .expect("add memory");
        assert_eq!(memory.content, "Sign every release");
        assert_eq!(memory.category, "rule");

        let updated = store
            .update(
                office.id,
                memory.id,
                "Sign and archive every release",
                "process",
                0.8,
            )
            .await
            .expect("update memory")
            .expect("memory exists");
        assert_eq!(updated.content, "Sign and archive every release");
        assert_eq!(updated.category, "process");
        assert_eq!(updated.importance, 0.8);

        assert!(
            store
                .update(other.id, memory.id, "hijacked", "rule", 0.5)
                .await
                .expect("scoped update")
                .is_none(),
            "another office must not update the memory"
        );
        assert!(
            !store
                .delete(other.id, memory.id)
                .await
                .expect("scoped delete"),
            "another office must not delete the memory"
        );

        let loaded = store
            .get(office.id, memory.id)
            .await
            .expect("get memory")
            .expect("memory exists");
        assert_eq!(loaded.content, "Sign and archive every release");

        assert!(store
            .delete(office.id, memory.id)
            .await
            .expect("delete memory"));
        assert!(store
            .get(office.id, memory.id)
            .await
            .expect("get")
            .is_none());

        drop(db);
        cleanup(&path);
    }
}
