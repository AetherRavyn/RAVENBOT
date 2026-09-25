//! Database query functions

use sqlx::SqlitePool;
use uuid::Uuid;

use crate::models::{BotRow, ThreadRow, MessageRow, RunRow};

/// Bot-related queries
pub struct BotQueries;

impl BotQueries {
    /// Does the bots table have the migration-009 `skills` column?
    async fn has_skills_column(pool: &SqlitePool) -> bool {
        Self::has_column(pool, "skills").await
    }

    /// Does the bots table have the migration-010 `approval_mode` column?
    async fn has_approval_column(pool: &SqlitePool) -> bool {
        Self::has_column(pool, "approval_mode").await
    }

    async fn has_column(pool: &SqlitePool, name: &str) -> bool {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM pragma_table_info('bots') WHERE name = ?")
            .bind(name)
            .fetch_one(pool)
            .await
            .unwrap_or(0)
            > 0
    }

    /// Overlay junction-table skills onto bots loaded without the column
    /// (pre-009 databases).
    async fn backfill_skills_from_junction(
        pool: &SqlitePool,
        bots: &mut [ravenbot_core::Bot],
    ) {
        for bot in bots.iter_mut() {
            let ids: Vec<(String,)> =
                sqlx::query_as("SELECT skill_id FROM bot_skills WHERE bot_id = ?")
                    .bind(bot.id.to_string())
                    .fetch_all(pool)
                    .await
                    .unwrap_or_default();
            if !ids.is_empty() {
                bot.skills = ids.into_iter().map(|(s,)| s).collect();
            }
        }
    }

    /// Sync the junction table from the canonical JSON (keeps old readers working).
    async fn sync_junction(pool: &SqlitePool, bot: &ravenbot_core::Bot) {
        let _ = sqlx::query("DELETE FROM bot_skills WHERE bot_id = ?")
            .bind(bot.id.to_string())
            .execute(pool)
            .await;
        for skill_id in &bot.skills {
            let _ = sqlx::query(
                "INSERT OR IGNORE INTO bot_skills (bot_id, skill_id) VALUES (?, ?)",
            )
            .bind(bot.id.to_string())
            .bind(skill_id)
            .execute(pool)
            .await;
        }
    }

