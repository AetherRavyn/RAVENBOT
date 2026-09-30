use sqlx::SqlitePool;
use uuid::Uuid;
use chrono::Utc;
use ravenbot_core::{AgentLearning, AgentIntelligence};

pub struct LearningEngine {
    pool: SqlitePool,
}

impl LearningEngine {
    pub fn new(pool: SqlitePool) -> Self { Self { pool } }

    /// Record a learning — called after every task, makes agent smarter
    pub async fn record(&self, bot_id: Uuid, chatroom_id: Option<Uuid>, learning_type: &str, content: &str, success: bool) -> Result<(), String> {
        let now = Utc::now();
        sqlx::query(
            "INSERT INTO agent_learnings (id, bot_id, chatroom_id, learning_type, content, success_rate, tasks_completed, tasks_failed, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(Uuid::new_v4().to_string())
        .bind(bot_id.to_string())
        .bind(chatroom_id.map(|u| u.to_string()))
        .bind(learning_type)
        .bind(content)
        .bind(if success { 1.0 } else { 0.0 })
        .bind(if success { 1 } else { 0 })
        .bind(if success { 0 } else { 1 })
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .execute(&self.pool).await.map_err(|e| e.to_string())?;

        // Update intelligence snapshot — agent gets smarter
        self.update_intelligence(bot_id).await?;
        tracing::info!(%bot_id, learning_type, "learning recorded");
        Ok(())
    }

    async fn update_intelligence(&self, bot_id: Uuid) -> Result<(), String> {
        let (memories,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM memory_facts WHERE bot_id = ?")
            .bind(bot_id.to_string()).fetch_one(&self.pool).await.map_err(|e| e.to_string())?;
        let (office_mems,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM office_memories WHERE created_by = ?")
            .bind(bot_id.to_string()).fetch_one(&self.pool).await.map_err(|e| e.to_string()).unwrap_or((0,));
        let (learnings,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM agent_learnings WHERE bot_id = ?")
            .bind(bot_id.to_string()).fetch_one(&self.pool).await.map_err(|e| e.to_string())?;
        let (completed,): (i64,) = sqlx::query_as("SELECT COALESCE(SUM(tasks_completed),0) FROM agent_learnings WHERE bot_id = ?")
            .bind(bot_id.to_string()).fetch_one(&self.pool).await.map_err(|e| e.to_string())?;
        let (failed,): (i64,) = sqlx::query_as("SELECT COALESCE(SUM(tasks_failed),0) FROM agent_learnings WHERE bot_id = ?")
            .bind(bot_id.to_string()).fetch_one(&self.pool).await.map_err(|e| e.to_string())?;
        let total = (completed + failed).max(1) as f32;
        let success_rate = completed as f32 / total;
        let streak: i64 = completed; // simplified
        // Intelligence score grows with data: memories + learnings + success
        let score = ((memories as f32 * 0.1 + office_mems as f32 * 0.15 + learnings as f32 * 0.2 + success_rate * 0.5).min(1.0) * 100.0).round() / 100.0;

        sqlx::query(
            "INSERT OR REPLACE INTO agent_intelligence (bot_id, total_memories, office_memories, learnings_count, intelligence_score, tasks_today, success_streak, last_active, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(bot_id.to_string())
        .bind(memories)
        .bind(office_mems)
        .bind(learnings)
        .bind(score)
        .bind(completed)
        .bind(streak)
        .bind(Utc::now().to_rfc3339())
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool).await.map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn get_intelligence(&self, bot_id: Uuid) -> Result<AgentIntelligence, String> {
        let row: Option<(String, i64, i64, i64, f64, i64, i64, Option<String>, String)> = sqlx::query_as(
            "SELECT bot_id, total_memories, office_memories, learnings_count, intelligence_score, tasks_today, success_streak, last_active, updated_at FROM agent_intelligence WHERE bot_id = ?"
        ).bind(bot_id.to_string()).fetch_optional(&self.pool).await.map_err(|e| e.to_string())?;
        if let Some(r) = row {
            Ok(AgentIntelligence {
                bot_id: Uuid::parse_str(&r.0).unwrap_or(bot_id),
                total_memories: r.1 as u32,
                office_memories: r.2 as u32,
                learnings_count: r.3 as u32,
                intelligence_score: r.4 as f32,
                tasks_today: r.5 as u32,
                success_streak: r.6 as u32,
                last_active: r.7.and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok().map(|dt| dt.with_timezone(&Utc))),
                updated_at: chrono::DateTime::parse_from_rfc3339(&r.8).map(|dt| dt.with_timezone(&Utc)).unwrap_or_else(|_| Utc::now()),
            })
        } else {
            Ok(AgentIntelligence {
                bot_id,
                total_memories: 0,
                office_memories: 0,
                learnings_count: 0,
                intelligence_score: 0.5,
                tasks_today: 0,
                success_streak: 0,
                last_active: None,
                updated_at: Utc::now(),
            })
        }
    }

    pub async fn list_learnings(&self, bot_id: Uuid, limit: u32) -> Result<Vec<AgentLearning>, String> {
        let rows: Vec<(String, String, Option<String>, String, String, Option<String>, f64, i64, i64, String)> = sqlx::query_as(
            "SELECT id, bot_id, chatroom_id, learning_type, content, context, success_rate, tasks_completed, tasks_failed, created_at FROM agent_learnings WHERE bot_id = ? ORDER BY created_at DESC LIMIT ?"
        ).bind(bot_id.to_string()).bind(limit).fetch_all(&self.pool).await.map_err(|e| e.to_string())?;
        Ok(rows.into_iter().filter_map(|r| Some(AgentLearning {
            id: Uuid::parse_str(&r.0).ok()?,
            bot_id: Uuid::parse_str(&r.1).ok()?,
            chatroom_id: r.2.and_then(|s| Uuid::parse_str(&s).ok()),
            learning_type: r.3,
            content: r.4,
            context: r.5,
            success_rate: r.6 as f32,
            tasks_completed: r.7 as u32,
            tasks_failed: r.8 as u32,
            created_at: chrono::DateTime::parse_from_rfc3339(&r.9).ok().map(|dt| dt.with_timezone(&Utc)).unwrap_or_else(Utc::now),
        })).collect())
    }
}
