use crate::embedding::{EmbeddingProvider, cosine_similarity};
use ravenbot_core::OfficeMemory;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct OfficeMemoryStore {
    pool: SqlitePool,
    embedding_provider: Box<dyn EmbeddingProvider>,
}

impl OfficeMemoryStore {
    pub fn new(pool: SqlitePool, embedding_provider: Box<dyn EmbeddingProvider>) -> Self {
        Self { pool, embedding_provider }
    }

    pub async fn add(&self, chatroom_id: Uuid, content: &str, category: &str, created_by: Option<Uuid>, importance: f32) -> Result<OfficeMemory, String> {
        let embedding = self.embedding_provider.embed(content).await.map_err(|e| e.to_string())?;
        let mem = OfficeMemory {
            id: Uuid::new_v4(),
            chatroom_id,
            content: content.to_string(),
            embedding: Some(embedding.clone()),
            importance,
            category: category.to_string(),
            created_by,
            access_count: 0,
            last_accessed: chrono::Utc::now(),
            created_at: chrono::Utc::now(),
        };
        let bytes = serialize(&embedding);
        sqlx::query("INSERT INTO office_memories (id, chatroom_id, content, embedding, importance, category, created_by, access_count, last_accessed, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(mem.id.to_string()).bind(mem.chatroom_id.to_string()).bind(&mem.content).bind(&bytes).bind(mem.importance).bind(&mem.category).bind(mem.created_by.map(|u| u.to_string())).bind(mem.access_count as i32).bind(mem.last_accessed.to_rfc3339()).bind(mem.created_at.to_rfc3339())
            .execute(&self.pool).await.map_err(|e| e.to_string())?;
        tracing::info!(office=%chatroom_id, "office memory added");
        Ok(mem)
    }

    pub async fn retrieve(&self, chatroom_id: Uuid, query: &str, limit: usize, threshold: f32) -> Result<Vec<(OfficeMemory, f32)>, String> {
        let qemb = self.embedding_provider.embed(query).await.map_err(|e| e.to_string())?;
        let rows: Vec<(String, String, String, Option<Vec<u8>>, f32, String, Option<String>, i32, String, String)> = sqlx::query_as(
            "SELECT id, chatroom_id, content, embedding, importance, category, created_by, access_count, last_accessed, created_at FROM office_memories WHERE chatroom_id = ?"
        ).bind(chatroom_id.to_string()).fetch_all(&self.pool).await.map_err(|e| e.to_string())?;
        let mut scored: Vec<(OfficeMemory, f32)> = rows.into_iter().filter_map(|r| {
            let emb = r.3.as_ref().and_then(|b| deserialize(b))?;
            let sim = cosine_similarity(&qemb, &emb);
            if sim < threshold { return None; }
            let mem = OfficeMemory {
                id: Uuid::parse_str(&r.0).ok()?,
                chatroom_id: Uuid::parse_str(&r.1).ok()?,
                content: r.2,
                embedding: Some(emb),
                importance: r.4,
                category: r.5,
                created_by: r.6.and_then(|s| Uuid::parse_str(&s).ok()),
                access_count: r.7 as u32,
                last_accessed: chrono::DateTime::parse_from_rfc3339(&r.8).ok().map(|dt| dt.with_timezone(&chrono::Utc)).unwrap_or_else(chrono::Utc::now),
                created_at: chrono::DateTime::parse_from_rfc3339(&r.9).ok().map(|dt| dt.with_timezone(&chrono::Utc)).unwrap_or_else(chrono::Utc::now),
            };
            let score = sim * (0.6 + 0.4 * mem.importance);
            Some((mem, score))
        }).collect();
        scored.sort_by(|a,b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let res: Vec<_> = scored.into_iter().take(limit).collect();
        for (m, _) in &res {
            let _ = sqlx::query("UPDATE office_memories SET access_count=access_count+1, last_accessed=? WHERE id=?")
                .bind(chrono::Utc::now().to_rfc3339()).bind(m.id.to_string()).execute(&self.pool).await;
        }
        Ok(res)
    }

    pub async fn list(&self, chatroom_id: Uuid) -> Result<Vec<OfficeMemory>, String> {
        let rows: Vec<(String, String, String, Option<Vec<u8>>, f32, String, Option<String>, i32, String, String)> = sqlx::query_as(
            "SELECT id, chatroom_id, content, embedding, importance, category, created_by, access_count, last_accessed, created_at FROM office_memories WHERE chatroom_id = ? ORDER BY importance DESC, created_at DESC"
        ).bind(chatroom_id.to_string()).fetch_all(&self.pool).await.map_err(|e| e.to_string())?;
        Ok(rows.into_iter().filter_map(|r| Some(OfficeMemory {
            id: Uuid::parse_str(&r.0).ok()?,
            chatroom_id: Uuid::parse_str(&r.1).ok()?,
            content: r.2,
            embedding: r.3.as_ref().and_then(|b| deserialize(b)),
            importance: r.4,
            category: r.5,
            created_by: r.6.and_then(|s| Uuid::parse_str(&s).ok()),
            access_count: r.7 as u32,
            last_accessed: chrono::DateTime::parse_from_rfc3339(&r.8).ok().map(|dt| dt.with_timezone(&chrono::Utc)).unwrap_or_else(chrono::Utc::now),
            created_at: chrono::DateTime::parse_from_rfc3339(&r.9).ok().map(|dt| dt.with_timezone(&chrono::Utc)).unwrap_or_else(chrono::Utc::now),
        })).collect())
    }

    pub async fn count(&self, chatroom_id: Uuid) -> Result<u64, String> {
        let (c,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM office_memories WHERE chatroom_id = ?")
            .bind(chatroom_id.to_string()).fetch_one(&self.pool).await.map_err(|e| e.to_string())?;
        Ok(c as u64)
    }
}

fn serialize(v: &[f32]) -> Vec<u8> { v.iter().flat_map(|f| f.to_le_bytes()).collect() }
fn deserialize(b: &[u8]) -> Option<Vec<f32>> {
    if !b.len().is_multiple_of(4) { return None; }
    Some(b.chunks_exact(4).map(|c| f32::from_le_bytes([c[0],c[1],c[2],c[3]])).collect())
}