    /// Insert a new bot
    pub async fn insert(pool: &SqlitePool, bot: &ravenbot_core::Bot) -> Result<(), sqlx::Error> {
        let row = BotRow::from_domain(bot).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        let approval = row.approval_mode.clone().unwrap_or_else(|| "ask".to_string());

        if Self::has_skills_column(pool).await && Self::has_approval_column(pool).await {
            sqlx::query(
                "INSERT INTO bots (id, name, description, avatar_color, avatar_url, avatar_style, rank, specialty, status, config, permissions, is_orchestrator, delegate_to, skills, approval_mode, created_at, updated_at, last_active_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(&row.id)
            .bind(&row.name)
            .bind(&row.description)
            .bind(&row.avatar_color)
            .bind(&row.avatar_url)
            .bind(&row.avatar_style)
            .bind(&row.rank)
            .bind(&row.specialty)
            .bind(&row.status)
            .bind(&row.config)
            .bind(&row.permissions)
            .bind(row.is_orchestrator)
            .bind(&row.delegate_to)
            .bind(&row.skills)
            .bind(&approval)
            .bind(&row.created_at)
            .bind(&row.updated_at)
            .bind(&row.last_active_at)
            .execute(pool)
            .await?;
        } else if Self::has_skills_column(pool).await {
            sqlx::query(
                "INSERT INTO bots (id, name, description, avatar_color, avatar_url, avatar_style, rank, specialty, status, config, permissions, is_orchestrator, delegate_to, skills, created_at, updated_at, last_active_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(&row.id)
            .bind(&row.name)
            .bind(&row.description)
            .bind(&row.avatar_color)
            .bind(&row.avatar_url)
            .bind(&row.avatar_style)
            .bind(&row.rank)
            .bind(&row.specialty)
            .bind(&row.status)
            .bind(&row.config)
            .bind(&row.permissions)
            .bind(row.is_orchestrator)
            .bind(&row.delegate_to)
            .bind(&row.skills)
            .bind(&row.created_at)
            .bind(&row.updated_at)
            .bind(&row.last_active_at)
            .execute(pool)
            .await?;
        } else {
            sqlx::query(
                "INSERT INTO bots (id, name, description, avatar_color, avatar_url, avatar_style, rank, specialty, status, config, permissions, is_orchestrator, delegate_to, created_at, updated_at, last_active_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(&row.id)
            .bind(&row.name)
            .bind(&row.description)
            .bind(&row.avatar_color)
            .bind(&row.avatar_url)
            .bind(&row.avatar_style)
            .bind(&row.rank)
            .bind(&row.specialty)
            .bind(&row.status)
            .bind(&row.config)
            .bind(&row.permissions)
            .bind(row.is_orchestrator)
            .bind(&row.delegate_to)
            .bind(&row.created_at)
            .bind(&row.updated_at)
            .bind(&row.last_active_at)
            .execute(pool)
            .await?;
        }
        Self::sync_junction(pool, bot).await;

        Ok(())
    }

    /// Get a bot by ID
    pub async fn get(pool: &SqlitePool, id: Uuid) -> Result<Option<ravenbot_core::Bot>, sqlx::Error> {
        // `SELECT *` maps onto BotRow.skills as Option — missing columns on
        // old DBs decode as None, so this is safe pre- and post-009.
        let row: Option<BotRow> = sqlx::query_as("SELECT * FROM bots WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(pool)
            .await?;

        match row {
            Some(row) => {
                let mut bot =
                    row.to_domain().map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
                if row.skills.is_none() {
                    Self::backfill_skills_from_junction(pool, std::slice::from_mut(&mut bot))
                        .await;
                }
                Ok(Some(bot))
            }
            None => Ok(None),
        }
    }

    /// Get all bots
    pub async fn list(pool: &SqlitePool) -> Result<Vec<ravenbot_core::Bot>, sqlx::Error> {
        let rows: Vec<BotRow> = sqlx::query_as("SELECT * FROM bots ORDER BY updated_at DESC")
            .fetch_all(pool)
            .await?;

        let needs_backfill = rows.iter().any(|r| r.skills.is_none());
        let mut bots = Vec::new();
        for row in rows {
            bots.push(row.to_domain().map_err(|e| sqlx::Error::Decode(Box::new(e)))?);
        }
        if needs_backfill {
            Self::backfill_skills_from_junction(pool, &mut bots).await;
        }
        Ok(bots)
    }

    /// Update a bot
    pub async fn update(pool: &SqlitePool, bot: &ravenbot_core::Bot) -> Result<(), sqlx::Error> {
        let row = BotRow::from_domain(bot).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        let approval = row.approval_mode.clone().unwrap_or_else(|| "ask".to_string());

        if Self::has_skills_column(pool).await && Self::has_approval_column(pool).await {
            sqlx::query(
                "UPDATE bots SET name = ?, description = ?, avatar_color = ?, avatar_url = ?, avatar_style = ?, rank = ?, specialty = ?, status = ?, config = ?, permissions = ?, is_orchestrator = ?, delegate_to = ?, skills = ?, approval_mode = ?, updated_at = ?, last_active_at = ?
                 WHERE id = ?"
            )
            .bind(&row.name)
            .bind(&row.description)
            .bind(&row.avatar_color)
            .bind(&row.avatar_url)
            .bind(&row.avatar_style)
            .bind(&row.rank)
            .bind(&row.specialty)
            .bind(&row.status)
            .bind(&row.config)
            .bind(&row.permissions)
            .bind(row.is_orchestrator)
            .bind(&row.delegate_to)
            .bind(&row.skills)
            .bind(&approval)
            .bind(&row.updated_at)
            .bind(&row.last_active_at)
            .bind(&row.id)
            .execute(pool)
            .await?;
        } else if Self::has_skills_column(pool).await {
            sqlx::query(
                "UPDATE bots SET name = ?, description = ?, avatar_color = ?, avatar_url = ?, avatar_style = ?, rank = ?, specialty = ?, status = ?, config = ?, permissions = ?, is_orchestrator = ?, delegate_to = ?, skills = ?, updated_at = ?, last_active_at = ?
                 WHERE id = ?"
            )
            .bind(&row.name)
            .bind(&row.description)
            .bind(&row.avatar_color)
            .bind(&row.avatar_url)
            .bind(&row.avatar_style)
            .bind(&row.rank)
            .bind(&row.specialty)
            .bind(&row.status)
            .bind(&row.config)
            .bind(&row.permissions)
            .bind(row.is_orchestrator)
            .bind(&row.delegate_to)
            .bind(&row.skills)
            .bind(&row.updated_at)
            .bind(&row.last_active_at)
            .bind(&row.id)
            .execute(pool)
            .await?;
        } else {
            sqlx::query(
                "UPDATE bots SET name = ?, description = ?, avatar_color = ?, avatar_url = ?, avatar_style = ?, status = ?, config = ?, permissions = ?, is_orchestrator = ?, delegate_to = ?, updated_at = ?, last_active_at = ?
                 WHERE id = ?"
            )
            .bind(&row.name)
            .bind(&row.description)
            .bind(&row.avatar_color)
            .bind(&row.avatar_url)
            .bind(&row.avatar_style)
            .bind(&row.rank)
            .bind(&row.specialty)
            .bind(&row.status)
            .bind(&row.config)
            .bind(&row.permissions)
            .bind(row.is_orchestrator)
            .bind(&row.delegate_to)
            .bind(&row.updated_at)
            .bind(&row.last_active_at)
            .bind(&row.id)
            .execute(pool)
            .await?;
        }
        // Keep the legacy junction table in sync for old readers.
        Self::sync_junction(pool, bot).await;

        Ok(())
    }

    /// Delete a bot
    pub async fn delete(pool: &SqlitePool, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM bots WHERE id = ?")
            .bind(id.to_string())
            .execute(pool)
            .await?;
        Ok(())
    }
}

/// Thread-related queries
pub struct ThreadQueries;

impl ThreadQueries {
    /// Create a new thread
    pub async fn create(pool: &SqlitePool, thread: &ravenbot_core::Thread) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO threads (id, bot_id, title, is_active, ephemeral, channel_id, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(thread.id.to_string())
        .bind(thread.bot_id.to_string())
        .bind(&thread.title)
        .bind(thread.is_active)
        .bind(thread.ephemeral)
        .bind(thread.channel_id.map(|u| u.to_string()))
        .bind(thread.created_at.to_rfc3339())
        .bind(thread.updated_at.to_rfc3339())
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Get a thread by ID
    pub async fn get(pool: &SqlitePool, id: Uuid) -> Result<Option<ravenbot_core::Thread>, sqlx::Error> {
        let row: Option<ThreadRow> = sqlx::query_as("SELECT * FROM threads WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(pool)
            .await?;

        match row {
            Some(row) => {
                let created_at = chrono::DateTime::parse_from_rfc3339(&row.created_at)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now());
                let updated_at = chrono::DateTime::parse_from_rfc3339(&row.updated_at)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now());
                
                Ok(Some(ravenbot_core::Thread {
                    id: Uuid::parse_str(&row.id).unwrap_or_default(),
                    bot_id: Uuid::parse_str(&row.bot_id).unwrap_or_default(),
                    title: row.title,
                    is_active: row.is_active,
                    ephemeral: row.ephemeral,
                    channel_id: row.channel_id.as_deref().and_then(|s| Uuid::parse_str(s).ok()),
                    created_at,
                    updated_at,
                }))
            }
            None => Ok(None),
        }
    }

    /// Get threads for a bot
    pub async fn list_by_bot(pool: &SqlitePool, bot_id: Uuid) -> Result<Vec<ravenbot_core::Thread>, sqlx::Error> {
        let rows: Vec<ThreadRow> = sqlx::query_as(
            "SELECT * FROM threads WHERE bot_id = ? ORDER BY updated_at DESC"
        )
        .bind(bot_id.to_string())
        .fetch_all(pool)
        .await?;

        let threads = rows.into_iter().map(|row| {
            let created_at = chrono::DateTime::parse_from_rfc3339(&row.created_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now());
            let updated_at = chrono::DateTime::parse_from_rfc3339(&row.updated_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now());
            
            ravenbot_core::Thread {
                id: Uuid::parse_str(&row.id).unwrap_or_default(),
                bot_id: Uuid::parse_str(&row.bot_id).unwrap_or_default(),
                title: row.title,
                is_active: row.is_active,
                ephemeral: row.ephemeral,
                channel_id: row.channel_id.as_deref().and_then(|s| Uuid::parse_str(s).ok()),
                created_at,
                updated_at,
            }
        }).collect();

        Ok(threads)
    }
}

/// Message-related queries
pub struct MessageQueries;

impl MessageQueries {
    async fn has_message_sender_columns(pool: &SqlitePool) -> bool {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM pragma_table_info('messages') WHERE name = 'sender_bot_id'")
            .fetch_one(pool)
            .await
            .unwrap_or(0)
            > 0
    }

    /// Insert a message
    pub async fn insert(pool: &SqlitePool, message: &ravenbot_core::Message) -> Result<(), sqlx::Error> {
        let content = serde_json::to_string(&message.content)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        let attachments = serde_json::to_string(&message.attachments)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        let role = match message.role {
            ravenbot_core::MessageRole::User => "user",
            ravenbot_core::MessageRole::Assistant => "assistant",
            ravenbot_core::MessageRole::System => "system",
            ravenbot_core::MessageRole::Tool => "tool",
        };

        if Self::has_message_sender_columns(pool).await {
            sqlx::query(
                "INSERT INTO messages (id, thread_id, role, content, attachments, sender_bot_id, sender_name, reply_to_id, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(message.id.to_string())
            .bind(message.thread_id.to_string())
            .bind(role)
            .bind(&content)
            .bind(&attachments)
            .bind(message.sender_bot_id.map(|id| id.to_string()))
            .bind(&message.sender_name)
            .bind(message.reply_to_id.map(|id| id.to_string()))
            .bind(message.created_at.to_rfc3339())
            .execute(pool)
            .await?;
        } else {
            sqlx::query(
                "INSERT INTO messages (id, thread_id, role, content, attachments, created_at)
                 VALUES (?, ?, ?, ?, ?, ?)"
            )
            .bind(message.id.to_string())
            .bind(message.thread_id.to_string())
            .bind(role)
            .bind(&content)
            .bind(&attachments)
            .bind(message.created_at.to_rfc3339())
            .execute(pool)
            .await?;
        }

        Ok(())
    }

    /// Delete a message by id (used by regenerate)
    pub async fn delete(pool: &SqlitePool, message_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM messages WHERE id = ?")
            .bind(message_id.to_string())
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Get messages for a thread
    pub async fn list_by_thread(pool: &SqlitePool, thread_id: Uuid) -> Result<Vec<ravenbot_core::Message>, sqlx::Error> {
        let rows: Vec<MessageRow> = sqlx::query_as(
            "SELECT * FROM messages WHERE thread_id = ? ORDER BY created_at ASC"
        )
        .bind(thread_id.to_string())
        .fetch_all(pool)
        .await?;

        let mut messages = Vec::new();
        for row in rows {
            let content: ravenbot_core::MessageContent = serde_json::from_str(&row.content)
                .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
            let attachments: Vec<ravenbot_core::Attachment> = serde_json::from_str(&row.attachments)
                .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
            let role = match row.role.as_str() {
                "user" => ravenbot_core::MessageRole::User,
                "assistant" => ravenbot_core::MessageRole::Assistant,
                "system" => ravenbot_core::MessageRole::System,
                "tool" => ravenbot_core::MessageRole::Tool,
                _ => ravenbot_core::MessageRole::User,
            };

            messages.push(ravenbot_core::Message {
                id: Uuid::parse_str(&row.id).unwrap_or_default(),
                thread_id: Uuid::parse_str(&row.thread_id).unwrap_or_default(),
                role,
                content,
                attachments,
                sender_bot_id: row.sender_bot_id.as_deref().and_then(|v| Uuid::parse_str(v).ok()),
                sender_name: row.sender_name.clone(),
                reply_to_id: row.reply_to_id.as_deref().and_then(|v| Uuid::parse_str(v).ok()),
                created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now()),
            });
        }

        Ok(messages)
    }
}

/// Run-related queries
pub struct RunQueries;

impl RunQueries {
    /// Insert a run
    pub async fn insert(pool: &SqlitePool, run: &ravenbot_core::Run) -> Result<(), sqlx::Error> {
        let state = match run.state {
            ravenbot_core::RunState::Planning => "planning",
            ravenbot_core::RunState::Acting => "acting",
            ravenbot_core::RunState::Observing => "observing",
            ravenbot_core::RunState::Reflecting => "reflecting",
            ravenbot_core::RunState::WaitingOnUser => "waiting_on_user",
            ravenbot_core::RunState::Paused => "paused",
            ravenbot_core::RunState::Completed => "completed",
            ravenbot_core::RunState::Failed => "failed",
            ravenbot_core::RunState::Cancelled => "cancelled",
        };

        let checkpoint = run.checkpoint.as_ref()
            .map(|c| serde_json::to_string(c).unwrap_or_default());
        let outcome = run.outcome.as_ref()
            .map(|o| serde_json::to_string(o).unwrap_or_default());

        sqlx::query(
            "INSERT INTO runs (id, bot_id, thread_id, parent_run_id, state, checkpoint, outcome, tokens_consumed, cost_estimate, created_at, updated_at, completed_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(run.id.to_string())
        .bind(run.bot_id.to_string())
        .bind(run.thread_id.to_string())
        .bind(run.parent_run_id.map(|id| id.to_string()))
        .bind(state)
        .bind(&checkpoint)
        .bind(&outcome)
        .bind(run.tokens_consumed as i64)
        .bind(run.cost_estimate)
        .bind(run.created_at.to_rfc3339())
        .bind(run.updated_at.to_rfc3339())
        .bind(run.completed_at.map(|dt| dt.to_rfc3339()))
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Get a run by ID
    pub async fn get(pool: &SqlitePool, id: Uuid) -> Result<Option<ravenbot_core::Run>, sqlx::Error> {
        let row: Option<RunRow> = sqlx::query_as("SELECT * FROM runs WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(pool)
            .await?;

        match row {
            Some(row) => {
                // Convert row to domain type
                let state = match row.state.as_str() {
                    "planning" => ravenbot_core::RunState::Planning,
                    "acting" => ravenbot_core::RunState::Acting,
                    "observing" => ravenbot_core::RunState::Observing,
                    "reflecting" => ravenbot_core::RunState::Reflecting,
                    "waiting_on_user" => ravenbot_core::RunState::WaitingOnUser,
                    "paused" => ravenbot_core::RunState::Paused,
                    "completed" => ravenbot_core::RunState::Completed,
                    "failed" => ravenbot_core::RunState::Failed,
                    "cancelled" => ravenbot_core::RunState::Cancelled,
                    _ => ravenbot_core::RunState::Planning,
                };

                Ok(Some(ravenbot_core::Run {
                    id: Uuid::parse_str(&row.id).unwrap_or_default(),
                    bot_id: Uuid::parse_str(&row.bot_id).unwrap_or_default(),
                    thread_id: Uuid::parse_str(&row.thread_id).unwrap_or_default(),
                    parent_run_id: row.parent_run_id.as_ref().and_then(|s| Uuid::parse_str(s).ok()),
                    state,
                    checkpoint: row.checkpoint.as_ref().and_then(|s| serde_json::from_str(s).ok()),
                    outcome: row.outcome.as_ref().and_then(|s| serde_json::from_str(s).ok()),
                    tokens_consumed: row.tokens_consumed as u64,
                    cost_estimate: row.cost_estimate,
                    created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now()),
                    updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now()),
                    completed_at: row.completed_at.as_ref().and_then(|s| {
                        chrono::DateTime::parse_from_rfc3339(s)
                            .map(|dt| dt.with_timezone(&chrono::Utc))
                            .ok()
                    }),
                }))
            }
            None => Ok(None),
        }
    }

    /// Update a run
    pub async fn update(pool: &SqlitePool, run: &ravenbot_core::Run) -> Result<(), sqlx::Error> {
        let state = match run.state {
            ravenbot_core::RunState::Planning => "planning",
            ravenbot_core::RunState::Acting => "acting",
            ravenbot_core::RunState::Observing => "observing",
            ravenbot_core::RunState::Reflecting => "reflecting",
            ravenbot_core::RunState::WaitingOnUser => "waiting_on_user",
            ravenbot_core::RunState::Paused => "paused",
            ravenbot_core::RunState::Completed => "completed",
            ravenbot_core::RunState::Failed => "failed",
            ravenbot_core::RunState::Cancelled => "cancelled",
        };

        let checkpoint = run.checkpoint.as_ref()
            .map(|c| serde_json::to_string(c).unwrap_or_default());
        let outcome = run.outcome.as_ref()
            .map(|o| serde_json::to_string(o).unwrap_or_default());

        sqlx::query(
            "UPDATE runs SET state = ?, checkpoint = ?, outcome = ?, tokens_consumed = ?, cost_estimate = ?, updated_at = ?, completed_at = ?
             WHERE id = ?"
        )
        .bind(state)
        .bind(&checkpoint)
        .bind(&outcome)
        .bind(run.tokens_consumed as i64)
        .bind(run.cost_estimate)
        .bind(run.updated_at.to_rfc3339())
        .bind(run.completed_at.map(|dt| dt.to_rfc3339()))
        .bind(run.id.to_string())
        .execute(pool)
        .await?;

        Ok(())
    }
}

/// ChatRoom-related queries
pub struct ChatRoomQueries;

impl ChatRoomQueries {
    pub async fn create(pool: &SqlitePool, room: &ravenbot_core::ChatRoom) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO chatrooms (id, name, description, office_template, avatar_url, avatar_style, goal, policy, terms, budget, budget_distribution, project_folders, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(room.id.to_string())
        .bind(&room.name)
        .bind(&room.description)
        .bind(&room.office_template)
        .bind(&room.avatar_url)
        .bind(&room.avatar_style)
        .bind(&room.goal)
        .bind(&room.policy)
        .bind(&room.terms)
        .bind(room.budget)
        .bind(room.budget_distribution.as_ref().map(|v| v.to_string()))
        .bind(serde_json::to_string(&room.project_folders).ok())
        .bind(room.created_at.to_rfc3339())
        .bind(room.updated_at.to_rfc3339())
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn update(pool: &SqlitePool, room: &ravenbot_core::ChatRoom) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE chatrooms SET name = ?, description = ?, office_template = ?, avatar_url = ?, avatar_style = ?, goal = ?, policy = ?, terms = ?, budget = ?, budget_distribution = ?, project_folders = ?, updated_at = ? WHERE id = ?"
        )
        .bind(&room.name)
        .bind(&room.description)
        .bind(&room.office_template)
        .bind(&room.avatar_url)
        .bind(&room.avatar_style)
        .bind(&room.goal)
        .bind(&room.policy)
        .bind(&room.terms)
        .bind(room.budget)
        .bind(room.budget_distribution.as_ref().map(|v| v.to_string()))
        .bind(serde_json::to_string(&room.project_folders).ok())
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(room.id.to_string())
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn list(pool: &SqlitePool) -> Result<Vec<ravenbot_core::ChatRoom>, sqlx::Error> {
        let rows: Vec<crate::models::ChatRoomRow> = sqlx::query_as("SELECT * FROM chatrooms ORDER BY updated_at DESC")
            .fetch_all(pool)
            .await?;
        Ok(rows.into_iter().filter_map(|r| {
            Some(ravenbot_core::ChatRoom {
                id: uuid::Uuid::parse_str(&r.id).ok()?,
                name: r.name,
                description: r.description,
                office_template: r.office_template,
                avatar_url: r.avatar_url,
                avatar_style: r.avatar_style,
                goal: r.goal,
                policy: r.policy,
                terms: r.terms,
                budget: r.budget,
                budget_distribution: r.budget_distribution.as_deref().and_then(|s| serde_json::from_str(s).ok()),
                project_folders: r.project_folders.as_deref().and_then(|s| serde_json::from_str(s).ok()).unwrap_or_default(),
                created_at: chrono::DateTime::parse_from_rfc3339(&r.created_at).map(|dt| dt.with_timezone(&chrono::Utc)).unwrap_or_else(|_| chrono::Utc::now()),
                updated_at: chrono::DateTime::parse_from_rfc3339(&r.updated_at).map(|dt| dt.with_timezone(&chrono::Utc)).unwrap_or_else(|_| chrono::Utc::now()),
            })
        }).collect())
    }

    pub async fn get(pool: &SqlitePool, id: Uuid) -> Result<Option<ravenbot_core::ChatRoom>, sqlx::Error> {
        let row: Option<crate::models::ChatRoomRow> = sqlx::query_as("SELECT * FROM chatrooms WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(pool)
            .await?;
        Ok(row.and_then(|r| {
            Some(ravenbot_core::ChatRoom {
                id: uuid::Uuid::parse_str(&r.id).ok()?,
                name: r.name,
                description: r.description,
                office_template: r.office_template,
                avatar_url: r.avatar_url,
                avatar_style: r.avatar_style,
                goal: r.goal,
                policy: r.policy,
                terms: r.terms,
                budget: r.budget,
                budget_distribution: r.budget_distribution.as_deref().and_then(|s| serde_json::from_str(s).ok()),
                project_folders: r.project_folders.as_deref().and_then(|s| serde_json::from_str(s).ok()).unwrap_or_default(),
                created_at: chrono::DateTime::parse_from_rfc3339(&r.created_at).map(|dt| dt.with_timezone(&chrono::Utc)).unwrap_or_else(|_| chrono::Utc::now()),
                updated_at: chrono::DateTime::parse_from_rfc3339(&r.updated_at).map(|dt| dt.with_timezone(&chrono::Utc)).unwrap_or_else(|_| chrono::Utc::now()),
            })
        }))
    }

    pub async fn add_member(pool: &SqlitePool, member: &ravenbot_core::ChatRoomMember) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT OR REPLACE INTO chatroom_members (chatroom_id, bot_id, rank, specialty, joined_at) VALUES (?, ?, ?, ?, ?)")
            .bind(member.chatroom_id.to_string())
            .bind(member.bot_id.to_string())
            .bind(&member.rank)
            .bind(&member.specialty)
            .bind(member.joined_at.to_rfc3339())
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn list_members(pool: &SqlitePool, chatroom_id: Uuid) -> Result<Vec<ravenbot_core::ChatRoomMember>, sqlx::Error> {
        let rows: Vec<crate::models::ChatRoomMemberRow> = sqlx::query_as("SELECT * FROM chatroom_members WHERE chatroom_id = ?")
            .bind(chatroom_id.to_string())
            .fetch_all(pool)
            .await?;
        Ok(rows.into_iter().filter_map(|r| {
            Some(ravenbot_core::ChatRoomMember {
                chatroom_id: uuid::Uuid::parse_str(&r.chatroom_id).ok()?,
                bot_id: uuid::Uuid::parse_str(&r.bot_id).ok()?,
                rank: r.rank,
                specialty: r.specialty,
                joined_at: chrono::DateTime::parse_from_rfc3339(&r.joined_at).map(|dt| dt.with_timezone(&chrono::Utc)).unwrap_or_else(|_| chrono::Utc::now()),
            })
        }).collect())
    }

    pub async fn remove_member(pool: &SqlitePool, chatroom_id: Uuid, bot_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM chatroom_members WHERE chatroom_id = ? AND bot_id = ?")
            .bind(chatroom_id.to_string()).bind(bot_id.to_string()).execute(pool).await?;
        Ok(())
    }

    pub async fn update_member(pool: &SqlitePool, chatroom_id: Uuid, bot_id: Uuid, rank: &str, specialty: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE chatroom_members SET rank = ?, specialty = ? WHERE chatroom_id = ? AND bot_id = ?")
            .bind(rank).bind(specialty).bind(chatroom_id.to_string()).bind(bot_id.to_string()).execute(pool).await?;
        Ok(())
    }

    pub async fn delete(pool: &SqlitePool, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM chatrooms WHERE id = ?").bind(id.to_string()).execute(pool).await?;
        Ok(())
    }
}

impl ThreadQueries {
    /// Rename a thread
    pub async fn rename(pool: &SqlitePool, thread_id: Uuid, title: &str) -> Result<(), sqlx::Error> {
        let title = title.trim();
        if title.is_empty() {
            return Err(sqlx::Error::Decode("Title cannot be empty".to_string().into()));
        }
        sqlx::query("UPDATE threads SET title = ?, updated_at = ? WHERE id = ?")
            .bind(title)
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(thread_id.to_string())
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Delete a thread (messages cascade via FK)
    pub async fn delete(pool: &SqlitePool, thread_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM threads WHERE id = ?")
            .bind(thread_id.to_string())
            .execute(pool)
            .await?;
        Ok(())
    }
}

/// Cross-thread message search (simple LIKE scan; fine for local volumes)
pub struct SearchQueries;

impl SearchQueries {
    pub async fn messages(
        pool: &SqlitePool,
        query: &str,
        limit: u32,
    ) -> Result<Vec<(ravenbot_core::Message, String)>, sqlx::Error> {
        let pattern = format!("%{}%", query.replace('%', r"\%").replace('_', r"\_"));
        let rows: Vec<(String, String, String, String, String, String)> = sqlx::query_as(
            r#"SELECT m.id, m.thread_id, m.role, m.content, m.created_at,
                      t.title
               FROM messages m
               JOIN threads t ON t.id = m.thread_id
               WHERE m.content LIKE ? ESCAPE '\'
               ORDER BY m.created_at DESC
               LIMIT ?"#,
        )
        .bind(&pattern)
        .bind(limit)
        .fetch_all(pool)
        .await?;

        let mut results = Vec::new();
        for row in rows {
            let content: ravenbot_core::MessageContent = serde_json::from_str(&row.3)
                .unwrap_or(ravenbot_core::MessageContent::Text {
                    text: String::new(),
                    sources: Vec::new(),
                });
            let role = match row.2.as_str() {
                "user" => ravenbot_core::MessageRole::User,
                "assistant" => ravenbot_core::MessageRole::Assistant,
                "system" => ravenbot_core::MessageRole::System,
                _ => ravenbot_core::MessageRole::Tool,
            };
            let message = ravenbot_core::Message {
                id: Uuid::parse_str(&row.0).unwrap_or_default(),
                thread_id: Uuid::parse_str(&row.1).unwrap_or_default(),
                role,
                content,
                attachments: Vec::new(),
                sender_bot_id: None,
                sender_name: None,
                reply_to_id: None,
                created_at: chrono::DateTime::parse_from_rfc3339(&row.4)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now()),
            };
            results.push((message, row.5.clone()));
        }
        Ok(results)
    }
}

/// Approval-request queries (the "bots ask before they act" gate)
pub struct ApprovalQueries;

impl ApprovalQueries {
    fn row_to_domain(
        id: String,
        bot_id: String,
        thread_id: String,
        run_id: String,
        tool_name: String,
        tool_label: String,
        arguments: String,
        risk: String,
        status: String,
        note: Option<String>,
        created_at: String,
        decided_at: Option<String>,
    ) -> ravenbot_core::ApprovalRequest {
        let parse_dt = |s: &str| {
            chrono::DateTime::parse_from_rfc3339(s)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now())
        };
        ravenbot_core::ApprovalRequest {
            id: Uuid::parse_str(&id).unwrap_or_default(),
            bot_id: Uuid::parse_str(&bot_id).unwrap_or_default(),
            thread_id: Uuid::parse_str(&thread_id).unwrap_or_default(),
            run_id: Uuid::parse_str(&run_id).unwrap_or_default(),
            tool_name,
            tool_label,
            arguments: serde_json::from_str(&arguments).unwrap_or(serde_json::Value::Null),
            risk,
            status: match status.as_str() {
                "allowed" => ravenbot_core::ApprovalStatus::Allowed,
                "denied" => ravenbot_core::ApprovalStatus::Denied,
                "expired" => ravenbot_core::ApprovalStatus::Expired,
                _ => ravenbot_core::ApprovalStatus::Pending,
            },
            note,
            created_at: parse_dt(&created_at),
            decided_at: decided_at.as_deref().map(parse_dt),
        }
    }

    /// Insert a new pending request
    pub async fn create(pool: &SqlitePool, req: &ravenbot_core::ApprovalRequest) -> Result<(), sqlx::Error> {
        let status = match req.status {
            ravenbot_core::ApprovalStatus::Pending => "pending",
            ravenbot_core::ApprovalStatus::Allowed => "allowed",
            ravenbot_core::ApprovalStatus::Denied => "denied",
            ravenbot_core::ApprovalStatus::Expired => "expired",
        };
        sqlx::query(
            "INSERT INTO approvals (id, bot_id, thread_id, run_id, tool_name, tool_label, arguments, risk, status, note, created_at, decided_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(req.id.to_string())
        .bind(req.bot_id.to_string())
        .bind(req.thread_id.to_string())
        .bind(req.run_id.to_string())
        .bind(&req.tool_name)
        .bind(&req.tool_label)
        .bind(serde_json::to_string(&req.arguments).unwrap_or_default())
        .bind(&req.risk)
        .bind(status)
        .bind(&req.note)
        .bind(req.created_at.to_rfc3339())
        .bind(req.decided_at.map(|dt| dt.to_rfc3339()))
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Get one request by id
    pub async fn get(pool: &SqlitePool, id: Uuid) -> Result<Option<ravenbot_core::ApprovalRequest>, sqlx::Error> {
        type Row = (String, String, String, String, String, String, String, String, String, Option<String>, String, Option<String>);
        let row: Option<Row> = sqlx::query_as(
            "SELECT id, bot_id, thread_id, run_id, tool_name, tool_label, arguments, risk, status, note, created_at, decided_at
             FROM approvals WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|r| Self::row_to_domain(r.0, r.1, r.2, r.3, r.4, r.5, r.6, r.7, r.8, r.9, r.10, r.11)))
    }

    /// Pending requests for a thread (composer block + inline cards)
    pub async fn list_pending_for_thread(
        pool: &SqlitePool,
        thread_id: Uuid,
    ) -> Result<Vec<ravenbot_core::ApprovalRequest>, sqlx::Error> {
        type Row = (String, String, String, String, String, String, String, String, String, Option<String>, String, Option<String>);
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT id, bot_id, thread_id, run_id, tool_name, tool_label, arguments, risk, status, note, created_at, decided_at
             FROM approvals WHERE thread_id = ? AND status = 'pending' ORDER BY created_at ASC",
        )
        .bind(thread_id.to_string())
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| Self::row_to_domain(r.0, r.1, r.2, r.3, r.4, r.5, r.6, r.7, r.8, r.9, r.10, r.11))
            .collect())
    }

    /// Decide a request (allow/deny). Returns false if it wasn't pending.
    pub async fn decide(
        pool: &SqlitePool,
        id: Uuid,
        allowed: bool,
        note: Option<&str>,
    ) -> Result<bool, sqlx::Error> {
        let status = if allowed { "allowed" } else { "denied" };
        let res = sqlx::query(
            "UPDATE approvals SET status = ?, note = COALESCE(?, note), decided_at = ?
             WHERE id = ? AND status = 'pending'",
        )
        .bind(status)
        .bind(note)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(id.to_string())
        .execute(pool)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Expire all pending requests for a run (run finished without them)
    pub async fn expire_for_run(pool: &SqlitePool, run_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE approvals SET status = 'expired' WHERE run_id = ? AND status = 'pending'")
            .bind(run_id.to_string())
            .execute(pool)
            .await?;
        Ok(())
    }
}

/// Question queries (the `ask_user` human-in-the-loop gate)
pub struct QuestionQueries;

impl QuestionQueries {
    #[allow(clippy::too_many_arguments)]
    fn row_to_domain(
        id: String,
        bot_id: String,
        thread_id: String,
        run_id: String,
        header: String,
        question: String,
        options: String,
        allow_custom: i64,
        status: String,
        answer: Option<String>,
        created_at: String,
        answered_at: Option<String>,
    ) -> ravenbot_core::QuestionRequest {
        let parse_dt = |s: &str| {
            chrono::DateTime::parse_from_rfc3339(s)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now())
        };
        ravenbot_core::QuestionRequest {
            id: Uuid::parse_str(&id).unwrap_or_default(),
            bot_id: Uuid::parse_str(&bot_id).unwrap_or_default(),
            thread_id: Uuid::parse_str(&thread_id).unwrap_or_default(),
            run_id: Uuid::parse_str(&run_id).unwrap_or_default(),
            header,
            question,
            options: serde_json::from_str(&options).unwrap_or_default(),
            allow_custom: allow_custom != 0,
            status: match status.as_str() {
                "answered" => ravenbot_core::QuestionStatus::Answered,
                "expired" => ravenbot_core::QuestionStatus::Expired,
                _ => ravenbot_core::QuestionStatus::Pending,
            },
            answer,
            created_at: parse_dt(&created_at),
            answered_at: answered_at.as_deref().map(parse_dt),
        }
    }

    pub async fn create(
        pool: &SqlitePool,
        req: &ravenbot_core::QuestionRequest,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO questions (id, bot_id, thread_id, run_id, header, question, options, allow_custom, status, answer, created_at, answered_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(req.id.to_string())
        .bind(req.bot_id.to_string())
        .bind(req.thread_id.to_string())
        .bind(req.run_id.to_string())
        .bind(&req.header)
        .bind(&req.question)
        .bind(serde_json::to_string(&req.options).unwrap_or_else(|_| "[]".into()))
        .bind(if req.allow_custom { 1 } else { 0 })
        .bind("pending")
        .bind(&req.answer)
        .bind(req.created_at.to_rfc3339())
        .bind(req.answered_at.map(|dt| dt.to_rfc3339()))
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn get(
        pool: &SqlitePool,
        id: Uuid,
    ) -> Result<Option<ravenbot_core::QuestionRequest>, sqlx::Error> {
        type Row = (
            String, String, String, String, String, String, String, i64, String,
            Option<String>, String, Option<String>,
        );
        let row: Option<Row> = sqlx::query_as(
            "SELECT id, bot_id, thread_id, run_id, header, question, options, allow_custom, status, answer, created_at, answered_at
             FROM questions WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|r| {
            Self::row_to_domain(r.0, r.1, r.2, r.3, r.4, r.5, r.6, r.7, r.8, r.9, r.10, r.11)
        }))
    }

    pub async fn list_pending_for_thread(
        pool: &SqlitePool,
        thread_id: Uuid,
    ) -> Result<Vec<ravenbot_core::QuestionRequest>, sqlx::Error> {
        type Row = (
            String, String, String, String, String, String, String, i64, String,
            Option<String>, String, Option<String>,
        );
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT id, bot_id, thread_id, run_id, header, question, options, allow_custom, status, answer, created_at, answered_at
             FROM questions WHERE thread_id = ? AND status = 'pending' ORDER BY created_at ASC",
        )
        .bind(thread_id.to_string())
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                Self::row_to_domain(r.0, r.1, r.2, r.3, r.4, r.5, r.6, r.7, r.8, r.9, r.10, r.11)
            })
            .collect())
    }

    /// Record the user's answer. Returns false if it wasn't pending.
    pub async fn answer(
        pool: &SqlitePool,
        id: Uuid,
        answer: &str,
    ) -> Result<bool, sqlx::Error> {
        let res = sqlx::query(
            "UPDATE questions SET status = 'answered', answer = ?, answered_at = ?
             WHERE id = ? AND status = 'pending'",
        )
        .bind(answer)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(id.to_string())
        .execute(pool)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Expire a single pending question.
    pub async fn expire(pool: &SqlitePool, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE questions SET status = 'expired' WHERE id = ? AND status = 'pending'")
            .bind(id.to_string())
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Expire all pending questions for a run.
    pub async fn expire_for_run(pool: &SqlitePool, run_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE questions SET status = 'expired' WHERE run_id = ? AND status = 'pending'")
            .bind(run_id.to_string())
            .execute(pool)
            .await?;
        Ok(())
    }
}

/// Provider API keys persistence (migration 009)
pub struct ProviderKeyQueries;

impl ProviderKeyQueries {
    /// List all configured provider API keys
    pub async fn list(pool: &SqlitePool) -> Result<std::collections::HashMap<String, String>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String, String)>(
            "SELECT provider, api_key FROM provider_keys WHERE api_key != ''"
        )
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().collect())
    }

