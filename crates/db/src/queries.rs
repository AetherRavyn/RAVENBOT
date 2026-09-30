//! Database query functions

use sqlx::SqlitePool;
use uuid::Uuid;

use crate::models::{BotRow, ThreadRow, MessageRow, RunRow};

/// Bot-related queries
pub struct BotQueries;

impl BotQueries {
    /// Insert a new bot
    pub async fn insert(pool: &SqlitePool, bot: &ravenbot_core::Bot) -> Result<(), sqlx::Error> {
        let row = BotRow::from_domain(bot).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        
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

        Ok(())
    }

    /// Get a bot by ID
    pub async fn get(pool: &SqlitePool, id: Uuid) -> Result<Option<ravenbot_core::Bot>, sqlx::Error> {
        let row: Option<BotRow> = sqlx::query_as("SELECT * FROM bots WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(pool)
            .await?;

        match row {
            Some(row) => Ok(Some(row.to_domain().map_err(|e| sqlx::Error::Decode(Box::new(e)))?)),
            None => Ok(None),
        }
    }

    /// Get all bots
    pub async fn list(pool: &SqlitePool) -> Result<Vec<ravenbot_core::Bot>, sqlx::Error> {
        let rows: Vec<BotRow> = sqlx::query_as("SELECT * FROM bots ORDER BY updated_at DESC")
            .fetch_all(pool)
            .await?;

        let mut bots = Vec::new();
        for row in rows {
            bots.push(row.to_domain().map_err(|e| sqlx::Error::Decode(Box::new(e)))?);
        }
        Ok(bots)
    }

    /// Update a bot
    pub async fn update(pool: &SqlitePool, bot: &ravenbot_core::Bot) -> Result<(), sqlx::Error> {
        let row = BotRow::from_domain(bot).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        
        sqlx::query(
            "UPDATE bots SET name = ?, description = ?, avatar_color = ?, avatar_url = ?, avatar_style = ?, rank = ?, specialty = ?, status = ?, config = ?, permissions = ?, is_orchestrator = ?, delegate_to = ?, updated_at = ?, last_active_at = ?
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
            "INSERT INTO threads (id, bot_id, title, is_active, ephemeral, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(thread.id.to_string())
        .bind(thread.bot_id.to_string())
        .bind(&thread.title)
        .bind(thread.is_active)
        .bind(thread.ephemeral)
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
            "INSERT INTO chatrooms (id, name, description, office_template, avatar_url, avatar_style, goal, policy, terms, budget, budget_distribution, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
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
        .bind(room.created_at.to_rfc3339())
        .bind(room.updated_at.to_rfc3339())
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn update(pool: &SqlitePool, room: &ravenbot_core::ChatRoom) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE chatrooms SET name = ?, description = ?, office_template = ?, avatar_url = ?, avatar_style = ?, goal = ?, policy = ?, terms = ?, budget = ?, budget_distribution = ?, updated_at = ? WHERE id = ?"
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
                created_at: chrono::DateTime::parse_from_rfc3339(&row.4)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now()),
            };
            results.push((message, row.5.clone()));
        }
        Ok(results)
    }
}