    /// Get API key for a specific provider
    pub async fn get(pool: &SqlitePool, provider: &str) -> Result<Option<String>, sqlx::Error> {
        let row = sqlx::query_as::<_, (String,)>(
            "SELECT api_key FROM provider_keys WHERE provider = ?"
        )
        .bind(provider)
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|r| r.0))
    }

    /// Set (upsert) an API key for a provider, or delete if empty
    pub async fn set(pool: &SqlitePool, provider: &str, api_key: &str) -> Result<(), sqlx::Error> {
        let trimmed = api_key.trim();
        if trimmed.is_empty() {
            sqlx::query("DELETE FROM provider_keys WHERE provider = ?")
                .bind(provider)
                .execute(pool)
                .await?;
        } else {
            sqlx::query(
                "INSERT INTO provider_keys (provider, api_key, updated_at) VALUES (?, ?, datetime('now'))
                 ON CONFLICT(provider) DO UPDATE SET api_key = excluded.api_key, updated_at = excluded.updated_at"
            )
            .bind(provider)
            .bind(trimmed)
            .execute(pool)
            .await?;
        }
        Ok(())
    }

    /// Delete an API key for a provider
    pub async fn delete(pool: &SqlitePool, provider: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM provider_keys WHERE provider = ?")
            .bind(provider)
            .execute(pool)
            .await?;
        Ok(())
    }
}

/// User-defined model providers (migration 018).
/// Rows map to (id, display_name, kind, base_url, default_model, supports_tools, enabled).
pub struct CustomProviderQueries;

impl CustomProviderQueries {
    pub async fn list(pool: &SqlitePool) -> Result<Vec<(String, String, String, String, String, bool, bool)>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String, String, String, String, String, i64, i64)>(
            "SELECT id, display_name, kind, base_url, default_model, supports_tools, enabled FROM custom_providers ORDER BY display_name"
        )
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(|(id, dn, kind, base, dm, tools, en)| (id, dn, kind, base, dm, tools != 0, en != 0)).collect())
    }

    pub async fn get(pool: &SqlitePool, id: &str) -> Result<Option<(String, String, String, String, String, bool, bool)>, sqlx::Error> {
        let row = sqlx::query_as::<_, (String, String, String, String, String, i64, i64)>(
            "SELECT id, display_name, kind, base_url, default_model, supports_tools, enabled FROM custom_providers WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|(id, dn, kind, base, dm, tools, en)| (id, dn, kind, base, dm, tools != 0, en != 0)))
    }

    pub async fn upsert(
        pool: &SqlitePool,
        id: &str,
        display_name: &str,
        kind: &str,
        base_url: &str,
        default_model: &str,
        supports_tools: bool,
        enabled: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO custom_providers (id, display_name, kind, base_url, default_model, supports_tools, enabled, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))
             ON CONFLICT(id) DO UPDATE SET display_name = excluded.display_name, kind = excluded.kind,
               base_url = excluded.base_url, default_model = excluded.default_model,
               supports_tools = excluded.supports_tools, enabled = excluded.enabled, updated_at = excluded.updated_at"
        )
        .bind(id)
        .bind(display_name)
        .bind(kind)
        .bind(base_url)
        .bind(default_model)
        .bind(if supports_tools { 1i64 } else { 0i64 })
        .bind(if enabled { 1i64 } else { 0i64 })
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn delete(pool: &SqlitePool, id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM custom_providers WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }
}

/// Per-provider base URL overrides for built-in providers (migration 018).
/// A row's presence is the opt-in; absence means the compiled-in default.
pub struct ProviderBaseUrlQueries;

impl ProviderBaseUrlQueries {
    pub async fn list(pool: &SqlitePool) -> Result<Vec<(String, String)>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String, String)>(
            "SELECT provider, base_url FROM provider_base_urls"
        )
        .fetch_all(pool)
        .await?;
        Ok(rows)
    }

    pub async fn get(pool: &SqlitePool, provider: &str) -> Result<Option<String>, sqlx::Error> {
        let row = sqlx::query_as::<_, (String,)>(
            "SELECT base_url FROM provider_base_urls WHERE provider = ?"
        )
        .bind(provider)
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|r| r.0))
    }

    /// Set an override, or clear it when `base_url` is empty
    pub async fn set(pool: &SqlitePool, provider: &str, base_url: &str) -> Result<(), sqlx::Error> {
        let trimmed = base_url.trim().trim_end_matches('/');
        if trimmed.is_empty() {
            sqlx::query("DELETE FROM provider_base_urls WHERE provider = ?")
                .bind(provider)
                .execute(pool)
                .await?;
        } else {
            sqlx::query(
                "INSERT INTO provider_base_urls (provider, base_url, updated_at) VALUES (?, ?, datetime('now'))
                 ON CONFLICT(provider) DO UPDATE SET base_url = excluded.base_url, updated_at = excluded.updated_at"
            )
            .bind(provider)
            .bind(trimmed)
            .execute(pool)
            .await?;
        }
        Ok(())
    }
}

/// App settings persistence (migration 009)
pub struct AppSettingsQueries;

impl AppSettingsQueries {
    /// Get an app setting value
    pub async fn get(pool: &SqlitePool, key: &str) -> Result<Option<String>, sqlx::Error> {
        let row = sqlx::query_as::<_, (String,)>(
            "SELECT value FROM app_settings WHERE key = ?"
        )
        .bind(key)
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|r| r.0))
    }

    /// Set (upsert) an app setting value
    pub async fn set(pool: &SqlitePool, key: &str, value: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO app_settings (key, value, updated_at) VALUES (?, ?, datetime('now'))
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at"
        )
        .bind(key)
        .bind(value)
        .execute(pool)
        .await?;
        Ok(())
    }
}


/// Channel (context) queries — Work/Personal/project scoping
pub struct ChannelQueries;

/// Column tuple for a `channels` row.
type ChannelRow = (
    String, String, String, String, Option<String>, Option<String>, i64, Option<String>, String, String,
);

impl ChannelQueries {
    fn row_to_domain(r: ChannelRow) -> ravenbot_core::Channel {
        let parse_dt = |s: &str| {
            chrono::DateTime::parse_from_rfc3339(s)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now())
        };
        ravenbot_core::Channel {
            id: Uuid::parse_str(&r.0).unwrap_or_default(),
            name: r.1,
            description: r.2,
            instructions: r.3,
            working_folder: r.4,
            color: r.5,
            position: r.6,
            responder_rules: r.7.as_deref().and_then(|j| serde_json::from_str(j).ok()),
            created_at: parse_dt(&r.8),
            updated_at: parse_dt(&r.9),
        }
    }

    const COLS: &'static str =
        "id, name, description, instructions, working_folder, color, position, responder_rules, created_at, updated_at";

    pub async fn list(pool: &SqlitePool) -> Result<Vec<ravenbot_core::Channel>, sqlx::Error> {
        let rows: Vec<ChannelRow> = sqlx::query_as(&format!(
            "SELECT {} FROM channels ORDER BY position ASC, name ASC",
            Self::COLS
        ))
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(Self::row_to_domain).collect())
    }

    pub async fn get(
        pool: &SqlitePool,
        id: Uuid,
    ) -> Result<Option<ravenbot_core::Channel>, sqlx::Error> {
        let row: Option<ChannelRow> = sqlx::query_as(&format!(
            "SELECT {} FROM channels WHERE id = ?",
            Self::COLS
        ))
        .bind(id.to_string())
        .fetch_optional(pool)
        .await?;
        Ok(row.map(Self::row_to_domain))
    }

    pub async fn create(pool: &SqlitePool, c: &ravenbot_core::Channel) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO channels (id, name, description, instructions, working_folder, color, position, responder_rules, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(c.id.to_string())
        .bind(&c.name)
        .bind(&c.description)
        .bind(&c.instructions)
        .bind(&c.working_folder)
        .bind(&c.color)
        .bind(c.position)
        .bind(c.responder_rules.as_ref().map(|v| v.to_string()))
        .bind(c.created_at.to_rfc3339())
        .bind(c.updated_at.to_rfc3339())
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn update(pool: &SqlitePool, c: &ravenbot_core::Channel) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE channels SET name = ?, description = ?, instructions = ?, working_folder = ?, color = ?, position = ?, responder_rules = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(&c.name)
        .bind(&c.description)
        .bind(&c.instructions)
        .bind(&c.working_folder)
        .bind(&c.color)
        .bind(c.position)
        .bind(c.responder_rules.as_ref().map(|v| v.to_string()))
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(c.id.to_string())
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn delete(pool: &SqlitePool, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM channels WHERE id = ?")
            .bind(id.to_string())
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Bot ids assigned to a channel.
    pub async fn bots(pool: &SqlitePool, channel_id: Uuid) -> Result<Vec<Uuid>, sqlx::Error> {
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT bot_id FROM channel_bots WHERE channel_id = ?")
                .bind(channel_id.to_string())
                .fetch_all(pool)
                .await?;
        Ok(rows
            .into_iter()
            .filter_map(|(s,)| Uuid::parse_str(&s).ok())
            .collect())
    }

    /// Replace a channel's bot roster.
    pub async fn set_bots(
        pool: &SqlitePool,
        channel_id: Uuid,
        bot_ids: &[Uuid],
    ) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM channel_bots WHERE channel_id = ?")
            .bind(channel_id.to_string())
            .execute(pool)
            .await?;
        for bot_id in bot_ids {
            sqlx::query("INSERT OR IGNORE INTO channel_bots (channel_id, bot_id) VALUES (?, ?)")
                .bind(channel_id.to_string())
                .bind(bot_id.to_string())
                .execute(pool)
                .await?;
        }
        Ok(())
    }
}

/// Webhook trigger queries on routines.
pub struct WebhookQueries;

impl WebhookQueries {
    /// Set (or rotate) a routine's webhook secret and enable/disable it.
    pub async fn set(
        pool: &SqlitePool,
        routine_id: Uuid,
        secret: Option<&str>,
        enabled: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE routines SET webhook_secret = ?, webhook_enabled = ?, updated_at = ? WHERE id = ?",
        )
        .bind(secret)
        .bind(if enabled { 1 } else { 0 })
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(routine_id.to_string())
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn secret(
        pool: &SqlitePool,
        routine_id: Uuid,
    ) -> Result<Option<String>, sqlx::Error> {
        let row: Option<(Option<String>, i64)> = sqlx::query_as(
            "SELECT webhook_secret, webhook_enabled FROM routines WHERE id = ?",
        )
        .bind(routine_id.to_string())
        .fetch_optional(pool)
        .await?;
        Ok(row.and_then(|(s, enabled)| if enabled != 0 { s } else { None }))
    }

    /// Look up the routine id owning a webhook secret (for routing an inbound
    /// call). Returns the routine only when enabled.
    pub async fn routine_for_secret(
        pool: &SqlitePool,
        secret: &str,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM routines WHERE webhook_secret = ? AND webhook_enabled = 1",
        )
        .bind(secret)
        .fetch_optional(pool)
        .await?;
        Ok(row.and_then(|(s,)| Uuid::parse_str(&s).ok()))
    }
}

/// Per-bot contact state for the "bots as contacts" UX: pinned, hidden, and
/// last-read timestamp (drives unread badges). Kept in its own table so
/// frequent read-marking never rewrites the bot's config JSON.
pub struct BotContactQueries;

impl BotContactQueries {
    /// Ensure every existing bot has a contact row. New rows start read
    /// (last_read_at = now) so a fresh fleet shows zero unread.
    pub async fn ensure_rows(pool: &SqlitePool) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT OR IGNORE INTO bot_contacts (bot_id, pinned, hidden, last_read_at)
             SELECT id, 0, 0, ? FROM bots",
        )
        .bind(&now)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// (bot_id, pinned, hidden, last_read_at)
    pub async fn list(
        pool: &SqlitePool,
    ) -> Result<Vec<(Uuid, bool, bool, Option<String>)>, sqlx::Error> {
        let rows: Vec<(String, i64, i64, Option<String>)> =
            sqlx::query_as("SELECT bot_id, pinned, hidden, last_read_at FROM bot_contacts")
                .fetch_all(pool)
                .await?;
        Ok(rows
            .into_iter()
            .filter_map(|(id, pinned, hidden, read)| {
                Some((Uuid::parse_str(&id).ok()?, pinned != 0, hidden != 0, read))
            })
            .collect())
    }

    pub async fn set_pinned(
        pool: &SqlitePool,
        bot_id: Uuid,
        pinned: bool,
    ) -> Result<(), sqlx::Error> {
        Self::ensure_rows(pool).await?;
        sqlx::query("UPDATE bot_contacts SET pinned = ? WHERE bot_id = ?")
            .bind(if pinned { 1 } else { 0 })
            .bind(bot_id.to_string())
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn set_hidden(
        pool: &SqlitePool,
        bot_id: Uuid,
        hidden: bool,
    ) -> Result<(), sqlx::Error> {
        Self::ensure_rows(pool).await?;
        sqlx::query("UPDATE bot_contacts SET hidden = ? WHERE bot_id = ?")
            .bind(if hidden { 1 } else { 0 })
            .bind(bot_id.to_string())
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Mark a bot's threads as read up to now.
    pub async fn mark_read(pool: &SqlitePool, bot_id: Uuid) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO bot_contacts (bot_id, pinned, hidden, last_read_at)
             VALUES (?, 0, 0, ?)
             ON CONFLICT(bot_id) DO UPDATE SET last_read_at = excluded.last_read_at",
        )
        .bind(bot_id.to_string())
        .bind(&now)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Unread assistant-message count per bot (messages after last_read_at).
    pub async fn unread_counts(pool: &SqlitePool) -> Result<Vec<(Uuid, i64)>, sqlx::Error> {
        let rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT t.bot_id, COUNT(*) AS unread
             FROM messages m
             JOIN threads t ON t.id = m.thread_id
             LEFT JOIN bot_contacts c ON c.bot_id = t.bot_id
             WHERE m.role = 'assistant'
               AND (c.last_read_at IS NULL OR m.created_at > c.last_read_at)
             GROUP BY t.bot_id",
        )
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|(id, count)| Some((Uuid::parse_str(&id).ok()?, count)))
            .collect())
    }
}
