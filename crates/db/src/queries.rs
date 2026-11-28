//! Database query functions

use chrono::Utc;
use serde::Serialize;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::models::{BotRow, MessageRow, RunRow, ThreadRow};

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
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM pragma_table_info('bots') WHERE name = ?",
        )
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap_or(0)
            > 0
    }

    /// Overlay junction-table skills onto bots loaded without the column
    /// (pre-009 databases).
    async fn backfill_skills_from_junction(pool: &SqlitePool, bots: &mut [ravenbot_core::Bot]) {
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
            let _ =
                sqlx::query("INSERT OR IGNORE INTO bot_skills (bot_id, skill_id) VALUES (?, ?)")
                    .bind(bot.id.to_string())
                    .bind(skill_id)
                    .execute(pool)
                    .await;
        }
    }

    /// Insert a new bot
    pub async fn insert(pool: &SqlitePool, bot: &ravenbot_core::Bot) -> Result<(), sqlx::Error> {
        let row = BotRow::from_domain(bot).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        let approval = row
            .approval_mode
            .clone()
            .unwrap_or_else(|| "ask".to_string());

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
    pub async fn get(
        pool: &SqlitePool,
        id: Uuid,
    ) -> Result<Option<ravenbot_core::Bot>, sqlx::Error> {
        // `SELECT *` maps onto BotRow.skills as Option — missing columns on
        // old DBs decode as None, so this is safe pre- and post-009.
        let row: Option<BotRow> = sqlx::query_as("SELECT * FROM bots WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(pool)
            .await?;

        match row {
            Some(row) => {
                let mut bot = row
                    .to_domain()
                    .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
                if row.skills.is_none() {
                    Self::backfill_skills_from_junction(pool, std::slice::from_mut(&mut bot)).await;
                }
                Ok(Some(bot))
            }
            None => Ok(None),
        }
    }

    /// Get all bots. Manual drag-order wins; never-dragged bots keep newest-first.
    pub async fn list(pool: &SqlitePool) -> Result<Vec<ravenbot_core::Bot>, sqlx::Error> {
        let rows: Vec<BotRow> =
            sqlx::query_as("SELECT * FROM bots ORDER BY sort_order ASC, updated_at DESC")
                .fetch_all(pool)
                .await?;

        let needs_backfill = rows.iter().any(|r| r.skills.is_none());
        let mut bots = Vec::new();
        for row in rows {
            bots.push(
                row.to_domain()
                    .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
            );
        }
        if needs_backfill {
            Self::backfill_skills_from_junction(pool, &mut bots).await;
        }
        Ok(bots)
    }

    /// Persist manual ordering (sidebar drag-reorder): each id gets sort_order = its position.
    pub async fn reorder(pool: &SqlitePool, ordered_ids: &[Uuid]) -> Result<(), sqlx::Error> {
        let mut tx = pool.begin().await?;
        for (i, id) in ordered_ids.iter().enumerate() {
            sqlx::query("UPDATE bots SET sort_order = ? WHERE id = ?")
                .bind(i as i64)
                .bind(id.to_string())
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await
    }

    /// Update a bot
    pub async fn update(pool: &SqlitePool, bot: &ravenbot_core::Bot) -> Result<(), sqlx::Error> {
        let row = BotRow::from_domain(bot).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        let approval = row
            .approval_mode
            .clone()
            .unwrap_or_else(|| "ask".to_string());

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
        // Threads and messages cascade from the bot, but reactions have no
        // foreign key (see migration 029), so they are cleared first.
        let mut tx = pool.begin().await?;
        sqlx::query(
            "DELETE FROM message_reactions
             WHERE message_id IN (
                 SELECT m.id FROM messages m
                 JOIN threads t ON t.id = m.thread_id
                 WHERE t.bot_id = ?
             )",
        )
        .bind(id.to_string())
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM bots WHERE id = ?")
            .bind(id.to_string())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Record that a bot is doing something, and what.
    ///
    /// These two columns were only ever written by the whole-row `insert` and
    /// `update` above, which nothing calls mid-run — so `status` stayed
    /// `idle` and `last_active_at` stayed `NULL` for the life of every bot.
    /// The UI worked around that by deriving live state from the event stream
    /// instead, which is right while the window is open and useless after a
    /// restart or in a second window.
    ///
    /// `last_active_at` moves with every call because "when did this agent
    /// last do anything" is the question a user actually asks about an agent
    /// that has gone quiet.
    pub async fn mark_active(
        pool: &SqlitePool,
        id: Uuid,
        status: ravenbot_core::BotStatus,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE bots SET status = ?, last_active_at = ?, updated_at = ? WHERE id = ?")
            .bind(crate::models::bot_status_to_db(status))
            .bind(Utc::now().to_rfc3339())
            .bind(Utc::now().to_rfc3339())
            .bind(id.to_string())
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Record activity without changing the status.
    ///
    /// A status write can be superseded by a newer status before its detached
    /// writer runs. The newer write owns the status, but the superseded event
    /// still proves the agent was active, so `last_active_at` must not be lost.
    pub async fn touch_active(pool: &SqlitePool, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE bots SET last_active_at = ?, updated_at = ? WHERE id = ?")
            .bind(Utc::now().to_rfc3339())
            .bind(Utc::now().to_rfc3339())
            .bind(id.to_string())
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Move every bot in a set back to idle.
    ///
    /// Used at startup after the orphaned-run reconciliation, so a crash does
    /// not leave the fleet permanently shown as busy.
    pub async fn mark_all_idle(pool: &SqlitePool) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("UPDATE bots SET status = 'idle' WHERE status != 'idle'")
            .execute(pool)
            .await?;
        Ok(result.rows_affected())
    }
}

/// Thread-related queries
pub struct ThreadQueries;

impl ThreadQueries {
    /// Create a new thread
    pub async fn create(
        pool: &SqlitePool,
        thread: &ravenbot_core::Thread,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO threads (id, bot_id, title, is_active, ephemeral, channel_id, kind, visible, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(thread.id.to_string())
        .bind(thread.bot_id.to_string())
        .bind(&thread.title)
        .bind(thread.is_active)
        .bind(thread.ephemeral)
        .bind(thread.channel_id.map(|u| u.to_string()))
        .bind(thread.kind.as_str())
        .bind(thread.visible)
        .bind(thread.created_at.to_rfc3339())
        .bind(thread.updated_at.to_rfc3339())
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Get a thread by ID
    pub async fn get(
        pool: &SqlitePool,
        id: Uuid,
    ) -> Result<Option<ravenbot_core::Thread>, sqlx::Error> {
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
                    channel_id: row
                        .channel_id
                        .as_deref()
                        .and_then(|s| Uuid::parse_str(s).ok()),
                    kind: row
                        .kind
                        .as_deref()
                        .map(ravenbot_core::ThreadKind::from_db)
                        .unwrap_or_default(),
                    visible: row.visible.unwrap_or(1) != 0,
                    created_at,
                    updated_at,
                }))
            }
            None => Ok(None),
        }
    }

    /// Get threads for a bot
    pub async fn list_by_bot(
        pool: &SqlitePool,
        bot_id: Uuid,
    ) -> Result<Vec<ravenbot_core::Thread>, sqlx::Error> {
        let rows: Vec<ThreadRow> =
            sqlx::query_as("SELECT * FROM threads WHERE bot_id = ? ORDER BY updated_at DESC")
                .bind(bot_id.to_string())
                .fetch_all(pool)
                .await?;

        let threads = rows
            .into_iter()
            .map(|row| {
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
                    channel_id: row
                        .channel_id
                        .as_deref()
                        .and_then(|s| Uuid::parse_str(s).ok()),
                    kind: row
                        .kind
                        .as_deref()
                        .map(ravenbot_core::ThreadKind::from_db)
                        .unwrap_or_default(),
                    visible: row.visible.unwrap_or(1) != 0,
                    created_at,
                    updated_at,
                }
            })
            .collect();

        Ok(threads)
    }

    /// Threads the chat switcher should show. Internal node/delegation/routine
    /// threads stay queryable by id but never appear in an agent's history.
    pub async fn list_visible_by_bot(
        pool: &SqlitePool,
        bot_id: Uuid,
    ) -> Result<Vec<ravenbot_core::Thread>, sqlx::Error> {
        Ok(Self::list_by_bot(pool, bot_id)
            .await?
            .into_iter()
            .filter(|thread| thread.visible)
            .collect())
    }

    /// Whether a thread belongs in the user-facing history.
    pub async fn is_visible(pool: &SqlitePool, thread_id: Uuid) -> Result<bool, sqlx::Error> {
        let row: Option<(i64,)> = sqlx::query_as("SELECT visible FROM threads WHERE id = ?")
            .bind(thread_id.to_string())
            .fetch_optional(pool)
            .await?;
        Ok(row.map(|(visible,)| visible != 0).unwrap_or(false))
    }
}

/// Message-related queries
pub struct MessageQueries;

impl MessageQueries {
    async fn has_message_sender_columns(pool: &SqlitePool) -> bool {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM pragma_table_info('messages') WHERE name = 'sender_bot_id'",
        )
        .fetch_one(pool)
        .await
        .unwrap_or(0)
            > 0
    }

    /// Insert a message
    pub async fn insert(
        pool: &SqlitePool,
        message: &ravenbot_core::Message,
    ) -> Result<(), sqlx::Error> {
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
                "INSERT INTO messages (id, thread_id, role, content, attachments, sender_bot_id, sender_name, reply_to_id, context_used_tokens, context_window_tokens, context_compacted, context_system_tokens, context_tool_tokens, context_memory_tokens, context_message_tokens, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(message.id.to_string())
            .bind(message.thread_id.to_string())
            .bind(role)
            .bind(&content)
            .bind(&attachments)
            .bind(message.sender_bot_id.map(|id| id.to_string()))
            .bind(&message.sender_name)
            .bind(message.reply_to_id.map(|id| id.to_string()))
            .bind(message.context_used_tokens.map(|value| value as i64))
            .bind(message.context_window_tokens.map(|value| value as i64))
            .bind(if message.context_compacted { 1_i64 } else { 0 })
            .bind(message.context_system_tokens.map(|value| value as i64))
            .bind(message.context_tool_tokens.map(|value| value as i64))
            .bind(message.context_memory_tokens.map(|value| value as i64))
            .bind(message.context_message_tokens.map(|value| value as i64))
            .bind(message.created_at.to_rfc3339())
            .execute(pool)
            .await?;
        } else {
            sqlx::query(
                "INSERT INTO messages (id, thread_id, role, content, attachments, context_used_tokens, context_window_tokens, context_compacted, context_system_tokens, context_tool_tokens, context_memory_tokens, context_message_tokens, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(message.id.to_string())
            .bind(message.thread_id.to_string())
            .bind(role)
            .bind(&content)
            .bind(&attachments)
            .bind(message.context_used_tokens.map(|value| value as i64))
            .bind(message.context_window_tokens.map(|value| value as i64))
            .bind(if message.context_compacted { 1_i64 } else { 0 })
            .bind(message.context_system_tokens.map(|value| value as i64))
            .bind(message.context_tool_tokens.map(|value| value as i64))
            .bind(message.context_memory_tokens.map(|value| value as i64))
            .bind(message.context_message_tokens.map(|value| value as i64))
            .bind(message.created_at.to_rfc3339())
            .execute(pool)
            .await?;
        }

        Ok(())
    }

    /// Delete a message by id (used by regenerate)
    pub async fn delete(pool: &SqlitePool, message_id: Uuid) -> Result<(), sqlx::Error> {
        // Reactions are keyed by message id without a foreign key (see
        // migration 029), so they are removed in the same transaction as the
        // message. Without this, re-using or regenerating a turn would leave
        // orphan rows that a future message could inherit.
        let mut tx = pool.begin().await?;
        sqlx::query("DELETE FROM message_reactions WHERE message_id = ?")
            .bind(message_id.to_string())
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM messages WHERE id = ?")
            .bind(message_id.to_string())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Get messages for a thread
    pub async fn list_by_thread(
        pool: &SqlitePool,
        thread_id: Uuid,
    ) -> Result<Vec<ravenbot_core::Message>, sqlx::Error> {
        let rows: Vec<MessageRow> =
            sqlx::query_as("SELECT * FROM messages WHERE thread_id = ? ORDER BY created_at ASC")
                .bind(thread_id.to_string())
                .fetch_all(pool)
                .await?;

        // One query for the whole thread. A reaction lookup per message would
        // be N+1 on the most frequently opened screen in the app.
        let reaction_rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT r.message_id, r.emoji
             FROM message_reactions r
             JOIN messages m ON m.id = r.message_id
             WHERE m.thread_id = ?
             ORDER BY r.created_at ASC, r.rowid ASC",
        )
        .bind(thread_id.to_string())
        .fetch_all(pool)
        .await?;
        let mut reactions_by_message: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for (message_id, emoji) in reaction_rows {
            reactions_by_message
                .entry(message_id)
                .or_default()
                .push(emoji);
        }

        hydrate_messages(rows, reactions_by_message)
    }

    /// Fetch specific messages in a thread, used to resolve reply targets that
    /// are older than the loaded page. Unknown ids are omitted.
    pub async fn list_by_ids(
        pool: &SqlitePool,
        thread_id: Uuid,
        ids: &[Uuid],
    ) -> Result<Vec<ravenbot_core::Message>, sqlx::Error> {
        let ids: Vec<String> = ids.iter().take(100).map(|id| id.to_string()).collect();
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = vec!["?"; ids.len()].join(",");
        let sql = format!(
            "SELECT * FROM messages WHERE thread_id = ? AND id IN ({placeholders})
             ORDER BY created_at ASC, rowid ASC"
        );
        let mut query = sqlx::query_as::<_, MessageRow>(sqlx::AssertSqlSafe(sql.as_str()));
        query = query.bind(thread_id.to_string());
        for id in &ids {
            query = query.bind(id);
        }
        let rows = query.fetch_all(pool).await?;
        let mut reactions_by_message: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        let response_ids: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
        if !response_ids.is_empty() {
            let placeholders = vec!["?"; response_ids.len()].join(",");
            let sql = format!(
                "SELECT message_id, emoji FROM message_reactions
                 WHERE message_id IN ({placeholders})
                 ORDER BY created_at ASC, rowid ASC"
            );
            let mut query =
                sqlx::query_as::<_, (String, String)>(sqlx::AssertSqlSafe(sql.as_str()));
            for id in &response_ids {
                query = query.bind(id);
            }
            for (message_id, emoji) in query.fetch_all(pool).await? {
                reactions_by_message
                    .entry(message_id)
                    .or_default()
                    .push(emoji);
            }
        }
        hydrate_messages(rows, reactions_by_message)
    }

    /// One newest-first page of a thread, returned in chronological order.
    ///
    /// `before` is a message id cursor: the page contains messages strictly
    /// older than it. A page with `has_more` may have older history available.
    pub async fn list_page(
        pool: &SqlitePool,
        thread_id: Uuid,
        before: Option<Uuid>,
        limit: usize,
    ) -> Result<MessagePage, sqlx::Error> {
        let limit = limit.clamp(1, 200);
        let fetch = (limit + 1) as i64;
        let mut rows: Vec<MessageRow> = if let Some(before) = before {
            sqlx::query_as(
                "WITH cursor AS (
                     SELECT created_at, rowid AS rid FROM messages
                     WHERE id = ? AND thread_id = ?
                 )
                 SELECT * FROM messages
                 WHERE thread_id = ?
                   AND (created_at, rowid) < (SELECT created_at, rid FROM cursor)
                 ORDER BY created_at DESC, rowid DESC
                 LIMIT ?",
            )
            .bind(before.to_string())
            .bind(thread_id.to_string())
            .bind(thread_id.to_string())
            .bind(fetch)
            .fetch_all(pool)
            .await?
        } else {
            sqlx::query_as(
                "SELECT * FROM messages WHERE thread_id = ?
                 ORDER BY created_at DESC, rowid DESC LIMIT ?",
            )
            .bind(thread_id.to_string())
            .bind(fetch)
            .fetch_all(pool)
            .await?
        };

        let has_more = rows.len() > limit;
        rows.truncate(limit);
        rows.reverse();

        let ids: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
        let mut reactions_by_message: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        if !ids.is_empty() {
            let placeholders = vec!["?"; ids.len()].join(",");
            let sql = format!(
                "SELECT message_id, emoji FROM message_reactions
                 WHERE message_id IN ({placeholders})
                 ORDER BY created_at ASC, rowid ASC"
            );
            let mut query =
                sqlx::query_as::<_, (String, String)>(sqlx::AssertSqlSafe(sql.as_str()));
            for id in &ids {
                query = query.bind(id);
            }
            for (message_id, emoji) in query.fetch_all(pool).await? {
                reactions_by_message
                    .entry(message_id)
                    .or_default()
                    .push(emoji);
            }
        }

        let messages = hydrate_messages(rows, reactions_by_message)?;
        Ok(MessagePage { messages, has_more })
    }

    /// Messages strictly after `after_id` in insertion order.
    ///
    /// The floor is a message id rather than a timestamp because several
    /// messages can share a millisecond; `rowid` is the insertion order the
    /// transcript already uses.
    pub async fn list_after(
        pool: &SqlitePool,
        thread_id: Uuid,
        after_id: Uuid,
    ) -> Result<Vec<ravenbot_core::Message>, sqlx::Error> {
        let rows: Vec<MessageRow> = sqlx::query_as(
            "SELECT * FROM messages
             WHERE thread_id = ?
               AND rowid > (SELECT rowid FROM messages WHERE id = ?)
             ORDER BY rowid ASC",
        )
        .bind(thread_id.to_string())
        .bind(after_id.to_string())
        .fetch_all(pool)
        .await?;
        let ids: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
        let mut reactions_by_message: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        if !ids.is_empty() {
            let placeholders = vec!["?"; ids.len()].join(",");
            let sql = format!(
                "SELECT message_id, emoji FROM message_reactions
                 WHERE message_id IN ({placeholders})
                 ORDER BY created_at ASC, rowid ASC"
            );
            let mut query =
                sqlx::query_as::<_, (String, String)>(sqlx::AssertSqlSafe(sql.as_str()));
            for id in &ids {
                query = query.bind(id);
            }
            for (message_id, emoji) in query.fetch_all(pool).await? {
                reactions_by_message
                    .entry(message_id)
                    .or_default()
                    .push(emoji);
            }
        }
        hydrate_messages(rows, reactions_by_message)
    }

    pub async fn exists(pool: &SqlitePool, message_id: Uuid) -> Result<bool, sqlx::Error> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE id = ?")
            .bind(message_id.to_string())
            .fetch_one(pool)
            .await?;
        Ok(count > 0)
    }
}

/// A page of a thread, in chronological order, plus whether older messages
/// remain before the first item.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessagePage {
    pub messages: Vec<ravenbot_core::Message>,
    pub has_more: bool,
}

fn hydrate_messages(
    rows: Vec<MessageRow>,
    mut reactions_by_message: std::collections::HashMap<String, Vec<String>>,
) -> Result<Vec<ravenbot_core::Message>, sqlx::Error> {
    let mut messages = Vec::with_capacity(rows.len());
    for row in rows {
        messages.push(hydrate_message(row, &mut reactions_by_message));
    }
    Ok(messages)
}

/// Decode one row. A corrupt row becomes a visible placeholder instead of
/// failing the whole page; one bad legacy value must not lock a transcript.
fn hydrate_message(
    row: MessageRow,
    reactions_by_message: &mut std::collections::HashMap<String, Vec<String>>,
) -> ravenbot_core::Message {
    let role = match row.role.as_str() {
        "user" => ravenbot_core::MessageRole::User,
        "assistant" => ravenbot_core::MessageRole::Assistant,
        "system" => ravenbot_core::MessageRole::System,
        "tool" => ravenbot_core::MessageRole::Tool,
        _ => ravenbot_core::MessageRole::User,
    };
    let content = serde_json::from_str::<ravenbot_core::MessageContent>(&row.content)
        .unwrap_or_else(|error| {
            tracing::warn!(
                message_id = %row.id,
                %error,
                "message content could not be decoded; showing a placeholder"
            );
            ravenbot_core::MessageContent::Text {
                text: format!(
                    "⚠️ This message could not be decoded ({error}). Its original content is preserved in the database."
                ),
                sources: Vec::new(),
                reasoning: None,
                tools: Vec::new(),
                spans: Vec::new(),
            }
        });
    let attachments: Vec<ravenbot_core::Attachment> = serde_json::from_str(&row.attachments)
        .unwrap_or_else(|error| {
            tracing::warn!(
                message_id = %row.id,
                %error,
                "message attachments could not be decoded; dropping them"
            );
            Vec::new()
        });

    ravenbot_core::Message {
        id: Uuid::parse_str(&row.id).unwrap_or_default(),
        thread_id: Uuid::parse_str(&row.thread_id).unwrap_or_default(),
        role,
        content,
        attachments,
        sender_bot_id: row
            .sender_bot_id
            .as_deref()
            .and_then(|v| Uuid::parse_str(v).ok()),
        sender_name: row.sender_name.clone(),
        reply_to_id: row
            .reply_to_id
            .as_deref()
            .and_then(|v| Uuid::parse_str(v).ok()),
        reactions: reactions_by_message.remove(&row.id).unwrap_or_default(),
        context_used_tokens: row.context_used_tokens.map(|value| value.max(0) as u64),
        context_window_tokens: row.context_window_tokens.map(|value| value.max(0) as u64),
        context_compacted: row.context_compacted.unwrap_or(0) != 0,
        context_system_tokens: row.context_system_tokens.map(|value| value.max(0) as u64),
        context_tool_tokens: row.context_tool_tokens.map(|value| value.max(0) as u64),
        context_memory_tokens: row.context_memory_tokens.map(|value| value.max(0) as u64),
        context_message_tokens: row.context_message_tokens.map(|value| value.max(0) as u64),
        created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now()),
    }
}

/// One persisted context handoff.
#[derive(Debug, Clone, Serialize)]
pub struct ContextSummary {
    pub id: String,
    pub thread_id: String,
    pub through_message_id: Option<String>,
    pub summary: String,
    pub token_estimate: i64,
    pub model: String,
    pub created_at: String,
}

pub struct ContextSummaryQueries;

impl ContextSummaryQueries {
    pub async fn latest(
        pool: &SqlitePool,
        thread_id: Uuid,
    ) -> Result<Option<ContextSummary>, sqlx::Error> {
        let row: Option<(
            String,
            String,
            Option<String>,
            String,
            i64,
            String,
            String,
        )> = sqlx::query_as(
            "SELECT id, thread_id, through_message_id, summary, token_estimate, model, created_at
             FROM context_summaries WHERE thread_id = ?
             ORDER BY created_at DESC, rowid DESC LIMIT 1",
        )
        .bind(thread_id.to_string())
        .fetch_optional(pool)
        .await?;
        Ok(row.map(
            |(id, thread_id, through_message_id, summary, token_estimate, model, created_at)| {
                ContextSummary {
                    id,
                    thread_id,
                    through_message_id,
                    summary,
                    token_estimate,
                    model,
                    created_at,
                }
            },
        ))
    }

    /// Store a summary and point the thread at it. The floor is the last
    /// message the summary covers through; later runs load messages after it.
    #[allow(clippy::too_many_arguments)]
    pub async fn save(
        pool: &SqlitePool,
        thread_id: Uuid,
        through_message_id: Uuid,
        summary: &str,
        token_estimate: u64,
        model: &str,
    ) -> Result<Uuid, sqlx::Error> {
        let id = Uuid::new_v4();
        let now = Utc::now().to_rfc3339();
        let mut tx = pool.begin().await?;
        sqlx::query(
            "INSERT INTO context_summaries
                (id, thread_id, through_message_id, summary, token_estimate, model, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(thread_id.to_string())
        .bind(through_message_id.to_string())
        .bind(summary)
        .bind(token_estimate.min(i64::MAX as u64) as i64)
        .bind(model)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE threads SET context_summary_id = ?, context_floor_message_id = ?
             WHERE id = ?",
        )
        .bind(id.to_string())
        .bind(through_message_id.to_string())
        .bind(thread_id.to_string())
        .execute(&mut *tx)
        .await?;
        // Keep a bounded history; only the newest summary is replayed.
        sqlx::query(
            "DELETE FROM context_summaries
             WHERE thread_id = ? AND id NOT IN (
                 SELECT id FROM context_summaries WHERE thread_id = ?
                 ORDER BY created_at DESC, rowid DESC LIMIT 20
             )",
        )
        .bind(thread_id.to_string())
        .bind(thread_id.to_string())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(id)
    }

    pub async fn clear(pool: &SqlitePool, thread_id: Uuid) -> Result<(), sqlx::Error> {
        let mut tx = pool.begin().await?;
        sqlx::query("DELETE FROM context_summaries WHERE thread_id = ?")
            .bind(thread_id.to_string())
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "UPDATE threads SET context_summary_id = NULL, context_floor_message_id = NULL
             WHERE id = ?",
        )
        .bind(thread_id.to_string())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }
}

/// Run-related queries
#[derive(Debug, Clone, Serialize)]
pub struct RunActor {
    pub run_id: String,
    pub actor_id: String,
    pub actor_name: String,
    pub role: String,
    pub workspace_id: String,
}

pub struct RunQueries;

impl RunQueries {
    pub async fn set_actor(
        pool: &SqlitePool,
        run_id: Uuid,
        actor_id: &str,
        actor_name: &str,
        role: &str,
        workspace_id: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO run_actors (run_id, actor_id, actor_name, role, workspace_id)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(run_id) DO UPDATE SET
               actor_id = excluded.actor_id,
               actor_name = excluded.actor_name,
               role = excluded.role,
               workspace_id = excluded.workspace_id",
        )
        .bind(run_id.to_string())
        .bind(actor_id)
        .bind(actor_name)
        .bind(role)
        .bind(workspace_id)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn actor_for(
        pool: &SqlitePool,
        run_id: Uuid,
    ) -> Result<Option<RunActor>, sqlx::Error> {
        let row: Option<(String, String, String, String, String)> = sqlx::query_as(
            "SELECT run_id, actor_id, actor_name, role, workspace_id
             FROM run_actors WHERE run_id = ?",
        )
        .bind(run_id.to_string())
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|row| RunActor {
            run_id: row.0,
            actor_id: row.1,
            actor_name: row.2,
            role: row.3,
            workspace_id: row.4,
        }))
    }

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

        let checkpoint = run
            .checkpoint
            .as_ref()
            .map(|c| serde_json::to_string(c).unwrap_or_default());
        let outcome = run
            .outcome
            .as_ref()
            .map(|o| serde_json::to_string(o).unwrap_or_default());

        sqlx::query(
            "INSERT INTO runs (id, bot_id, thread_id, parent_run_id, state, checkpoint, outcome, tokens_consumed, cost_estimate, model_cost, tool_cost, created_at, updated_at, completed_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
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
        .bind(run.model_cost)
        .bind(run.tool_cost)
        .bind(run.created_at.to_rfc3339())
        .bind(run.updated_at.to_rfc3339())
        .bind(run.completed_at.map(|dt| dt.to_rfc3339()))
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Get a run by ID
    pub async fn get(
        pool: &SqlitePool,
        id: Uuid,
    ) -> Result<Option<ravenbot_core::Run>, sqlx::Error> {
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
                    parent_run_id: row
                        .parent_run_id
                        .as_ref()
                        .and_then(|s| Uuid::parse_str(s).ok()),
                    state,
                    checkpoint: row
                        .checkpoint
                        .as_ref()
                        .and_then(|s| serde_json::from_str(s).ok()),
                    outcome: row
                        .outcome
                        .as_ref()
                        .and_then(|s| serde_json::from_str(s).ok()),
                    tokens_consumed: row.tokens_consumed as u64,
                    cost_estimate: row.cost_estimate,
                    model_cost: row.model_cost,
                    tool_cost: row.tool_cost,
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

    /// Park runs that were active when the process quit so startup can resume
    /// them from their checkpoint. Deliberately limited to top-level, plain
    /// 1:1 threads: office graph nodes and delegated child runs have no
    /// independent restart path, and a 1:1 goal round is retried as a whole
    /// round so it cannot skip the goal's review/verdict step.
    pub async fn park_interrupted_runs(pool: &SqlitePool) -> Result<u64, sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE runs
             SET state = 'paused', resume_on_start = 1, updated_at = ?, completed_at = NULL
             WHERE state IN ('planning','acting','observing','reflecting','waiting_on_user')
               AND parent_run_id IS NULL
               AND thread_id NOT IN (SELECT thread_id FROM chatroom_threads)
               AND thread_id NOT IN (SELECT thread_id FROM chatroom_office_threads)
               AND thread_id NOT IN (
                   SELECT thread_id FROM channel_tasks WHERE thread_id IS NOT NULL
               )
               -- Background task runs are reconciled by the task loop and must
               -- not be resumed as ordinary 1:1 chats.
               AND thread_id NOT IN (
                   SELECT thread_id FROM agent_tasks WHERE thread_id IS NOT NULL
               )
               -- A 1:1 goal owns its round. Resuming the raw run would skip
               -- the goal's review/verdict step and leave the goal due, so the
               -- round is closed and the goal loop starts a fresh one.
               AND thread_id NOT IN (
                   SELECT thread_id FROM goals
                   WHERE office_id IS NULL AND thread_id IS NOT NULL AND status != 'done'
               )",
        )
        .bind(now)
        .execute(pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Close active office, graph-child, channel-task, and 1:1 goal runs that
    /// cannot be resumed independently. They are reported as interrupted
    /// failures.
    pub async fn fail_orphaned_runs(pool: &SqlitePool) -> Result<u64, sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE runs
             SET state = 'failed', outcome = ?, resume_on_start = 0,
                 updated_at = ?, completed_at = ?
             WHERE state IN ('planning','acting','observing','reflecting','waiting_on_user')
               AND (
                   parent_run_id IS NOT NULL
                   OR thread_id IN (SELECT thread_id FROM chatroom_threads)
                   OR thread_id IN (SELECT thread_id FROM chatroom_office_threads)
                   OR thread_id IN (
                       SELECT thread_id FROM channel_tasks WHERE thread_id IS NOT NULL
                   )
                   OR thread_id IN (
                       SELECT thread_id FROM agent_tasks WHERE thread_id IS NOT NULL
                   )
                   OR thread_id IN (
                       SELECT thread_id FROM goals
                       WHERE office_id IS NULL AND thread_id IS NOT NULL AND status != 'done'
                   )
               )",
        )
        .bind(r#"{"Failure":{"error":"app quit while the run was active"}}"#)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Claim one interrupted run for the startup resume worker.
    ///
    /// The flag is cleared in the same transaction as the selection so two
    /// startup paths (desktop and daemon) cannot execute the same run twice.
    pub async fn claim_resumable_run(
        pool: &SqlitePool,
    ) -> Result<Option<ravenbot_core::Run>, sqlx::Error> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM runs
             WHERE resume_on_start = 1 AND state = 'paused'
             ORDER BY updated_at ASC
             LIMIT 1",
        )
        .fetch_optional(pool)
        .await?;
        let Some((id,)) = row else {
            return Ok(None);
        };
        let Some(id) = Uuid::parse_str(&id).ok() else {
            return Ok(None);
        };
        // Read before clearing the flag: a transient read error must not make
        // the run unclaimable forever.
        let Some(run) = Self::get(pool, id).await? else {
            return Ok(None);
        };
        let claimed = sqlx::query(
            "UPDATE runs SET resume_on_start = 0
             WHERE id = ? AND resume_on_start = 1 AND state = 'paused'",
        )
        .bind(id.to_string())
        .execute(pool)
        .await?;
        if claimed.rows_affected() == 0 {
            return Ok(None);
        }
        Ok(Some(run))
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

        let checkpoint = run
            .checkpoint
            .as_ref()
            .map(|c| serde_json::to_string(c).unwrap_or_default());
        let outcome = run
            .outcome
            .as_ref()
            .map(|o| serde_json::to_string(o).unwrap_or_default());

        sqlx::query(
            "UPDATE runs SET state = ?, checkpoint = ?, outcome = ?, tokens_consumed = ?, cost_estimate = ?, model_cost = ?, tool_cost = ?, updated_at = ?, completed_at = ?
             WHERE id = ?"
        )
        .bind(state)
        .bind(&checkpoint)
        .bind(&outcome)
        .bind(run.tokens_consumed as i64)
        .bind(run.cost_estimate)
        .bind(run.model_cost)
        .bind(run.tool_cost)
        .bind(run.updated_at.to_rfc3339())
        .bind(run.completed_at.map(|dt| dt.to_rfc3339()))
        .bind(run.id.to_string())
        .execute(pool)
        .await?;

        Ok(())
    }
}

/// Liveness for one run. `heartbeat_at` answers "is it still moving?";
/// `updated_at` only changes when the run's state changes.
#[derive(Debug, Clone, Serialize)]
pub struct RunProgress {
    pub run_id: String,
    pub thread_id: String,
    pub state: String,
    pub phase: String,
    pub last_tool: Option<String>,
    pub last_error: Option<String>,
    pub heartbeat_at: Option<chrono::DateTime<chrono::Utc>>,
    pub lease_owner: Option<String>,
    pub lease_expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// One append-only ledger entry. `work_item_id` is reserved for Phase 1B; today
/// events are keyed by run and thread.
#[derive(Debug, Clone, Serialize)]
pub struct WorkEvent {
    pub id: String,
    pub work_item_id: Option<String>,
    pub run_id: Option<String>,
    pub thread_id: Option<String>,
    pub kind: String,
    pub payload: serde_json::Value,
    pub created_at: String,
}

type ProgressRow = (
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
);

fn parse_event_time(value: Option<String>) -> Option<chrono::DateTime<chrono::Utc>> {
    value.and_then(|value| {
        chrono::DateTime::parse_from_rfc3339(&value)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .ok()
    })
}

/// Run liveness heartbeats and the append-only work ledger.
///
/// Heartbeats are deliberately separate from `RunQueries::update`: a heartbeat
/// must not rewrite the checkpoint, outcome, or token totals, and a state write
/// must not make a stuck run look alive. They also run at different frequencies.
pub struct RunProgressQueries;

impl RunProgressQueries {
    /// Mark a run as started and clear a previous failure.
    pub async fn start(pool: &SqlitePool, run_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE runs
             SET heartbeat_at = ?, phase = 'starting', last_error = NULL
             WHERE id = ?",
        )
        .bind(Utc::now().to_rfc3339())
        .bind(run_id.to_string())
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Record forward motion. `tool` names the tool currently executing, if any.
    pub async fn touch(
        pool: &SqlitePool,
        run_id: Uuid,
        phase: &str,
        tool: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        if let Some(tool) = tool {
            sqlx::query("UPDATE runs SET heartbeat_at = ?, phase = ?, last_tool = ? WHERE id = ?")
                .bind(&now)
                .bind(phase)
                .bind(tool)
                .bind(run_id.to_string())
                .execute(pool)
                .await?;
        } else {
            sqlx::query("UPDATE runs SET heartbeat_at = ?, phase = ? WHERE id = ?")
                .bind(&now)
                .bind(phase)
                .bind(run_id.to_string())
                .execute(pool)
                .await?;
        }
        Ok(())
    }

    /// Record the latest error without making it the run's terminal outcome.
    pub async fn error(
        pool: &SqlitePool,
        run_id: Uuid,
        phase: &str,
        message: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE runs SET heartbeat_at = ?, phase = ?, last_error = ? WHERE id = ?")
            .bind(Utc::now().to_rfc3339())
            .bind(phase)
            .bind(message)
            .bind(run_id.to_string())
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Terminal phase. The heartbeat is refreshed so a watchdog reading the
    /// row after completion does not see a stale running state.
    pub async fn finish(
        pool: &SqlitePool,
        run_id: Uuid,
        phase: &str,
        error: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE runs SET heartbeat_at = ?, phase = ?, last_error = ? WHERE id = ?")
            .bind(Utc::now().to_rfc3339())
            .bind(phase)
            .bind(error)
            .bind(run_id.to_string())
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Append a ledger event. A payload that is not JSON is wrapped so the
    /// column stays parseable rather than silently accepting arbitrary text.
    pub async fn record(
        pool: &SqlitePool,
        run_id: Option<Uuid>,
        thread_id: Option<Uuid>,
        kind: &str,
        payload: serde_json::Value,
    ) -> Result<Uuid, sqlx::Error> {
        Self::record_for_work_item(pool, None, run_id, thread_id, kind, payload).await
    }

    /// Record an event that belongs to a durable work item.
    pub async fn record_for_work_item(
        pool: &SqlitePool,
        work_item_id: Option<&str>,
        run_id: Option<Uuid>,
        thread_id: Option<Uuid>,
        kind: &str,
        payload: serde_json::Value,
    ) -> Result<Uuid, sqlx::Error> {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO work_events
                (id, work_item_id, run_id, thread_id, kind, payload_json, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(work_item_id)
        .bind(run_id.map(|id| id.to_string()))
        .bind(thread_id.map(|id| id.to_string()))
        .bind(kind)
        .bind(payload.to_string())
        .bind(Utc::now().to_rfc3339())
        .execute(pool)
        .await?;
        Ok(id)
    }

    pub async fn get(pool: &SqlitePool, run_id: Uuid) -> Result<Option<RunProgress>, sqlx::Error> {
        let row: Option<ProgressRow> = sqlx::query_as(
            "SELECT id, thread_id, state, phase, last_tool, last_error,
                        heartbeat_at, lease_owner, lease_expires_at, updated_at
                 FROM runs WHERE id = ?",
        )
        .bind(run_id.to_string())
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|row| RunProgress {
            run_id: row.0,
            thread_id: row.1,
            state: row.2,
            phase: row.3,
            last_tool: row.4,
            last_error: row.5,
            heartbeat_at: parse_event_time(row.6),
            lease_owner: row.7,
            lease_expires_at: parse_event_time(row.8),
            updated_at: parse_event_time(Some(row.9)).unwrap_or_else(Utc::now),
        }))
    }

    /// Whether a live or user-paused run currently owns this thread.
    ///
    /// The thread-goal scheduler uses this to yield to a user turn instead of
    /// starting a second run in the same conversation. A paused run counts:
    /// resuming it later must not interleave with a goal round that started
    /// while it was parked.
    pub async fn has_active_for_thread(
        pool: &SqlitePool,
        thread_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let exists: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM runs
             WHERE thread_id = ?
               AND state IN ('planning','acting','observing','reflecting','waiting_on_user','paused')
             LIMIT 1",
        )
        .bind(thread_id.to_string())
        .fetch_optional(pool)
        .await?;
        Ok(exists.is_some())
    }

    /// Active, non-paused runs for one agent. Used by `interrupt_agent`; a
    /// paused run is deliberately not included because there is no live work
    /// to stop until someone resumes it.
    pub async fn active_ids_for_bot(
        pool: &SqlitePool,
        bot_id: Uuid,
    ) -> Result<Vec<Uuid>, sqlx::Error> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT id FROM runs
             WHERE bot_id = ?
               AND state IN ('planning','acting','observing','reflecting','waiting_on_user')
             ORDER BY created_at ASC",
        )
        .bind(bot_id.to_string())
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|(id,)| Uuid::parse_str(&id).ok())
            .collect())
    }

    /// Every non-terminal run currently attached to one thread.
    ///
    /// Used when a user stops a 1:1 goal: scheduling alone would leave the
    /// round already in flight running to completion, which is not what
    /// "stop" means in the conversation.
    pub async fn active_ids_for_thread(
        pool: &SqlitePool,
        thread_id: Uuid,
    ) -> Result<Vec<Uuid>, sqlx::Error> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT id FROM runs
             WHERE thread_id = ?
               AND state IN ('planning','acting','observing','reflecting','waiting_on_user','paused')
             ORDER BY created_at ASC",
        )
        .bind(thread_id.to_string())
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|(id,)| Uuid::parse_str(&id).ok())
            .collect())
    }

    /// Active runs whose last heartbeat is older than `threshold_secs`.
    ///
    /// `updated_at` is the fallback for a run created before heartbeat support
    /// or one that has not reached its first touch yet. Terminal and paused
    /// runs are never reported: they are not expected to move.
    pub async fn stalled(
        pool: &SqlitePool,
        threshold_secs: u64,
        limit: u32,
    ) -> Result<Vec<RunProgress>, sqlx::Error> {
        let cutoff = (Utc::now()
            - chrono::Duration::seconds(threshold_secs.min(i64::MAX as u64) as i64))
        .to_rfc3339();
        let rows: Vec<ProgressRow> = sqlx::query_as(
            "SELECT id, thread_id, state, phase, last_tool, last_error,
                        heartbeat_at, lease_owner, lease_expires_at, updated_at
                 FROM runs
                 WHERE state IN ('planning','acting','observing','reflecting','waiting_on_user')
                   AND COALESCE(heartbeat_at, updated_at) < ?
                 ORDER BY COALESCE(heartbeat_at, updated_at) ASC
                 LIMIT ?",
        )
        .bind(cutoff)
        .bind(i64::from(limit.clamp(1, 500)))
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| RunProgress {
                run_id: row.0,
                thread_id: row.1,
                state: row.2,
                phase: row.3,
                last_tool: row.4,
                last_error: row.5,
                heartbeat_at: parse_event_time(row.6),
                lease_owner: row.7,
                lease_expires_at: parse_event_time(row.8),
                updated_at: parse_event_time(Some(row.9)).unwrap_or_else(Utc::now),
            })
            .collect())
    }

    pub async fn events_for_run(
        pool: &SqlitePool,
        run_id: Uuid,
        limit: u32,
    ) -> Result<Vec<WorkEvent>, sqlx::Error> {
        let rows: Vec<(
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            String,
            String,
            String,
        )> = sqlx::query_as(
            "SELECT id, work_item_id, run_id, thread_id, kind, payload_json, created_at
                 FROM work_events WHERE run_id = ?
                 ORDER BY created_at ASC LIMIT ?",
        )
        .bind(run_id.to_string())
        .bind(i64::from(limit.clamp(1, 1000)))
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| WorkEvent {
                id: row.0,
                work_item_id: row.1,
                run_id: row.2,
                thread_id: row.3,
                kind: row.4,
                payload: serde_json::from_str(&row.5).unwrap_or(serde_json::Value::Null),
                created_at: row.6,
            })
            .collect())
    }

    /// Keep the ledger bounded: drop entries older than `keep_days`, then trim
    /// the remainder to the newest `max_rows`.
    pub async fn prune(
        pool: &SqlitePool,
        keep_days: i64,
        max_rows: i64,
    ) -> Result<u64, sqlx::Error> {
        let cutoff = (Utc::now() - chrono::Duration::days(keep_days.max(1))).to_rfc3339();
        let by_age = sqlx::query("DELETE FROM work_events WHERE created_at < ?")
            .bind(cutoff)
            .execute(pool)
            .await?
            .rows_affected();
        let by_count = sqlx::query(
            "DELETE FROM work_events WHERE id NOT IN (
                 SELECT id FROM work_events ORDER BY created_at DESC LIMIT ?
             )",
        )
        .bind(max_rows.max(1))
        .execute(pool)
        .await?
        .rows_affected();
        Ok(by_age + by_count)
    }
}

/// One accountable piece of work in the durable ledger.
#[derive(Debug, Clone, Serialize)]
pub struct WorkItem {
    pub id: String,
    pub kind: String,
    pub parent_id: Option<String>,
    pub office_id: Option<String>,
    pub thread_id: Option<String>,
    pub run_id: Option<String>,
    pub title: String,
    pub instruction: String,
    pub contract: serde_json::Value,
    pub status: String,
    pub owner_bot_id: Option<String>,
    /// The person who claimed the task for coordination, if any.
    pub owner_account: Option<String>,
    /// When the human claim was taken. Preserved when the same person re-claims.
    pub claimed_at: Option<String>,
    pub reviewer_bot_id: Option<String>,
    pub attempts: i64,
    pub max_attempts: i64,
    pub priority: i64,
    pub created_at: String,
    pub started_at: Option<String>,
    pub updated_at: String,
    pub completed_at: Option<String>,
    pub lease_owner: Option<String>,
    pub lease_expires_at: Option<String>,
    pub heartbeat_at: Option<String>,
    pub workspace: Option<String>,
    pub result: Option<String>,
    pub error: Option<String>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkComment {
    pub id: String,
    pub work_item_id: String,
    pub author: String,
    pub body: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkArtifact {
    pub id: String,
    pub work_item_id: String,
    pub path: String,
    pub sha256: Option<String>,
    pub mime: Option<String>,
    pub size: Option<i64>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkItemDetail {
    pub item: WorkItem,
    pub comments: Vec<WorkComment>,
    pub artifacts: Vec<WorkArtifact>,
    pub events: Vec<WorkEvent>,
}

/// Fields needed to seed one graph node into the ledger.
#[derive(Debug, Clone)]
pub struct NewWorkItem {
    pub id: Uuid,
    pub kind: String,
    pub parent_id: Option<Uuid>,
    pub office_id: Option<Uuid>,
    pub thread_id: Option<Uuid>,
    pub run_id: Option<Uuid>,
    pub title: String,
    pub instruction: String,
    pub contract: serde_json::Value,
    pub status: String,
    pub owner_bot_id: Option<Uuid>,
    pub attempts: i64,
    pub max_attempts: i64,
    pub priority: i64,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct WorkItemRow {
    id: String,
    kind: String,
    parent_id: Option<String>,
    office_id: Option<String>,
    thread_id: Option<String>,
    run_id: Option<String>,
    title: String,
    instruction: String,
    contract_json: String,
    status: String,
    owner_bot_id: Option<String>,
    owner_account: Option<String>,
    claimed_at: Option<String>,
    reviewer_bot_id: Option<String>,
    attempts: i64,
    max_attempts: i64,
    priority: i64,
    created_at: String,
    started_at: Option<String>,
    updated_at: String,
    completed_at: Option<String>,
    lease_owner: Option<String>,
    lease_expires_at: Option<String>,
    heartbeat_at: Option<String>,
    workspace: Option<String>,
    result: Option<String>,
    error: Option<String>,
    metadata_json: String,
}

impl From<WorkItemRow> for WorkItem {
    fn from(row: WorkItemRow) -> Self {
        Self {
            id: row.id,
            kind: row.kind,
            parent_id: row.parent_id,
            office_id: row.office_id,
            thread_id: row.thread_id,
            run_id: row.run_id,
            title: row.title,
            instruction: row.instruction,
            contract: serde_json::from_str(&row.contract_json)
                .unwrap_or_else(|_| serde_json::json!({})),
            status: row.status,
            owner_bot_id: row.owner_bot_id,
            owner_account: row.owner_account,
            claimed_at: row.claimed_at,
            reviewer_bot_id: row.reviewer_bot_id,
            attempts: row.attempts,
            max_attempts: row.max_attempts,
            priority: row.priority,
            created_at: row.created_at,
            started_at: row.started_at,
            updated_at: row.updated_at,
            completed_at: row.completed_at,
            lease_owner: row.lease_owner,
            lease_expires_at: row.lease_expires_at,
            heartbeat_at: row.heartbeat_at,
            workspace: row.workspace,
            result: row.result,
            error: row.error,
            metadata: serde_json::from_str(&row.metadata_json)
                .unwrap_or_else(|_| serde_json::json!({})),
        }
    }
}

/// Why a human claim or release could not be applied.
#[derive(Debug)]
pub enum ClaimError {
    /// No work item has this id.
    NotFound,
    /// Another account holds the claim (`Some`), or nobody does (`None`, for a
    /// release). The owner is carried so the caller can name them.
    Conflict {
        owner: Option<String>,
    },
    /// The item's status does not allow the transition (archived work).
    InvalidState(String),
    /// The account name is empty, too long, or contains control characters.
    InvalidAccount(&'static str),
    Database(sqlx::Error),
}

impl std::fmt::Display for ClaimError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "The work item was not found."),
            Self::Conflict { owner: Some(owner) } => {
                write!(f, "This task is already claimed by @{owner}.")
            }
            Self::Conflict { owner: None } => write!(f, "This task is not claimed."),
            Self::InvalidState(status) => {
                write!(f, "A task in '{status}' state cannot be claimed.")
            }
            Self::InvalidAccount(message) => f.write_str(message),
            Self::Database(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for ClaimError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            _ => None,
        }
    }
}

impl From<sqlx::Error> for ClaimError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

/// Normalize an account name for storage. Names are usernames, not display
/// names, so they are trimmed, capped, and rejected if they contain control
/// characters that would corrupt the timeline payload.
fn valid_claim_account(account: &str) -> Result<&str, ClaimError> {
    let account = account.trim();
    if account.is_empty() {
        return Err(ClaimError::InvalidAccount(
            "An account is required to claim a task.",
        ));
    }
    if account.chars().count() > 64 {
        return Err(ClaimError::InvalidAccount(
            "An account name must be at most 64 characters.",
        ));
    }
    if account.chars().any(char::is_control) {
        return Err(ClaimError::InvalidAccount(
            "An account name cannot contain control characters.",
        ));
    }
    Ok(account)
}

/// Durable work items, dependencies, comments, artifacts, and transitions.
///
/// The graph remains the scheduler. Every graph transition is mirrored here so
/// a human, a reviewer, or another agent can read the work without loading a
/// snapshot, and so an interrupted claim has a record of what was running.
pub struct WorkItemQueries;

impl WorkItemQueries {
    /// Insert a planned item, or refresh the planning fields of an existing one.
    /// Seed never moves an item backwards: a resumed graph must not turn a done
    /// task into pending.
    ///
    /// `owner_account` and `claimed_at` are deliberately absent from the
    /// upsert: a graph replan must never clear or rewrite a person's claim.
    pub async fn seed(pool: &SqlitePool, item: &NewWorkItem) -> Result<(), sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO work_items
                (id, kind, parent_id, office_id, thread_id, run_id, title, instruction,
                 contract_json, status, owner_bot_id, attempts, max_attempts, priority,
                 created_at, updated_at, metadata_json)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                kind = excluded.kind,
                parent_id = COALESCE(excluded.parent_id, work_items.parent_id),
                office_id = COALESCE(excluded.office_id, work_items.office_id),
                thread_id = COALESCE(excluded.thread_id, work_items.thread_id),
                title = excluded.title,
                instruction = excluded.instruction,
                contract_json = excluded.contract_json,
                owner_bot_id = excluded.owner_bot_id,
                max_attempts = excluded.max_attempts,
                priority = excluded.priority,
                metadata_json = excluded.metadata_json,
                updated_at = excluded.updated_at",
        )
        .bind(item.id.to_string())
        .bind(&item.kind)
        .bind(item.parent_id.map(|id| id.to_string()))
        .bind(item.office_id.map(|id| id.to_string()))
        .bind(item.thread_id.map(|id| id.to_string()))
        .bind(item.run_id.map(|id| id.to_string()))
        .bind(&item.title)
        .bind(&item.instruction)
        .bind(item.contract.to_string())
        .bind(&item.status)
        .bind(item.owner_bot_id.map(|id| id.to_string()))
        .bind(item.attempts)
        .bind(item.max_attempts)
        .bind(item.priority)
        .bind(&now)
        .bind(&now)
        .bind(item.metadata.to_string())
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn link(
        pool: &SqlitePool,
        parent_id: Uuid,
        child_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT OR IGNORE INTO work_links (parent_id, child_id) VALUES (?, ?)")
            .bind(parent_id.to_string())
            .bind(child_id.to_string())
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn get(pool: &SqlitePool, id: &str) -> Result<Option<WorkItem>, sqlx::Error> {
        let row: Option<WorkItemRow> = sqlx::query_as("SELECT * FROM work_items WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
        Ok(row.map(WorkItem::from))
    }

    pub async fn list_by_office(
        pool: &SqlitePool,
        office_id: Uuid,
        limit: u32,
    ) -> Result<Vec<WorkItem>, sqlx::Error> {
        let rows: Vec<WorkItemRow> = sqlx::query_as(
            "SELECT * FROM work_items WHERE office_id = ?
             ORDER BY created_at ASC, id ASC LIMIT ?",
        )
        .bind(office_id.to_string())
        .bind(i64::from(limit.clamp(1, 500)))
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(WorkItem::from).collect())
    }

    /// Retire failed attempts that the current plan no longer contains.
    ///
    /// A failed node is useful history, not a permanent backlog row. Without
    /// this, every bad cycle leaves a `failed` item that makes the project
    /// board look unfinished forever and blocks deterministic completion even
    /// after a later verified cycle succeeds. The event log and the item's
    /// result remain; only the open-board status is retired.
    pub async fn retire_stale_failures(
        pool: &SqlitePool,
        office_id: Uuid,
        keep_ids: &[Uuid],
    ) -> Result<u64, sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let placeholders = keep_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = if placeholders.is_empty() {
            "UPDATE work_items
             SET status = 'cancelled',
                 error = 'Superseded by the current office plan; this attempt is kept in the event history.',
                 completed_at = ?, updated_at = ?
             WHERE office_id = ? AND status = 'failed'"
                .to_string()
        } else {
            format!(
                "UPDATE work_items
                 SET status = 'cancelled',
                     error = 'Superseded by the current office plan; this attempt is kept in the event history.',
                     completed_at = ?, updated_at = ?
                 WHERE office_id = ? AND status = 'failed' AND id NOT IN ({placeholders})"
            )
        };
        let mut query = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(&now)
            .bind(&now)
            .bind(office_id.to_string());
        for id in keep_ids {
            query = query.bind(id.to_string());
        }
        let result = query.execute(pool).await?;
        Ok(result.rows_affected())
    }

    /// Retire a running item whose executor lease expired.
    ///
    /// A crashed or abandoned graph leaves its last node in `running` forever;
    /// the durable backlog then shows work that no process owns, and a goal
    /// review sees open work that can never finish. Callers check that no
    /// resumable graph owns the office before retiring anything.
    pub async fn retire_expired_running(
        pool: &SqlitePool,
        office_id: Uuid,
        now: &str,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE work_items
             SET status = 'cancelled',
                 error = 'The run that owned this task ended before it finished; the task is retired so the durable backlog reflects real open work.',
                 completed_at = ?, updated_at = ?
             WHERE office_id = ? AND status = 'running'
               AND lease_expires_at IS NOT NULL AND lease_expires_at < ?",
        )
        .bind(now)
        .bind(now)
        .bind(office_id.to_string())
        .bind(now)
        .execute(pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Open work across the whole project, most actionable first.
    ///
    /// The office board used to read only the last graph snapshot, so failed
    /// and blocked tasks disappeared when the next cycle started. This query
    /// is the durable backlog the planner and the review need.
    pub async fn list_open_by_office(
        pool: &SqlitePool,
        office_id: Uuid,
        limit: u32,
    ) -> Result<Vec<WorkItem>, sqlx::Error> {
        let rows: Vec<WorkItemRow> = sqlx::query_as(
            "SELECT * FROM work_items
             WHERE office_id = ? AND status NOT IN ('done', 'cancelled')
             ORDER BY CASE status
                 WHEN 'running' THEN 0
                 WHEN 'review' THEN 1
                 WHEN 'blocked' THEN 2
                 WHEN 'failed' THEN 3
                 WHEN 'ready' THEN 4
                 WHEN 'pending' THEN 5
                 ELSE 6
             END, updated_at DESC, id DESC
             LIMIT ?",
        )
        .bind(office_id.to_string())
        .bind(i64::from(limit.clamp(1, 500)))
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(WorkItem::from).collect())
    }

    /// Recently completed work, newest first, for the project board.
    pub async fn list_done_by_office(
        pool: &SqlitePool,
        office_id: Uuid,
        limit: u32,
    ) -> Result<Vec<WorkItem>, sqlx::Error> {
        let rows: Vec<WorkItemRow> = sqlx::query_as(
            "SELECT * FROM work_items
             WHERE office_id = ? AND status = 'done'
             ORDER BY COALESCE(completed_at, updated_at) DESC, id DESC
             LIMIT ?",
        )
        .bind(office_id.to_string())
        .bind(i64::from(limit.clamp(1, 200)))
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(WorkItem::from).collect())
    }

    pub async fn list_by_thread(
        pool: &SqlitePool,
        thread_id: Uuid,
        limit: u32,
    ) -> Result<Vec<WorkItem>, sqlx::Error> {
        let rows: Vec<WorkItemRow> = sqlx::query_as(
            "SELECT * FROM work_items WHERE thread_id = ?
             ORDER BY created_at ASC, id ASC LIMIT ?",
        )
        .bind(thread_id.to_string())
        .bind(i64::from(limit.clamp(1, 500)))
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(WorkItem::from).collect())
    }

    /// Atomically claim an item for a person.
    ///
    /// The human claim is coordination state, separate from the graph lease:
    /// it does not move the status, spend an attempt, or touch the instruction,
    /// result, or run attribution. The conditional `UPDATE` is the arbiter, so
    /// two people racing for the same task cannot both see success.
    pub async fn claim(pool: &SqlitePool, id: &str, account: &str) -> Result<WorkItem, ClaimError> {
        let account = valid_claim_account(account)?;
        let id = id.trim();
        if id.is_empty() {
            return Err(ClaimError::NotFound);
        }
        let now = Utc::now().to_rfc3339();

        // Read only to describe the transition and preserve an existing
        // `claimed_at`. The UPDATE condition below decides every race.
        let before = Self::get(pool, id).await?.ok_or(ClaimError::NotFound)?;
        if before.status == "archived" {
            return Err(ClaimError::InvalidState(before.status));
        }

        let changed = sqlx::query(
            "UPDATE work_items
             SET owner_account = ?, claimed_at = COALESCE(claimed_at, ?), updated_at = ?
             WHERE id = ? AND status != 'archived'
               AND (owner_account IS NULL OR owner_account = ?)",
        )
        .bind(account)
        .bind(&now)
        .bind(&now)
        .bind(id)
        .bind(account)
        .execute(pool)
        .await?
        .rows_affected();

        if changed == 0 {
            let current = Self::get(pool, id).await?.ok_or(ClaimError::NotFound)?;
            if current.status == "archived" {
                return Err(ClaimError::InvalidState(current.status));
            }
            return Err(ClaimError::Conflict {
                owner: current.owner_account,
            });
        }

        // Re-claiming a task you already own is idempotent: the row keeps its
        // original `claimed_at`, and there is no transition to record.
        if before.owner_account.as_deref() != Some(account) {
            Self::event(
                pool,
                id,
                None,
                "claimed",
                serde_json::json!({
                    "account": account,
                    "previous_owner": before.owner_account,
                }),
            )
            .await?;
        }

        Self::get(pool, id).await?.ok_or(ClaimError::NotFound)
    }

    /// Release a human claim. Only the account that holds it may release it;
    /// an unowned task and another person's task are both conflicts.
    pub async fn release(
        pool: &SqlitePool,
        id: &str,
        account: &str,
    ) -> Result<WorkItem, ClaimError> {
        let account = valid_claim_account(account)?;
        let id = id.trim();
        if id.is_empty() {
            return Err(ClaimError::NotFound);
        }
        let now = Utc::now().to_rfc3339();

        let changed = sqlx::query(
            "UPDATE work_items
             SET owner_account = NULL, claimed_at = NULL, updated_at = ?
             WHERE id = ? AND owner_account = ?",
        )
        .bind(&now)
        .bind(id)
        .bind(account)
        .execute(pool)
        .await?
        .rows_affected();

        if changed == 0 {
            let current = Self::get(pool, id).await?.ok_or(ClaimError::NotFound)?;
            return Err(ClaimError::Conflict {
                owner: current.owner_account,
            });
        }

        Self::event(
            pool,
            id,
            None,
            "released",
            serde_json::json!({ "account": account, "previous_owner": account }),
        )
        .await?;

        Self::get(pool, id).await?.ok_or(ClaimError::NotFound)
    }

    /// Atomically take the execution lease on an item.
    ///
    /// Two workers may race on the same ready row; only the `UPDATE` that
    /// changes a pending/ready row wins. A failed item with attempts remaining
    /// is retryable, and a currently running item can be re-leased only by the
    /// run that already owns it (resume after a restart).
    ///
    /// This is the graph executor's lease. A person claiming the task for
    /// coordination is [`WorkItemQueries::claim`], which is a separate column
    /// and does not move the task's status or spend an attempt.
    pub async fn claim_for_run(
        pool: &SqlitePool,
        id: &str,
        run_id: Uuid,
        owner: &str,
        lease_secs: u64,
    ) -> Result<bool, sqlx::Error> {
        let now = Utc::now();
        let now_text = now.to_rfc3339();
        let lease_text =
            (now + chrono::Duration::seconds(lease_secs.min(i64::MAX as u64) as i64)).to_rfc3339();

        let first = sqlx::query(
            "UPDATE work_items
             SET status = 'running', run_id = ?, lease_owner = ?, lease_expires_at = ?,
                 heartbeat_at = ?, started_at = COALESCE(started_at, ?),
                 attempts = attempts + 1, updated_at = ?, error = NULL, completed_at = NULL
             WHERE id = ? AND status IN ('pending','ready')",
        )
        .bind(run_id.to_string())
        .bind(owner)
        .bind(&lease_text)
        .bind(&now_text)
        .bind(&now_text)
        .bind(&now_text)
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
        if first == 1 {
            Self::event(
                pool,
                id,
                Some(run_id),
                "claimed",
                serde_json::json!({ "owner": owner, "attempt": 1 }),
            )
            .await?;
            return Ok(true);
        }

        let retry = sqlx::query(
            "UPDATE work_items
             SET status = 'running', run_id = ?, lease_owner = ?, lease_expires_at = ?,
                 heartbeat_at = ?, attempts = attempts + 1, updated_at = ?,
                 error = NULL, completed_at = NULL
             WHERE id = ? AND status = 'failed' AND attempts < max_attempts",
        )
        .bind(run_id.to_string())
        .bind(owner)
        .bind(&lease_text)
        .bind(&now_text)
        .bind(&now_text)
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
        if retry == 1 {
            Self::event(
                pool,
                id,
                Some(run_id),
                "retried",
                serde_json::json!({ "owner": owner }),
            )
            .await?;
            return Ok(true);
        }

        // Resume the exact same run after a restart without consuming another
        // attempt. A different run must not steal a live lease.
        let resumed = sqlx::query(
            "UPDATE work_items
             SET lease_owner = ?, lease_expires_at = ?, heartbeat_at = ?, updated_at = ?
             WHERE id = ? AND status = 'running' AND run_id = ?",
        )
        .bind(owner)
        .bind(&lease_text)
        .bind(&now_text)
        .bind(&now_text)
        .bind(id)
        .bind(run_id.to_string())
        .execute(pool)
        .await?
        .rows_affected();
        if resumed == 1 {
            return Ok(true);
        }

        // A crashed owner's lease expires and the item becomes reclaimable by a
        // new run. Reclaiming does consume an attempt: the previous run may
        // have left a side effect, and the graph's retry policy is what decides
        // whether re-running is safe.
        let reclaimed = sqlx::query(
            "UPDATE work_items
             SET status = 'running', run_id = ?, lease_owner = ?, lease_expires_at = ?,
                 heartbeat_at = ?, attempts = attempts + 1, updated_at = ?,
                 error = NULL, completed_at = NULL
             WHERE id = ? AND status = 'running' AND attempts < max_attempts
               AND lease_expires_at IS NOT NULL AND lease_expires_at < ?",
        )
        .bind(run_id.to_string())
        .bind(owner)
        .bind(&lease_text)
        .bind(&now_text)
        .bind(&now_text)
        .bind(id)
        .bind(&now_text)
        .execute(pool)
        .await?
        .rows_affected();
        if reclaimed == 1 {
            Self::event(
                pool,
                id,
                Some(run_id),
                "lease_reclaimed",
                serde_json::json!({ "owner": owner }),
            )
            .await?;
        }
        Ok(reclaimed == 1)
    }

    /// Reopen a running item for a loop iteration without spending an attempt.
    pub async fn mark_running_again(
        pool: &SqlitePool,
        id: &str,
        run_id: Uuid,
        lease_secs: u64,
    ) -> Result<bool, sqlx::Error> {
        let now = Utc::now();
        let lease_text =
            (now + chrono::Duration::seconds(lease_secs.min(i64::MAX as u64) as i64)).to_rfc3339();
        let changed = sqlx::query(
            "UPDATE work_items
             SET status = 'running', run_id = ?, lease_owner = 'graph-executor',
                 lease_expires_at = ?, heartbeat_at = ?, updated_at = ?,
                 error = NULL, completed_at = NULL
             WHERE id = ?",
        )
        .bind(run_id.to_string())
        .bind(&lease_text)
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
        if changed == 1 {
            Self::event(
                pool,
                id,
                Some(run_id),
                "iteration_started",
                serde_json::json!({}),
            )
            .await?;
        }
        Ok(changed == 1)
    }

    pub async fn attach_thread(
        pool: &SqlitePool,
        id: &str,
        thread_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let changed = sqlx::query(
            "UPDATE work_items SET thread_id = ?, updated_at = ?
             WHERE id = ? AND (thread_id IS NULL OR thread_id != ?)",
        )
        .bind(thread_id.to_string())
        .bind(Utc::now().to_rfc3339())
        .bind(id)
        .bind(thread_id.to_string())
        .execute(pool)
        .await?
        .rows_affected();
        Ok(changed == 1)
    }

    pub async fn heartbeat(
        pool: &SqlitePool,
        id: &str,
        lease_secs: u64,
    ) -> Result<bool, sqlx::Error> {
        let now = Utc::now();
        let lease_text =
            (now + chrono::Duration::seconds(lease_secs.min(i64::MAX as u64) as i64)).to_rfc3339();
        let changed = sqlx::query(
            "UPDATE work_items SET heartbeat_at = ?, lease_expires_at = ?, updated_at = ?
             WHERE id = ? AND status = 'running'",
        )
        .bind(now.to_rfc3339())
        .bind(lease_text)
        .bind(now.to_rfc3339())
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
        Ok(changed == 1)
    }

    pub async fn mark_done(
        pool: &SqlitePool,
        id: &str,
        run_id: Option<Uuid>,
        result: &str,
    ) -> Result<bool, sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let changed = sqlx::query(
            "UPDATE work_items
             SET status = 'done', result = ?, error = NULL, completed_at = ?,
                 updated_at = ?, heartbeat_at = ?, lease_owner = NULL, lease_expires_at = NULL
             WHERE id = ? AND status != 'done'",
        )
        .bind(result)
        .bind(&now)
        .bind(&now)
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
        if changed == 1 {
            Self::event(
                pool,
                id,
                run_id,
                "completed",
                serde_json::json!({ "result_preview": preview_text(result, 500) }),
            )
            .await?;
        }
        Ok(changed == 1)
    }

    pub async fn mark_failed(
        pool: &SqlitePool,
        id: &str,
        run_id: Option<Uuid>,
        error: &str,
    ) -> Result<bool, sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let changed = sqlx::query(
            "UPDATE work_items
             SET status = 'failed', error = ?, completed_at = ?, updated_at = ?,
                 heartbeat_at = ?, lease_owner = NULL, lease_expires_at = NULL
             WHERE id = ? AND status != 'failed'",
        )
        .bind(error)
        .bind(&now)
        .bind(&now)
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
        if changed == 1 {
            Self::event(
                pool,
                id,
                run_id,
                "failed",
                serde_json::json!({ "error": preview_text(error, 500) }),
            )
            .await?;
        }
        Ok(changed == 1)
    }

    pub async fn mark_skipped(
        pool: &SqlitePool,
        id: &str,
        reason: &str,
    ) -> Result<bool, sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let changed = sqlx::query(
            "UPDATE work_items
             SET status = 'skipped', error = ?, completed_at = ?, updated_at = ?,
                 lease_owner = NULL, lease_expires_at = NULL
             WHERE id = ? AND status NOT IN ('done','skipped')",
        )
        .bind(reason)
        .bind(&now)
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
        if changed == 1 {
            Self::event(
                pool,
                id,
                None,
                "skipped",
                serde_json::json!({ "reason": preview_text(reason, 500) }),
            )
            .await?;
        }
        Ok(changed == 1)
    }

    /// Requeue any non-pending item. Used by graph retry and rework, where the
    /// scheduler has already decided the task must run again.
    pub async fn mark_pending(
        pool: &SqlitePool,
        id: &str,
        reason: &str,
    ) -> Result<bool, sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let changed = sqlx::query(
            "UPDATE work_items
             SET status = 'pending', run_id = NULL, error = NULL, completed_at = NULL,
                 updated_at = ?, lease_owner = NULL, lease_expires_at = NULL
             WHERE id = ? AND status != 'pending'",
        )
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
        if changed == 1 {
            Self::event(
                pool,
                id,
                None,
                "requeued",
                serde_json::json!({ "reason": reason }),
            )
            .await?;
        }
        Ok(changed == 1)
    }

    /// Requeue an item for a deliberate graph rerun. Unlike a scheduler
    /// reschedule, a rework is a fresh start: the attempt counter resets so the
    /// rerun gets the task's full retry budget.
    pub async fn reset_for_rerun(
        pool: &SqlitePool,
        id: &str,
        reason: &str,
    ) -> Result<bool, sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let changed = sqlx::query(
            "UPDATE work_items
             SET status = 'pending', run_id = NULL, attempts = 0, error = NULL,
                 started_at = NULL, completed_at = NULL, updated_at = ?,
                 lease_owner = NULL, lease_expires_at = NULL
             WHERE id = ?",
        )
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
        if changed == 1 {
            Self::event(
                pool,
                id,
                None,
                "requeued",
                serde_json::json!({ "reason": reason, "fresh_attempts": true }),
            )
            .await?;
        }
        Ok(changed == 1)
    }

    /// Mark a task blocked by a person. This is a coordination state, not a
    /// terminal one: unblocking returns it to pending and can rerun its graph.
    pub async fn mark_blocked(
        pool: &SqlitePool,
        id: &str,
        reason: &str,
    ) -> Result<bool, sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let changed = sqlx::query(
            "UPDATE work_items
             SET status = 'blocked', error = ?, completed_at = ?, updated_at = ?,
                 lease_owner = NULL, lease_expires_at = NULL
             WHERE id = ? AND status NOT IN ('done','blocked')",
        )
        .bind(reason)
        .bind(&now)
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
        if changed == 1 {
            Self::event(
                pool,
                id,
                None,
                "blocked",
                serde_json::json!({ "reason": preview_text(reason, 500) }),
            )
            .await?;
        }
        Ok(changed == 1)
    }

    /// Put completed work into human review. The result stays readable; the
    /// status is what gates approval.
    pub async fn mark_review(
        pool: &SqlitePool,
        id: &str,
        reviewer: &str,
    ) -> Result<bool, sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let changed = sqlx::query(
            "UPDATE work_items
             SET status = 'review', reviewer_bot_id = ?, updated_at = ?
             WHERE id = ? AND status IN ('done','review')",
        )
        .bind(reviewer)
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
        if changed == 1 {
            Self::event(
                pool,
                id,
                None,
                "review_requested",
                serde_json::json!({ "reviewer": reviewer }),
            )
            .await?;
        }
        Ok(changed == 1)
    }

    /// Approve reviewed work. A comment, when present, is recorded before the
    /// transition so the approval and its reason are one trail.
    pub async fn approve_review(
        pool: &SqlitePool,
        id: &str,
        reviewer: &str,
        comment: Option<&str>,
    ) -> Result<bool, sqlx::Error> {
        if let Some(comment) = comment.map(str::trim).filter(|body| !body.is_empty()) {
            Self::add_comment(pool, id, reviewer, comment).await?;
        }
        let now = Utc::now().to_rfc3339();
        let changed = sqlx::query(
            "UPDATE work_items
             SET status = 'done', reviewer_bot_id = ?, completed_at = ?, updated_at = ?
             WHERE id = ? AND status = 'review'",
        )
        .bind(reviewer)
        .bind(&now)
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
        if changed == 1 {
            Self::event(
                pool,
                id,
                None,
                "review_approved",
                serde_json::json!({ "reviewer": reviewer }),
            )
            .await?;
        }
        Ok(changed == 1)
    }

    pub async fn add_comment(
        pool: &SqlitePool,
        work_item_id: &str,
        author: &str,
        body: &str,
    ) -> Result<WorkComment, sqlx::Error> {
        let comment = WorkComment {
            id: Uuid::new_v4().to_string(),
            work_item_id: work_item_id.to_string(),
            author: author.to_string(),
            body: body.to_string(),
            created_at: Utc::now().to_rfc3339(),
        };
        sqlx::query(
            "INSERT INTO work_comments (id, work_item_id, author, body, created_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&comment.id)
        .bind(&comment.work_item_id)
        .bind(&comment.author)
        .bind(&comment.body)
        .bind(&comment.created_at)
        .execute(pool)
        .await?;
        Self::event(
            pool,
            work_item_id,
            None,
            "comment",
            serde_json::json!({ "author": author, "body": preview_text(body, 500) }),
        )
        .await?;
        Ok(comment)
    }

    pub async fn list_comments(
        pool: &SqlitePool,
        work_item_id: &str,
    ) -> Result<Vec<WorkComment>, sqlx::Error> {
        let rows: Vec<(String, String, String, String, String)> = sqlx::query_as(
            "SELECT id, work_item_id, author, body, created_at
             FROM work_comments WHERE work_item_id = ? ORDER BY created_at ASC, id ASC",
        )
        .bind(work_item_id)
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| WorkComment {
                id: row.0,
                work_item_id: row.1,
                author: row.2,
                body: row.3,
                created_at: row.4,
            })
            .collect())
    }

    pub async fn add_artifact(
        pool: &SqlitePool,
        work_item_id: &str,
        path: &str,
        sha256: Option<&str>,
        mime: Option<&str>,
        size: Option<i64>,
    ) -> Result<WorkArtifact, sqlx::Error> {
        let artifact = WorkArtifact {
            id: Uuid::new_v4().to_string(),
            work_item_id: work_item_id.to_string(),
            path: path.to_string(),
            sha256: sha256.map(str::to_string),
            mime: mime.map(str::to_string),
            size,
            created_at: Utc::now().to_rfc3339(),
        };
        sqlx::query(
            "INSERT INTO work_artifacts
                (id, work_item_id, path, sha256, mime, size, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&artifact.id)
        .bind(&artifact.work_item_id)
        .bind(&artifact.path)
        .bind(&artifact.sha256)
        .bind(&artifact.mime)
        .bind(artifact.size)
        .bind(&artifact.created_at)
        .execute(pool)
        .await?;
        Self::event(
            pool,
            work_item_id,
            None,
            "artifact",
            serde_json::json!({ "path": path, "sha256": sha256, "size": size }),
        )
        .await?;
        Ok(artifact)
    }

    /// Attach one artifact path if it is not already attached. Writes a single
    /// event only on the insert, so repeated graph reconciliation is quiet.
    pub async fn add_artifact_once(
        pool: &SqlitePool,
        work_item_id: &str,
        path: &str,
    ) -> Result<bool, sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let changed = sqlx::query(
            "INSERT INTO work_artifacts (id, work_item_id, path, created_at)
             SELECT ?, ?, ?, ?
             WHERE NOT EXISTS (
                 SELECT 1 FROM work_artifacts WHERE work_item_id = ? AND path = ?
             )",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(work_item_id)
        .bind(path)
        .bind(&now)
        .bind(work_item_id)
        .bind(path)
        .execute(pool)
        .await?
        .rows_affected();
        if changed == 1 {
            Self::event(
                pool,
                work_item_id,
                None,
                "artifact",
                serde_json::json!({ "path": path }),
            )
            .await?;
        }
        Ok(changed == 1)
    }

    pub async fn list_artifacts(
        pool: &SqlitePool,
        work_item_id: &str,
    ) -> Result<Vec<WorkArtifact>, sqlx::Error> {
        let rows: Vec<(
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<i64>,
            String,
        )> = sqlx::query_as(
            "SELECT id, work_item_id, path, sha256, mime, size, created_at
                 FROM work_artifacts WHERE work_item_id = ? ORDER BY created_at ASC, id ASC",
        )
        .bind(work_item_id)
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| WorkArtifact {
                id: row.0,
                work_item_id: row.1,
                path: row.2,
                sha256: row.3,
                mime: row.4,
                size: row.5,
                created_at: row.6,
            })
            .collect())
    }

    pub async fn events(
        pool: &SqlitePool,
        work_item_id: &str,
        limit: u32,
    ) -> Result<Vec<WorkEvent>, sqlx::Error> {
        let rows: Vec<(
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            String,
            String,
            String,
        )> = sqlx::query_as(
            "SELECT id, work_item_id, run_id, thread_id, kind, payload_json, created_at
                 FROM work_events WHERE work_item_id = ?
                 ORDER BY created_at ASC LIMIT ?",
        )
        .bind(work_item_id)
        .bind(i64::from(limit.clamp(1, 1000)))
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| WorkEvent {
                id: row.0,
                work_item_id: row.1,
                run_id: row.2,
                thread_id: row.3,
                kind: row.4,
                payload: serde_json::from_str(&row.5).unwrap_or(serde_json::Value::Null),
                created_at: row.6,
            })
            .collect())
    }

    pub async fn detail(
        pool: &SqlitePool,
        work_item_id: &str,
    ) -> Result<Option<WorkItemDetail>, sqlx::Error> {
        let Some(item) = Self::get(pool, work_item_id).await? else {
            return Ok(None);
        };
        Ok(Some(WorkItemDetail {
            comments: Self::list_comments(pool, work_item_id).await?,
            artifacts: Self::list_artifacts(pool, work_item_id).await?,
            events: Self::events(pool, work_item_id, 500).await?,
            item,
        }))
    }

    async fn event(
        pool: &SqlitePool,
        work_item_id: &str,
        run_id: Option<Uuid>,
        kind: &str,
        payload: serde_json::Value,
    ) -> Result<(), sqlx::Error> {
        RunProgressQueries::record_for_work_item(
            pool,
            Some(work_item_id),
            run_id,
            None,
            kind,
            payload,
        )
        .await?;
        Ok(())
    }
}

/// One persisted quality-gate execution.
#[derive(Debug, Clone, Serialize)]
pub struct GateRunRecord {
    pub id: String,
    pub office_id: String,
    pub thread_id: Option<String>,
    pub cycle: i64,
    pub command: String,
    pub cwd: String,
    pub exit_code: Option<i64>,
    pub passed: bool,
    pub timed_out: bool,
    pub duration_ms: i64,
    pub output_tail: String,
    pub created_at: String,
    /// Contract that required the gate, when one exists.
    pub goal_id: Option<String>,
}

type GateRunRow = (
    String,
    String,
    Option<String>,
    i64,
    String,
    String,
    Option<i64>,
    bool,
    bool,
    i64,
    String,
    String,
    Option<String>,
);

fn gate_row(row: GateRunRow) -> GateRunRecord {
    GateRunRecord {
        id: row.0,
        office_id: row.1,
        thread_id: row.2,
        cycle: row.3,
        command: row.4,
        cwd: row.5,
        exit_code: row.6,
        passed: row.7,
        timed_out: row.8,
        duration_ms: row.9,
        output_tail: row.10,
        created_at: row.11,
        goal_id: row.12,
    }
}

pub struct QualityGateQueries;

impl QualityGateQueries {
    /// Persist one gate result. A failed command is data, not an error.
    #[allow(clippy::too_many_arguments)]
    pub async fn record(
        pool: &SqlitePool,
        office_id: Uuid,
        thread_id: Option<Uuid>,
        cycle: u32,
        command: &str,
        cwd: &str,
        exit_code: Option<i32>,
        passed: bool,
        timed_out: bool,
        duration_ms: u64,
        output_tail: &str,
        goal_id: Option<Uuid>,
    ) -> Result<Uuid, sqlx::Error> {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO office_gate_runs
                (id, office_id, thread_id, cycle, command, cwd, exit_code, passed,
                 timed_out, duration_ms, output_tail, created_at, goal_id)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(office_id.to_string())
        .bind(thread_id.map(|id| id.to_string()))
        .bind(i64::from(cycle))
        .bind(command)
        .bind(cwd)
        .bind(exit_code.map(i64::from))
        .bind(i64::from(passed))
        .bind(i64::from(timed_out))
        .bind(duration_ms.min(i64::MAX as u64) as i64)
        .bind(output_tail)
        .bind(Utc::now().to_rfc3339())
        .bind(goal_id.map(|id| id.to_string()))
        .execute(pool)
        .await?;
        Ok(id)
    }

    pub async fn list_for_office(
        pool: &SqlitePool,
        office_id: Uuid,
        limit: u32,
    ) -> Result<Vec<GateRunRecord>, sqlx::Error> {
        let rows: Vec<GateRunRow> = sqlx::query_as(
            "SELECT id, office_id, thread_id, cycle, command, cwd, exit_code, passed,
                    timed_out, duration_ms, output_tail, created_at, goal_id
             FROM office_gate_runs WHERE office_id = ?
             ORDER BY created_at DESC LIMIT ?",
        )
        .bind(office_id.to_string())
        .bind(i64::from(limit.clamp(1, 500)))
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(gate_row).collect())
    }

    pub async fn list_for_goal(
        pool: &SqlitePool,
        goal_id: Uuid,
        limit: u32,
    ) -> Result<Vec<GateRunRecord>, sqlx::Error> {
        let rows: Vec<GateRunRow> = sqlx::query_as(
            "SELECT id, office_id, thread_id, cycle, command, cwd, exit_code, passed,
                    timed_out, duration_ms, output_tail, created_at, goal_id
             FROM office_gate_runs WHERE goal_id = ?
             ORDER BY created_at DESC LIMIT ?",
        )
        .bind(goal_id.to_string())
        .bind(i64::from(limit.clamp(1, 500)))
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(gate_row).collect())
    }
}

/// One persisted completion contract.
#[derive(Debug, Clone, Serialize)]
pub struct GoalRecord {
    pub id: String,
    pub office_id: Option<String>,
    pub thread_id: Option<String>,
    pub objective: String,
    pub contract: ravenbot_core::GoalContract,
    pub status: String,
    pub turns_used: i64,
    pub tokens_used: i64,
    pub last_verdict: Option<String>,
    pub last_reason: String,
    /// When the autopilot should start the next reviewed cycle. `None` means
    /// the goal is done, paused, blocked, or otherwise not scheduled.
    pub next_run_at: Option<String>,
    pub last_run_at: Option<String>,
    pub rounds_used: i64,
    pub max_rounds: i64,
    pub consecutive_failures: i64,
    pub blocked_rounds: i64,
    pub last_blocker: String,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GoalDetail {
    pub goal: GoalRecord,
    pub gates: Vec<ravenbot_core::GoalGate>,
    pub gate_runs: Vec<GateRunRecord>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct GoalRow {
    id: String,
    office_id: Option<String>,
    thread_id: Option<String>,
    objective: String,
    contract_json: String,
    status: String,
    turns_used: i64,
    tokens_used: i64,
    last_verdict: Option<String>,
    last_reason: String,
    next_run_at: Option<String>,
    last_run_at: Option<String>,
    rounds_used: i64,
    max_rounds: i64,
    consecutive_failures: i64,
    blocked_rounds: i64,
    last_blocker: String,
    created_at: String,
    updated_at: String,
    completed_at: Option<String>,
}

impl From<GoalRow> for GoalRecord {
    fn from(row: GoalRow) -> Self {
        let mut contract: ravenbot_core::GoalContract =
            serde_json::from_str(&row.contract_json).unwrap_or_default();
        if contract.objective.trim().is_empty() {
            contract.objective = row.objective.clone();
        }
        Self {
            id: row.id,
            office_id: row.office_id,
            thread_id: row.thread_id,
            objective: row.objective,
            contract,
            status: row.status,
            turns_used: row.turns_used,
            tokens_used: row.tokens_used,
            last_verdict: row.last_verdict,
            last_reason: row.last_reason,
            next_run_at: row.next_run_at,
            last_run_at: row.last_run_at,
            rounds_used: row.rounds_used,
            max_rounds: row.max_rounds,
            consecutive_failures: row.consecutive_failures,
            blocked_rounds: row.blocked_rounds,
            last_blocker: row.last_blocker,
            created_at: row.created_at,
            updated_at: row.updated_at,
            completed_at: row.completed_at,
        }
    }
}

/// Backoff after failed automatic cycles. A clean cycle gets a short pause so
/// the UI can settle; repeated failures back off to fifteen minutes.
fn goal_retry_delay_seconds(consecutive_failures: i64) -> i64 {
    match consecutive_failures {
        0 => 15,
        1 => 60,
        2 => 300,
        _ => 900,
    }
}

pub struct GoalQueries;

impl GoalQueries {
    /// Create a goal and its gates.
    pub async fn create(
        pool: &SqlitePool,
        office_id: Option<Uuid>,
        thread_id: Option<Uuid>,
        contract: &ravenbot_core::GoalContract,
    ) -> Result<GoalRecord, sqlx::Error> {
        Self::create_with_rounds(pool, office_id, thread_id, contract, 24).await
    }

    /// Create a goal with an explicit automatic-round budget.
    ///
    /// Office goals keep the historical 24-round default; a 1:1 thread goal
    /// exposes the budget to the user and may set a smaller or larger one.
    pub async fn create_with_rounds(
        pool: &SqlitePool,
        office_id: Option<Uuid>,
        thread_id: Option<Uuid>,
        contract: &ravenbot_core::GoalContract,
        max_rounds: u32,
    ) -> Result<GoalRecord, sqlx::Error> {
        let id = Uuid::new_v4();
        let now = Utc::now().to_rfc3339();
        // Five seconds lets the current turn finish starting before a winit
        // can schedule the next automatic cycle.
        let next_run_at = (Utc::now() + chrono::Duration::seconds(5)).to_rfc3339();
        let mut contract = contract.clone();
        contract.normalize();
        let max_rounds = i64::from(max_rounds.clamp(1, 64));
        sqlx::query(
            "INSERT INTO goals
                (id, office_id, thread_id, objective, contract_json, status,
                 next_run_at, max_rounds, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, 'active', ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(office_id.map(|value| value.to_string()))
        .bind(thread_id.map(|value| value.to_string()))
        .bind(&contract.objective)
        .bind(serde_json::to_string(&contract).unwrap_or_default())
        .bind(&next_run_at)
        .bind(max_rounds)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await?;
        Self::replace_gates(pool, id, &contract.gates).await?;
        Self::get(pool, id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    /// Create the office's contract only when it does not already have one.
    pub async fn ensure_office_goal(
        pool: &SqlitePool,
        office_id: Uuid,
        objective: &str,
        max_turns: u32,
    ) -> Result<GoalRecord, sqlx::Error> {
        if let Some(existing) = Self::get_by_office(pool, office_id).await? {
            if existing.status == "active" && existing.next_run_at.is_none() {
                if let Ok(id) = Uuid::parse_str(&existing.id) {
                    Self::arm_now(pool, id).await?;
                }
                return Self::get_by_office(pool, office_id)
                    .await?
                    .ok_or(sqlx::Error::RowNotFound);
            }
            return Ok(existing);
        }
        let mut contract = ravenbot_core::GoalContract::for_objective(objective);
        contract.max_turns = max_turns.max(1);
        Self::create(pool, Some(office_id), None, &contract).await
    }

    pub async fn get(pool: &SqlitePool, id: Uuid) -> Result<Option<GoalRecord>, sqlx::Error> {
        let row: Option<GoalRow> = sqlx::query_as("SELECT * FROM goals WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(pool)
            .await?;
        Ok(row.map(GoalRecord::from))
    }

    pub async fn get_by_office(
        pool: &SqlitePool,
        office_id: Uuid,
    ) -> Result<Option<GoalRecord>, sqlx::Error> {
        let row: Option<GoalRow> = sqlx::query_as("SELECT * FROM goals WHERE office_id = ?")
            .bind(office_id.to_string())
            .fetch_optional(pool)
            .await?;
        Ok(row.map(GoalRecord::from))
    }

    /// The completion contract attached to a 1:1 agent thread.
    pub async fn get_by_thread(
        pool: &SqlitePool,
        thread_id: Uuid,
    ) -> Result<Option<GoalRecord>, sqlx::Error> {
        let row: Option<GoalRow> = sqlx::query_as(
            "SELECT * FROM goals WHERE thread_id = ? AND office_id IS NULL
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(thread_id.to_string())
        .fetch_optional(pool)
        .await?;
        Ok(row.map(GoalRecord::from))
    }

    /// Create the thread's goal only when it does not already have one.
    pub async fn ensure_thread_goal(
        pool: &SqlitePool,
        thread_id: Uuid,
        objective: &str,
        max_rounds: u32,
    ) -> Result<GoalRecord, sqlx::Error> {
        if let Some(existing) = Self::get_by_thread(pool, thread_id).await? {
            if existing.status == "active" && existing.next_run_at.is_none() {
                if let Ok(id) = Uuid::parse_str(&existing.id) {
                    Self::arm_now(pool, id).await?;
                }
                return Self::get_by_thread(pool, thread_id)
                    .await?
                    .ok_or(sqlx::Error::RowNotFound);
            }
            return Ok(existing);
        }
        let mut contract = ravenbot_core::GoalContract::for_objective(objective);
        contract.max_turns = max_rounds.clamp(1, 64);
        Self::create_with_rounds(pool, None, Some(thread_id), &contract, max_rounds).await
    }

    /// Replace the editable contract fields without resetting its history.
    pub async fn update_contract(
        pool: &SqlitePool,
        id: Uuid,
        contract: &ravenbot_core::GoalContract,
    ) -> Result<Option<GoalRecord>, sqlx::Error> {
        let mut contract = contract.clone();
        contract.normalize();
        let changed = sqlx::query(
            "UPDATE goals SET objective = ?, contract_json = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(&contract.objective)
        .bind(serde_json::to_string(&contract).unwrap_or_default())
        .bind(Utc::now().to_rfc3339())
        .bind(id.to_string())
        .execute(pool)
        .await?
        .rows_affected();
        if changed == 0 {
            return Ok(None);
        }
        Self::replace_gates(pool, id, &contract.gates).await?;
        Self::get(pool, id).await
    }

    /// Change a goal's token/round budget without discarding its history.
    ///
    /// Used when a paused goal is resumed: the user can raise the ceiling the
    /// goal hit, or lower a budget before a long unattended stretch. A token
    /// budget of 0 means unlimited.
    pub async fn set_budget(
        pool: &SqlitePool,
        id: Uuid,
        max_tokens: u64,
        max_rounds: Option<u32>,
    ) -> Result<Option<GoalRecord>, sqlx::Error> {
        let Some(current) = Self::get(pool, id).await? else {
            return Ok(None);
        };
        let mut contract = current.contract;
        contract.max_tokens = max_tokens.min(100_000_000);
        let rounds = max_rounds
            .map(|rounds| rounds.clamp(1, 64))
            .unwrap_or_else(|| current.max_rounds.clamp(1, 64) as u32);
        contract.max_turns = rounds;
        contract.normalize();
        sqlx::query(
            "UPDATE goals SET contract_json = ?, max_rounds = ?, updated_at = ? WHERE id = ?",
        )
        .bind(serde_json::to_string(&contract).unwrap_or_default())
        .bind(i64::from(rounds))
        .bind(Utc::now().to_rfc3339())
        .bind(id.to_string())
        .execute(pool)
        .await?;
        Self::get(pool, id).await
    }

    /// Arm an active goal for its next automatic continuation.
    pub async fn arm_now(pool: &SqlitePool, id: Uuid) -> Result<bool, sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let changed = sqlx::query(
            "UPDATE goals SET next_run_at = ?, updated_at = ?
             WHERE id = ? AND status = 'active'",
        )
        .bind(&now)
        .bind(&now)
        .bind(id.to_string())
        .execute(pool)
        .await?
        .rows_affected();
        Ok(changed == 1)
    }

    /// Postpone an active goal after a failed start or a persistent blocker.
    /// Repeated failures back off further instead of retrying on every tick.
    pub async fn postpone(
        pool: &SqlitePool,
        id: Uuid,
        seconds: i64,
        reason: &str,
    ) -> Result<bool, sqlx::Error> {
        let now = Utc::now();
        let next = (now + chrono::Duration::seconds(seconds.clamp(1, 86_400))).to_rfc3339();
        let changed = sqlx::query(
            "UPDATE goals
             SET next_run_at = ?, consecutive_failures = consecutive_failures + 1,
                 last_reason = ?, updated_at = ?
             WHERE id = ? AND status = 'active'",
        )
        .bind(&next)
        .bind(reason)
        .bind(now.to_rfc3339())
        .bind(id.to_string())
        .execute(pool)
        .await?
        .rows_affected();
        Ok(changed == 1)
    }

    /// Park a goal with a durable reason (budget, no progress, or a hard
    /// blocker) so the autopilot cannot immediately pick it up again.
    pub async fn pause_with_reason(
        pool: &SqlitePool,
        id: Uuid,
        reason: &str,
    ) -> Result<bool, sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let changed = sqlx::query(
            "UPDATE goals
             SET status = 'paused', next_run_at = NULL, last_reason = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(reason)
        .bind(&now)
        .bind(id.to_string())
        .execute(pool)
        .await?
        .rows_affected();
        Ok(changed == 1)
    }

    /// Active goals whose next automatic cycle is due.
    pub async fn due_for_autopilot(
        pool: &SqlitePool,
        now: &str,
        limit: u32,
    ) -> Result<Vec<GoalRecord>, sqlx::Error> {
        let rows: Vec<GoalRow> = sqlx::query_as(
            "SELECT * FROM goals
             WHERE status = 'active' AND next_run_at IS NOT NULL AND next_run_at <= ?
             ORDER BY next_run_at ASC, updated_at ASC
             LIMIT ?",
        )
        .bind(now)
        .bind(i64::from(limit.clamp(1, 100)))
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(GoalRecord::from).collect())
    }

    /// Due 1:1 thread goals, filtered in SQL so a busy office queue cannot
    /// starve them out of a shared page of results.
    pub async fn due_thread_for_autopilot(
        pool: &SqlitePool,
        now: &str,
        limit: u32,
    ) -> Result<Vec<GoalRecord>, sqlx::Error> {
        let rows: Vec<GoalRow> = sqlx::query_as(
            "SELECT * FROM goals
             WHERE status = 'active' AND next_run_at IS NOT NULL AND next_run_at <= ?
               AND office_id IS NULL AND thread_id IS NOT NULL
               AND rounds_used < max_rounds
             ORDER BY next_run_at ASC, updated_at ASC
             LIMIT ?",
        )
        .bind(now)
        .bind(i64::from(limit.clamp(1, 100)))
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(GoalRecord::from).collect())
    }

    pub async fn set_status(
        pool: &SqlitePool,
        id: Uuid,
        status: ravenbot_core::GoalStatus,
    ) -> Result<bool, sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let completed = matches!(status, ravenbot_core::GoalStatus::Done);
        let active = matches!(status, ravenbot_core::GoalStatus::Active);
        let next_run_at = active.then_some(now.clone());
        let changed = sqlx::query(
            "UPDATE goals
             SET status = ?, next_run_at = ?, updated_at = ?,
                 rounds_used = CASE WHEN ? THEN 0 ELSE rounds_used END,
                 consecutive_failures = CASE WHEN ? THEN 0 ELSE consecutive_failures END,
                 blocked_rounds = CASE WHEN ? THEN 0 ELSE blocked_rounds END,
                 last_blocker = CASE WHEN ? THEN '' ELSE last_blocker END,
                 completed_at = CASE WHEN ? THEN ? WHEN ? THEN NULL ELSE completed_at END
             WHERE id = ?",
        )
        .bind(status.as_str())
        .bind(&next_run_at)
        .bind(&now)
        .bind(i64::from(active))
        .bind(i64::from(active))
        .bind(i64::from(active))
        .bind(i64::from(active))
        .bind(i64::from(completed))
        .bind(&now)
        .bind(i64::from(active))
        .bind(id.to_string())
        .execute(pool)
        .await?
        .rows_affected();
        Ok(changed == 1)
    }

    /// Reopen a terminal goal for a new mission. Usage history is kept; the
    /// contract budget is edited separately if the new mission needs more room.
    pub async fn reopen(pool: &SqlitePool, id: Uuid) -> Result<bool, sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let changed = sqlx::query(
            "UPDATE goals
             SET status = 'active', next_run_at = ?, completed_at = NULL,
                 last_verdict = NULL, last_reason = '',
                 rounds_used = 0, consecutive_failures = 0, blocked_rounds = 0,
                 last_blocker = '', updated_at = ?
             WHERE id = ?",
        )
        .bind(&now)
        .bind(&now)
        .bind(id.to_string())
        .execute(pool)
        .await?
        .rows_affected();
        Ok(changed == 1)
    }

    /// Record one reviewed cycle and schedule the next automatic one.
    ///
    /// A done verdict is terminal. A blocked verdict is persisted for three
    /// consecutive cycles before the goal is parked; until then it stays
    /// active and retries after a backoff. `wait` is also a scheduled retry,
    /// not a terminal state. A continue verdict is capped by `max_rounds` and
    /// backed off after failed cycles.
    pub async fn record_verdict(
        pool: &SqlitePool,
        id: Uuid,
        verdict: ravenbot_core::GoalVerdict,
        reason: &str,
        tokens_delta: u64,
        failed: bool,
    ) -> Result<Option<GoalRecord>, sqlx::Error> {
        let Some(current) = Self::get(pool, id).await? else {
            return Ok(None);
        };
        let failures = if failed {
            current.consecutive_failures.saturating_add(1)
        } else {
            0
        };
        let blocked_rounds = if verdict == ravenbot_core::GoalVerdict::Blocked {
            current.blocked_rounds.saturating_add(1)
        } else {
            0
        };
        let rounds = current.rounds_used.saturating_add(1);
        let max_rounds = current.max_rounds.max(1);
        let rounds_exhausted = rounds >= max_rounds;
        let max_blocked_rounds = 3;

        let terminal = matches!(
            current.status.as_str(),
            "done" | "cancelled" | "failed" | "blocked" | "paused"
        );
        // A cancelled goal is closed for every verdict, not only for a late
        // continue: a round that finishes after the user stopped it must not
        // revive the goal by claiming done.
        let cancelled = current.status == "cancelled";
        let (status, retry_after_secs) = if cancelled
            || (terminal
                && matches!(
                    verdict,
                    ravenbot_core::GoalVerdict::Continue | ravenbot_core::GoalVerdict::Wait
                )) {
            // A late review must not resurrect a terminal or parked goal.
            (current.status.as_str(), None)
        } else {
            match verdict {
                ravenbot_core::GoalVerdict::Done => ("done", None),
                ravenbot_core::GoalVerdict::Blocked if blocked_rounds >= max_blocked_rounds => {
                    ("blocked", None)
                }
                ravenbot_core::GoalVerdict::Blocked => ("active", Some(300)),
                ravenbot_core::GoalVerdict::Wait => ("active", Some(120)),
                ravenbot_core::GoalVerdict::Continue if rounds_exhausted => ("paused", None),
                ravenbot_core::GoalVerdict::Continue => {
                    ("active", Some(goal_retry_delay_seconds(failures)))
                }
            }
        };
        let now = Utc::now();
        let next_run_at =
            retry_after_secs.map(|seconds| (now + chrono::Duration::seconds(seconds)).to_rfc3339());
        let now_text = now.to_rfc3339();
        let completed_at = (status == "done").then_some(now_text.clone());
        let last_reason = if verdict == ravenbot_core::GoalVerdict::Continue && rounds_exhausted {
            format!(
                "{} Auto-continuation budget exhausted ({} of {} rounds).",
                reason.trim(),
                rounds,
                max_rounds
            )
        } else {
            reason.trim().to_string()
        };
        let last_blocker = if verdict == ravenbot_core::GoalVerdict::Blocked {
            reason.trim().to_string()
        } else {
            String::new()
        };

        sqlx::query(
            "UPDATE goals
             SET turns_used = turns_used + 1, rounds_used = ?,
                 tokens_used = tokens_used + ?, consecutive_failures = ?,
                 blocked_rounds = ?, last_blocker = ?, last_verdict = ?,
                 last_reason = ?, status = ?, next_run_at = ?, last_run_at = ?,
                 updated_at = ?, completed_at = COALESCE(?, completed_at)
             WHERE id = ?",
        )
        .bind(rounds)
        .bind(tokens_delta.min(i64::MAX as u64) as i64)
        .bind(failures)
        .bind(blocked_rounds)
        .bind(&last_blocker)
        .bind(verdict.as_str())
        .bind(&last_reason)
        .bind(status)
        .bind(&next_run_at)
        .bind(&now_text)
        .bind(&now_text)
        .bind(&completed_at)
        .bind(id.to_string())
        .execute(pool)
        .await?;
        Self::get(pool, id).await
    }

    /// Total tokens spent by office-linked runs since a timestamp.
    ///
    /// Both the office's own conversation thread and every node thread are
    /// included, because an office turn spends across both.
    pub async fn tokens_for_office_since(
        pool: &SqlitePool,
        office_id: Uuid,
        since: &str,
    ) -> Result<u64, sqlx::Error> {
        let total: i64 = sqlx::query_scalar(
            "SELECT CAST(COALESCE(SUM(r.tokens_consumed), 0) AS INTEGER)
             FROM runs r
             WHERE r.created_at >= ?
               AND (
                   r.thread_id IN (
                       SELECT thread_id FROM chatroom_office_threads WHERE chatroom_id = ?
                   )
                   OR r.thread_id IN (
                       SELECT thread_id FROM chatroom_threads WHERE chatroom_id = ?
                   )
               )",
        )
        .bind(since)
        .bind(office_id.to_string())
        .bind(office_id.to_string())
        .fetch_one(pool)
        .await?;
        Ok(total.max(0) as u64)
    }

    /// Total tokens spent by runs in one 1:1 thread since a timestamp.
    pub async fn tokens_for_thread_since(
        pool: &SqlitePool,
        thread_id: Uuid,
        since: &str,
    ) -> Result<u64, sqlx::Error> {
        let total: i64 = sqlx::query_scalar(
            "SELECT CAST(COALESCE(SUM(tokens_consumed), 0) AS INTEGER)
             FROM runs WHERE thread_id = ? AND created_at >= ?",
        )
        .bind(thread_id.to_string())
        .bind(since)
        .fetch_one(pool)
        .await?;
        Ok(total.max(0) as u64)
    }

    /// Record the highest observed token total for a goal. Monotonic so a
    /// transient query failure cannot reset the budget downwards.
    pub async fn set_tokens_used(
        pool: &SqlitePool,
        id: Uuid,
        tokens: u64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE goals SET tokens_used = MAX(tokens_used, ?), updated_at = ?
             WHERE id = ?",
        )
        .bind(tokens.min(i64::MAX as u64) as i64)
        .bind(Utc::now().to_rfc3339())
        .bind(id.to_string())
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn replace_gates(
        pool: &SqlitePool,
        goal_id: Uuid,
        gates: &[ravenbot_core::GoalGate],
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let mut tx = pool.begin().await?;
        sqlx::query("DELETE FROM goal_gates WHERE goal_id = ?")
            .bind(goal_id.to_string())
            .execute(&mut *tx)
            .await?;
        for (position, gate) in gates.iter().take(4).enumerate() {
            sqlx::query(
                "INSERT INTO goal_gates
                    (id, goal_id, command, cwd, timeout_secs, required, position, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(gate.id.to_string())
            .bind(goal_id.to_string())
            .bind(gate.command.trim())
            .bind(gate.cwd.as_deref())
            .bind(gate.timeout_secs.clamp(1, 3_600) as i64)
            .bind(i64::from(gate.required))
            .bind(position as i64)
            .bind(&now)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn get_gates(
        pool: &SqlitePool,
        goal_id: Uuid,
    ) -> Result<Vec<ravenbot_core::GoalGate>, sqlx::Error> {
        let rows: Vec<(String, String, Option<String>, i64, bool, i64)> = sqlx::query_as(
            "SELECT id, command, cwd, timeout_secs, required, position
             FROM goal_gates WHERE goal_id = ? ORDER BY position ASC, created_at ASC",
        )
        .bind(goal_id.to_string())
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|(id, command, cwd, timeout_secs, required, position)| {
                Some(ravenbot_core::GoalGate {
                    id: Uuid::parse_str(&id).ok()?,
                    command,
                    cwd,
                    timeout_secs: timeout_secs.max(1) as u64,
                    required,
                    position: position.max(0) as u32,
                })
            })
            .collect())
    }

    pub async fn detail(
        pool: &SqlitePool,
        goal_id: Uuid,
    ) -> Result<Option<GoalDetail>, sqlx::Error> {
        let Some(goal) = Self::get(pool, goal_id).await? else {
            return Ok(None);
        };
        Ok(Some(GoalDetail {
            gates: Self::get_gates(pool, goal_id).await?,
            gate_runs: QualityGateQueries::list_for_goal(pool, goal_id, 50).await?,
            goal,
        }))
    }

    pub async fn office_detail(
        pool: &SqlitePool,
        office_id: Uuid,
    ) -> Result<Option<GoalDetail>, sqlx::Error> {
        let Some(goal) = Self::get_by_office(pool, office_id).await? else {
            return Ok(None);
        };
        Self::detail(pool, Uuid::parse_str(&goal.id).unwrap_or_default()).await
    }

    pub async fn thread_detail(
        pool: &SqlitePool,
        thread_id: Uuid,
    ) -> Result<Option<GoalDetail>, sqlx::Error> {
        let Some(goal) = Self::get_by_thread(pool, thread_id).await? else {
            return Ok(None);
        };
        Self::detail(pool, Uuid::parse_str(&goal.id).unwrap_or_default()).await
    }
}

fn preview_text(text: &str, max: usize) -> String {
    let mut preview: String = text.chars().take(max).collect();
    if text.chars().count() > max {
        preview.push('…');
    }
    preview
}

/// ChatRoom-related queries
pub struct ChatRoomQueries;

impl ChatRoomQueries {
    pub async fn create(
        pool: &SqlitePool,
        room: &ravenbot_core::ChatRoom,
    ) -> Result<(), sqlx::Error> {
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

    pub async fn update(
        pool: &SqlitePool,
        room: &ravenbot_core::ChatRoom,
    ) -> Result<(), sqlx::Error> {
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
        let rows: Vec<crate::models::ChatRoomRow> =
            sqlx::query_as("SELECT * FROM chatrooms ORDER BY updated_at DESC")
                .fetch_all(pool)
                .await?;
        Ok(rows
            .into_iter()
            .filter_map(|r| {
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
                    budget_distribution: r
                        .budget_distribution
                        .as_deref()
                        .and_then(|s| serde_json::from_str(s).ok()),
                    project_folders: r
                        .project_folders
                        .as_deref()
                        .and_then(|s| serde_json::from_str(s).ok())
                        .unwrap_or_default(),
                    created_at: chrono::DateTime::parse_from_rfc3339(&r.created_at)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now()),
                    updated_at: chrono::DateTime::parse_from_rfc3339(&r.updated_at)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now()),
                })
            })
            .collect())
    }

    pub async fn get(
        pool: &SqlitePool,
        id: Uuid,
    ) -> Result<Option<ravenbot_core::ChatRoom>, sqlx::Error> {
        let row: Option<crate::models::ChatRoomRow> =
            sqlx::query_as("SELECT * FROM chatrooms WHERE id = ?")
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
                budget_distribution: r
                    .budget_distribution
                    .as_deref()
                    .and_then(|s| serde_json::from_str(s).ok()),
                project_folders: r
                    .project_folders
                    .as_deref()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or_default(),
                created_at: chrono::DateTime::parse_from_rfc3339(&r.created_at)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now()),
                updated_at: chrono::DateTime::parse_from_rfc3339(&r.updated_at)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now()),
            })
        }))
    }

    pub async fn add_member(
        pool: &SqlitePool,
        member: &ravenbot_core::ChatRoomMember,
    ) -> Result<(), sqlx::Error> {
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

    pub async fn list_members(
        pool: &SqlitePool,
        chatroom_id: Uuid,
    ) -> Result<Vec<ravenbot_core::ChatRoomMember>, sqlx::Error> {
        let rows: Vec<crate::models::ChatRoomMemberRow> =
            sqlx::query_as("SELECT * FROM chatroom_members WHERE chatroom_id = ?")
                .bind(chatroom_id.to_string())
                .fetch_all(pool)
                .await?;
        Ok(rows
            .into_iter()
            .filter_map(|r| {
                Some(ravenbot_core::ChatRoomMember {
                    chatroom_id: uuid::Uuid::parse_str(&r.chatroom_id).ok()?,
                    bot_id: uuid::Uuid::parse_str(&r.bot_id).ok()?,
                    rank: r.rank,
                    specialty: r.specialty,
                    joined_at: chrono::DateTime::parse_from_rfc3339(&r.joined_at)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now()),
                })
            })
            .collect())
    }

    pub async fn remove_member(
        pool: &SqlitePool,
        chatroom_id: Uuid,
        bot_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM chatroom_members WHERE chatroom_id = ? AND bot_id = ?")
            .bind(chatroom_id.to_string())
            .bind(bot_id.to_string())
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn update_member(
        pool: &SqlitePool,
        chatroom_id: Uuid,
        bot_id: Uuid,
        rank: &str,
        specialty: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE chatroom_members SET rank = ?, specialty = ? WHERE chatroom_id = ? AND bot_id = ?")
            .bind(rank).bind(specialty).bind(chatroom_id.to_string()).bind(bot_id.to_string()).execute(pool).await?;
        Ok(())
    }

    pub async fn delete(pool: &SqlitePool, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM chatrooms WHERE id = ?")
            .bind(id.to_string())
            .execute(pool)
            .await?;
        Ok(())
    }
}

impl ThreadQueries {
    /// Rename a thread
    pub async fn rename(
        pool: &SqlitePool,
        thread_id: Uuid,
        title: &str,
    ) -> Result<(), sqlx::Error> {
        // A conversation name is a label, not a document. Trim and cap it so a
        // direct IPC call cannot put a paragraph in the history list.
        let title = title.trim();
        let title: String = title.chars().take(200).collect();
        if title.trim().is_empty() {
            return Err(sqlx::Error::Decode(
                "Title cannot be empty".to_string().into(),
            ));
        }
        sqlx::query("UPDATE threads SET title = ?, updated_at = ? WHERE id = ?")
            .bind(title)
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(thread_id.to_string())
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Delete a thread (messages cascade via FK; reactions are cleared first)
    pub async fn delete(pool: &SqlitePool, thread_id: Uuid) -> Result<(), sqlx::Error> {
        let mut tx = pool.begin().await?;
        sqlx::query(
            "DELETE FROM message_reactions
             WHERE message_id IN (SELECT id FROM messages WHERE thread_id = ?)",
        )
        .bind(thread_id.to_string())
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM threads WHERE id = ?")
            .bind(thread_id.to_string())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
}

/// One full-text hit. `snippet` carries FTS5's `>>>match<<<` markers so the
/// caller can highlight without re-searching the message body.
#[derive(Debug, Clone, Serialize)]
pub struct FtsHit {
    pub message_id: String,
    pub thread_id: String,
    pub thread_title: String,
    pub snippet: String,
    pub score: f64,
}

/// Cross-thread message search. The LIKE path remains for the UI's simple
/// substring lookup; `search_fts` is the agent-facing ranked search.
pub struct SearchQueries;

impl SearchQueries {
    /// Cross-thread hits with the thread title and owning agent. The extra
    /// fields are what let the picker open the exact conversation and message.
    async fn hits(
        pool: &SqlitePool,
        query: &str,
        limit: u32,
    ) -> Result<Vec<(ravenbot_core::Message, String, String)>, sqlx::Error> {
        let pattern = format!(
            "%{}%",
            query
                .replace('\\', r"\\")
                .replace('%', r"\%")
                .replace('_', r"\_")
        );
        let rows: Vec<(String, String, String, String, String, String, String)> = sqlx::query_as(
            r#"SELECT m.id, m.thread_id, m.role, m.content, m.created_at,
                      t.title, t.bot_id
               FROM messages m
               JOIN threads t ON t.id = m.thread_id
               WHERE m.content LIKE ? ESCAPE '\'
                 AND m.role IN ('user', 'assistant')
               ORDER BY m.created_at DESC
               LIMIT ?"#,
        )
        .bind(&pattern)
        .bind(limit)
        .fetch_all(pool)
        .await?;

        let mut results = Vec::new();
        for row in rows {
            let content: ravenbot_core::MessageContent =
                serde_json::from_str(&row.3).unwrap_or(ravenbot_core::MessageContent::Text {
                    text: String::new(),
                    sources: Vec::new(),
                    reasoning: None,
                    tools: Vec::new(),
                    spans: Vec::new(),
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
                reactions: Vec::new(),
                context_used_tokens: None,
                context_window_tokens: None,
                context_compacted: false,
                context_system_tokens: None,
                context_tool_tokens: None,
                context_memory_tokens: None,
                context_message_tokens: None,
                created_at: chrono::DateTime::parse_from_rfc3339(&row.4)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now()),
            };
            results.push((message, row.5, row.6));
        }
        Ok(results)
    }

    pub async fn messages(
        pool: &SqlitePool,
        query: &str,
        limit: u32,
    ) -> Result<Vec<(ravenbot_core::Message, String)>, sqlx::Error> {
        Ok(Self::hits(pool, query, limit)
            .await?
            .into_iter()
            .map(|(message, title, _)| (message, title))
            .collect())
    }

    pub async fn messages_with_bot(
        pool: &SqlitePool,
        query: &str,
        limit: u32,
    ) -> Result<Vec<(ravenbot_core::Message, String, String)>, sqlx::Error> {
        Self::hits(pool, query, limit).await
    }

    /// Ranked full-text search over every transcript.
    ///
    /// The FTS query is built from quoted terms, not passed raw: FTS5 treats
    /// `-`, `:`, and operators as syntax, and a pasted error string must not
    /// become a parse error. Quoting makes each word a phrase and the implicit
    /// space an AND.
    pub async fn search_fts(
        pool: &SqlitePool,
        query: &str,
        thread_id: Option<Uuid>,
        limit: u32,
    ) -> Result<Vec<FtsHit>, sqlx::Error> {
        let match_query = query
            .split_whitespace()
            .map(|term| term.trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '-'))
            .filter(|term| !term.is_empty())
            .map(|term| format!("\"{}\"", term.replace('"', "")))
            .collect::<Vec<_>>()
            .join(" ");
        if match_query.is_empty() {
            return Ok(Vec::new());
        }
        let limit = i64::from(limit.clamp(1, 50));
        let rows: Vec<(String, String, String, f64, String)> = if let Some(thread_id) = thread_id {
            sqlx::query_as(
                "SELECT messages_fts.message_id, messages_fts.thread_id,
                        snippet(messages_fts, 2, '>>>', '<<<', '…', 24),
                        bm25(messages_fts),
                        COALESCE(t.title, '')
                 FROM messages_fts
                 LEFT JOIN threads t ON t.id = messages_fts.thread_id
                 WHERE messages_fts MATCH ? AND messages_fts.thread_id = ?
                 ORDER BY bm25(messages_fts)
                 LIMIT ?",
            )
            .bind(&match_query)
            .bind(thread_id.to_string())
            .bind(limit)
            .fetch_all(pool)
            .await?
        } else {
            sqlx::query_as(
                "SELECT messages_fts.message_id, messages_fts.thread_id,
                        snippet(messages_fts, 2, '>>>', '<<<', '…', 24),
                        bm25(messages_fts),
                        COALESCE(t.title, '')
                 FROM messages_fts
                 LEFT JOIN threads t ON t.id = messages_fts.thread_id
                 WHERE messages_fts MATCH ?
                 ORDER BY bm25(messages_fts)
                 LIMIT ?",
            )
            .bind(&match_query)
            .bind(limit)
            .fetch_all(pool)
            .await?
        };
        Ok(rows
            .into_iter()
            .map(
                |(message_id, thread_id, snippet, score, thread_title)| FtsHit {
                    message_id,
                    thread_id,
                    thread_title,
                    snippet: snippet
                        .replace("\\n", " ")
                        .replace("\\t", " ")
                        .replace("\\\"", "\""),
                    score,
                },
            )
            .collect())
    }
}

/// Fleet usage grouped by agent. This is the same aggregate the Usage screen
/// shows, so the numbers come from the run ledger rather than a UI estimate.
pub struct UsageQueries;

impl UsageQueries {
    /// Model and tool spend across runs. Utility calls have no run and are
    /// reported by [`UtilityUsageQueries`].
    pub async fn spend_breakdown(pool: &SqlitePool, days: u32) -> Result<(f64, f64), sqlx::Error> {
        let modifier = (days > 0).then(|| format!("-{days} days"));
        if let Some(modifier) = modifier {
            let row: (f64, f64) = sqlx::query_as(
                "SELECT CAST(COALESCE(SUM(model_cost), 0) AS REAL),
                        CAST(COALESCE(SUM(tool_cost), 0) AS REAL)
                 FROM runs
                 WHERE datetime(created_at) >= datetime('now', ?)",
            )
            .bind(modifier)
            .fetch_one(pool)
            .await?;
            Ok(row)
        } else {
            sqlx::query_as(
                "SELECT CAST(COALESCE(SUM(model_cost), 0) AS REAL),
                        CAST(COALESCE(SUM(tool_cost), 0) AS REAL)
                 FROM runs",
            )
            .fetch_one(pool)
            .await
        }
    }

    /// Spend grouped by the office a run belongs to.
    ///
    /// A run inherits its office from the office's own conversation thread or
    /// from a delegated office thread. The scalar subqueries are ordered and
    /// limited so a thread linked to more than one office is counted once,
    /// rather than inflating both offices' totals. Runs outside any office are
    /// returned with a `None` office so the UI can show a Direct group and the
    /// office rows still reconcile with the fleet total.
    pub async fn office_summary(
        pool: &SqlitePool,
        days: u32,
    ) -> Result<
        Vec<(
            Option<String>,
            Option<String>,
            Option<f64>,
            i64,
            i64,
            f64,
            i64,
            i64,
        )>,
        sqlx::Error,
    > {
        let base = "WITH office_runs AS (
                SELECT r.id, r.tokens_consumed, r.cost_estimate, r.state,
                       COALESCE(
                           (SELECT ct.chatroom_id FROM chatroom_threads ct
                            WHERE ct.thread_id = r.thread_id
                            ORDER BY ct.created_at ASC LIMIT 1),
                           (SELECT cot.chatroom_id FROM chatroom_office_threads cot
                            WHERE cot.thread_id = r.thread_id
                            ORDER BY cot.created_at ASC, cot.chatroom_id ASC LIMIT 1)
                       ) AS office_id
                FROM runs r";
        let tail = ")
            SELECT o.office_id, c.name, c.budget, COUNT(*),
                   CAST(COALESCE(SUM(o.tokens_consumed), 0) AS INTEGER),
                   CAST(COALESCE(SUM(o.cost_estimate), 0) AS REAL),
                   CAST(SUM(CASE WHEN o.state = 'failed' THEN 1 ELSE 0 END) AS INTEGER),
                   CAST(SUM(CASE WHEN o.state = 'completed' THEN 1 ELSE 0 END) AS INTEGER)
            FROM office_runs o
            LEFT JOIN chatrooms c ON c.id = o.office_id
            GROUP BY o.office_id, c.name";
        if days > 0 {
            let sql = format!("{base} WHERE datetime(r.created_at) >= datetime('now', ?){tail}");
            sqlx::query_as(sqlx::AssertSqlSafe(sql))
                .bind(format!("-{days} days"))
                .fetch_all(pool)
                .await
        } else {
            let sql = format!("{base}{tail}");
            sqlx::query_as(sqlx::AssertSqlSafe(sql))
                .fetch_all(pool)
                .await
        }
    }

    pub async fn summary(
        pool: &SqlitePool,
        days: u32,
    ) -> Result<Vec<(String, i64, i64, f64, i64, i64)>, sqlx::Error> {
        let modifier = (days > 0).then(|| format!("-{days} days"));
        if let Some(modifier) = modifier {
            sqlx::query_as(
                "SELECT bot_id, COUNT(*), CAST(COALESCE(SUM(tokens_consumed), 0) AS INTEGER),
                        CAST(COALESCE(SUM(cost_estimate), 0) AS REAL),
                        CAST(SUM(CASE WHEN state = 'failed' THEN 1 ELSE 0 END) AS INTEGER),
                        CAST(SUM(CASE WHEN state = 'completed' THEN 1 ELSE 0 END) AS INTEGER)
                 FROM runs
                 WHERE datetime(created_at) >= datetime('now', ?)
                 GROUP BY bot_id",
            )
            .bind(modifier)
            .fetch_all(pool)
            .await
        } else {
            sqlx::query_as(
                "SELECT bot_id, COUNT(*), CAST(COALESCE(SUM(tokens_consumed), 0) AS INTEGER),
                        CAST(COALESCE(SUM(cost_estimate), 0) AS REAL),
                        CAST(SUM(CASE WHEN state = 'failed' THEN 1 ELSE 0 END) AS INTEGER),
                        CAST(SUM(CASE WHEN state = 'completed' THEN 1 ELSE 0 END) AS INTEGER)
                 FROM runs
                 GROUP BY bot_id",
            )
            .fetch_all(pool)
            .await
        }
    }
}

/// Model calls that happen outside an interactive run (planning, synthesis,
/// org design). They still spend money, so they get their own ledger rather
/// than being attributed to whichever run happened to be nearest.
pub struct UtilityUsageQueries;

impl UtilityUsageQueries {
    pub async fn record(
        pool: &SqlitePool,
        bot_id: Uuid,
        tokens: u64,
        cost: f64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO utility_usage (id, bot_id, tokens, cost)
             VALUES (?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(bot_id.to_string())
        .bind(tokens as i64)
        .bind(cost)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Total utility tokens and cost in the window.
    pub async fn total(pool: &SqlitePool, days: u32) -> Result<(i64, f64), sqlx::Error> {
        let modifier = (days > 0).then(|| format!("-{days} days"));
        if let Some(modifier) = modifier {
            sqlx::query_as(
                "SELECT CAST(COALESCE(SUM(tokens), 0) AS INTEGER),
                        CAST(COALESCE(SUM(cost), 0) AS REAL)
                 FROM utility_usage
                 WHERE datetime(created_at) >= datetime('now', ?)",
            )
            .bind(modifier)
            .fetch_one(pool)
            .await
        } else {
            sqlx::query_as(
                "SELECT CAST(COALESCE(SUM(tokens), 0) AS INTEGER),
                        CAST(COALESCE(SUM(cost), 0) AS REAL)
                 FROM utility_usage",
            )
            .fetch_one(pool)
            .await
        }
    }
}

/// A paired remote device. The token hash is never returned.
#[derive(Debug, Clone, Serialize)]
pub struct RemoteDevice {
    pub id: String,
    pub name: String,
    pub role: String,
    /// The local account this device last signed in as. All three account
    /// fields are `None` until a sign-in succeeds and become `None` again when
    /// the grant is revoked.
    pub account_id: Option<String>,
    pub account_name: Option<String>,
    pub account_role: Option<String>,
    pub created_at: String,
    pub last_seen_at: Option<String>,
    pub revoked_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoteAuditEntry {
    pub id: String,
    pub device_name: Option<String>,
    pub role: Option<String>,
    pub account_name: Option<String>,
    pub method: String,
    pub path: String,
    pub status: i64,
    pub created_at: String,
}

/// Paired devices, one-time pairing codes, and the remote audit trail.
pub struct RemoteAccessQueries;

impl RemoteAccessQueries {
    pub async fn create_pairing(
        pool: &SqlitePool,
        code_hash: &str,
        role: &str,
        ttl_secs: i64,
    ) -> Result<Uuid, sqlx::Error> {
        let id = Uuid::new_v4();
        let expires = chrono::Utc::now() + chrono::Duration::seconds(ttl_secs.max(60));
        sqlx::query(
            "INSERT INTO remote_pairings (id, code_hash, role, expires_at)
             VALUES (?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(code_hash)
        .bind(role)
        .bind(expires.to_rfc3339())
        .execute(pool)
        .await?;
        Ok(id)
    }

    /// Consume a one-time code. The conditional UPDATE is what makes two
    /// simultaneous attempts unable to both succeed.
    pub async fn consume_pairing(
        pool: &SqlitePool,
        code_hash: &str,
    ) -> Result<Option<(Uuid, String)>, sqlx::Error> {
        let row: Option<(String, String)> = sqlx::query_as(
            "SELECT id, role FROM remote_pairings
             WHERE code_hash = ? AND used_at IS NULL
               AND datetime(expires_at) > datetime('now')
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(code_hash)
        .fetch_optional(pool)
        .await?;
        let Some((id, role)) = row else {
            return Ok(None);
        };
        let result = sqlx::query(
            "UPDATE remote_pairings SET used_at = datetime('now')
             WHERE id = ? AND used_at IS NULL",
        )
        .bind(&id)
        .execute(pool)
        .await?;
        if result.rows_affected() != 1 {
            return Ok(None);
        }
        Ok(Some((Uuid::parse_str(&id).unwrap_or_default(), role)))
    }

    pub async fn create_device(
        pool: &SqlitePool,
        name: &str,
        role: &str,
        token_hash: &str,
    ) -> Result<Uuid, sqlx::Error> {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO remote_devices (id, name, role, token_hash)
             VALUES (?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(name)
        .bind(role)
        .bind(token_hash)
        .execute(pool)
        .await?;
        Ok(id)
    }

    pub async fn device_by_id(
        pool: &SqlitePool,
        id: &str,
    ) -> Result<Option<RemoteDevice>, sqlx::Error> {
        let row: Option<(
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            String,
            Option<String>,
            Option<String>,
        )> = sqlx::query_as(
            "SELECT id, name, role, account_id, account_name, account_role,
                    created_at, last_seen_at, revoked_at
                 FROM remote_devices WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|row| RemoteDevice {
            id: row.0,
            name: row.1,
            role: row.2,
            account_id: row.3,
            account_name: row.4,
            account_role: row.5,
            created_at: row.6,
            last_seen_at: row.7,
            revoked_at: row.8,
        }))
    }

    pub async fn device_for_token(
        pool: &SqlitePool,
        token_hash: &str,
    ) -> Result<Option<RemoteDevice>, sqlx::Error> {
        let row: Option<(
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            String,
            Option<String>,
            Option<String>,
        )> = sqlx::query_as(
            "SELECT id, name, role, account_id, account_name, account_role,
                    created_at, last_seen_at, revoked_at
                 FROM remote_devices
                 WHERE token_hash = ? AND revoked_at IS NULL",
        )
        .bind(token_hash)
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|row| RemoteDevice {
            id: row.0,
            name: row.1,
            role: row.2,
            account_id: row.3,
            account_name: row.4,
            account_role: row.5,
            created_at: row.6,
            last_seen_at: row.7,
            revoked_at: row.8,
        }))
    }

    /// Persist a successful account sign-in. The device keeps its paired role;
    /// the account role is stored so the host can show the binding and the
    /// effective role is always recomputed from both.
    pub async fn bind_account(
        pool: &SqlitePool,
        device_id: &str,
        account_id: &str,
        account_name: &str,
        account_role: &str,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE remote_devices
                SET account_id = ?, account_name = ?, account_role = ?
              WHERE id = ? AND revoked_at IS NULL",
        )
        .bind(account_id)
        .bind(account_name)
        .bind(account_role)
        .bind(device_id)
        .execute(pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Drop a binding that is no longer authorized. The device token stays
    /// valid for signing in again (or a different account), matching the
    /// `login_required` response.
    pub async fn clear_account_binding(
        pool: &SqlitePool,
        device_id: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE remote_devices
                SET account_id = NULL, account_name = NULL, account_role = NULL
              WHERE id = ?",
        )
        .bind(device_id)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn touch_device(pool: &SqlitePool, id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE remote_devices SET last_seen_at = datetime('now') WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn list_devices(pool: &SqlitePool) -> Result<Vec<RemoteDevice>, sqlx::Error> {
        let rows: Vec<(
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            String,
            Option<String>,
            Option<String>,
        )> = sqlx::query_as(
            "SELECT id, name, role, account_id, account_name, account_role,
                    created_at, last_seen_at, revoked_at
                 FROM remote_devices ORDER BY created_at DESC",
        )
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| RemoteDevice {
                id: row.0,
                name: row.1,
                role: row.2,
                account_id: row.3,
                account_name: row.4,
                account_role: row.5,
                created_at: row.6,
                last_seen_at: row.7,
                revoked_at: row.8,
            })
            .collect())
    }

    pub async fn revoke_device(pool: &SqlitePool, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE remote_devices SET revoked_at = datetime('now')
             WHERE id = ? AND revoked_at IS NULL",
        )
        .bind(id.to_string())
        .execute(pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn audit(
        pool: &SqlitePool,
        device_id: Option<&str>,
        device_name: Option<&str>,
        role: Option<&str>,
        account_name: Option<&str>,
        method: &str,
        path: &str,
        status: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO remote_audit
               (id, device_id, device_name, role, account_name, method, path, status)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(device_id)
        .bind(device_name)
        .bind(role)
        .bind(account_name)
        .bind(method)
        .bind(path)
        .bind(status)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Keep the remote tables bounded. Called when the listener starts, not on
    /// every request.
    pub async fn prune(pool: &SqlitePool) -> Result<(), sqlx::Error> {
        sqlx::query(
            "DELETE FROM remote_pairings
             WHERE datetime(expires_at) < datetime('now', '-1 day')",
        )
        .execute(pool)
        .await?;
        sqlx::query(
            "DELETE FROM remote_audit
             WHERE datetime(created_at) < datetime('now', '-90 days')",
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn recent_audit(
        pool: &SqlitePool,
        limit: i64,
    ) -> Result<Vec<RemoteAuditEntry>, sqlx::Error> {
        let rows: Vec<(
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            String,
            String,
            i64,
            String,
        )> = sqlx::query_as(
            "SELECT id, device_name, role, account_name, method, path, status, created_at
                 FROM remote_audit ORDER BY created_at DESC LIMIT ?",
        )
        .bind(limit.clamp(1, 200))
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| RemoteAuditEntry {
                id: row.0,
                device_name: row.1,
                role: row.2,
                account_name: row.3,
                method: row.4,
                path: row.5,
                status: row.6,
                created_at: row.7,
            })
            .collect())
    }
}

/// The recoverable part of a pending OIDC authorization-code flow.
///
/// The nonce and PKCE verifier are needed at the callback, so they are stored
/// in plaintext for the short attempt lifetime. Authorization codes, access
/// tokens and ID tokens are never stored.
#[derive(Debug, Clone)]
pub struct RemoteSsoAttempt {
    pub id: String,
    pub device_id: String,
    pub nonce: String,
    pub verifier: String,
    pub issuer: String,
    pub client_id: String,
    pub redirect_url: String,
    pub token_endpoint: String,
    pub jwks_uri: String,
    pub username_claim: String,
}

/// The values written when an SSO flow starts. The state is already hashed.
#[derive(Debug, Clone)]
pub struct NewRemoteSsoAttempt<'a> {
    pub state_hash: &'a str,
    pub device_id: &'a str,
    pub nonce: &'a str,
    pub verifier: &'a str,
    pub issuer: &'a str,
    pub client_id: &'a str,
    pub redirect_url: &'a str,
    pub token_endpoint: &'a str,
    pub jwks_uri: &'a str,
    pub username_claim: &'a str,
    pub ttl_secs: i64,
}

/// Pending remote SSO attempts. Rows are single-use and expire with their
/// authorization flow.
pub struct RemoteSsoQueries;

impl RemoteSsoQueries {
    /// Insert an attempt unless the device or the host already has too many
    /// outstanding flows. Expired rows are pruned first, so a stale tab cannot
    /// consume a device's allowance forever.
    pub async fn create(
        pool: &SqlitePool,
        attempt: &NewRemoteSsoAttempt<'_>,
        max_per_device: i64,
        max_global: i64,
    ) -> Result<bool, sqlx::Error> {
        let now = Utc::now().timestamp();
        let mut tx = pool.begin().await?;
        sqlx::query("DELETE FROM remote_sso_attempts WHERE expires_at <= ?")
            .bind(now)
            .execute(&mut *tx)
            .await?;
        let device_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM remote_sso_attempts
             WHERE device_id = ? AND expires_at > ?",
        )
        .bind(attempt.device_id)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;
        let total: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM remote_sso_attempts WHERE expires_at > ?")
                .bind(now)
                .fetch_one(&mut *tx)
                .await?;
        if device_count >= max_per_device.max(0) || total >= max_global.max(0) {
            tx.rollback().await?;
            return Ok(false);
        }
        sqlx::query(
            "INSERT INTO remote_sso_attempts
               (id, state_hash, device_id, nonce, verifier, issuer, client_id,
                redirect_url, token_endpoint, jwks_uri, username_claim,
                created_at, expires_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(attempt.state_hash)
        .bind(attempt.device_id)
        .bind(attempt.nonce)
        .bind(attempt.verifier)
        .bind(attempt.issuer)
        .bind(attempt.client_id)
        .bind(attempt.redirect_url)
        .bind(attempt.token_endpoint)
        .bind(attempt.jwks_uri)
        .bind(attempt.username_claim)
        .bind(now)
        .bind(now + attempt.ttl_secs)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    /// Atomically consume a state. A second callback with the same state gets
    /// `None`, and an expired row is never returned.
    pub async fn consume(
        pool: &SqlitePool,
        state_hash: &str,
    ) -> Result<Option<RemoteSsoAttempt>, sqlx::Error> {
        let now = Utc::now().timestamp();
        let mut tx = pool.begin().await?;
        let row: Option<(
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
        )> = sqlx::query_as(
            "SELECT id, device_id, nonce, verifier, issuer, client_id,
                    redirect_url, token_endpoint, jwks_uri, username_claim
               FROM remote_sso_attempts
              WHERE state_hash = ? AND expires_at > ?",
        )
        .bind(state_hash)
        .bind(now)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            tx.rollback().await?;
            return Ok(None);
        };
        let deleted = sqlx::query("DELETE FROM remote_sso_attempts WHERE id = ?")
            .bind(&row.0)
            .execute(&mut *tx)
            .await?;
        if deleted.rows_affected() != 1 {
            tx.rollback().await?;
            return Ok(None);
        }
        tx.commit().await?;
        Ok(Some(RemoteSsoAttempt {
            id: row.0,
            device_id: row.1,
            nonce: row.2,
            verifier: row.3,
            issuer: row.4,
            client_id: row.5,
            redirect_url: row.6,
            token_endpoint: row.7,
            jwks_uri: row.8,
            username_claim: row.9,
        }))
    }

    /// Number of live attempts for one device, used before discovery so a
    /// capped device cannot hammer the identity provider.
    pub async fn outstanding_for_device(
        pool: &SqlitePool,
        device_id: &str,
    ) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM remote_sso_attempts
             WHERE device_id = ? AND expires_at > ?",
        )
        .bind(device_id)
        .bind(Utc::now().timestamp())
        .fetch_one(pool)
        .await
    }

    pub async fn outstanding_total(pool: &SqlitePool) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar("SELECT COUNT(*) FROM remote_sso_attempts WHERE expires_at > ?")
            .bind(Utc::now().timestamp())
            .fetch_one(pool)
            .await
    }

    /// Drop expired attempts. Called when the listener starts and whenever a
    /// flow starts or finishes.
    pub async fn prune_expired(pool: &SqlitePool) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM remote_sso_attempts WHERE expires_at <= ?")
            .bind(Utc::now().timestamp())
            .execute(pool)
            .await?;
        Ok(())
    }
}

/// A reviewable outbound email draft.
///
/// The body is present here because this is the full row used by the draft
/// editor. [`EmailDraftSummary`] is what list responses return, so a list can
/// never leak a body by accident.
#[derive(Debug, Clone, Serialize)]
pub struct EmailDraft {
    pub id: String,
    pub bot_id: String,
    pub thread_id: Option<String>,
    pub run_id: Option<String>,
    pub to_addresses: Vec<String>,
    pub cc_addresses: Vec<String>,
    pub subject: String,
    pub body: String,
    pub status: String,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub sent_at: Option<String>,
}

/// Draft metadata for lists. Deliberately has no `body` field.
#[derive(Debug, Clone, Serialize)]
pub struct EmailDraftSummary {
    pub id: String,
    pub bot_id: String,
    pub thread_id: Option<String>,
    pub run_id: Option<String>,
    pub to_addresses: Vec<String>,
    pub cc_addresses: Vec<String>,
    pub subject: String,
    pub status: String,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub sent_at: Option<String>,
}

/// Values written when an agent creates a draft.
#[derive(Debug, Clone)]
pub struct NewEmailDraft<'a> {
    pub bot_id: &'a str,
    pub thread_id: Option<&'a str>,
    pub run_id: Option<&'a str>,
    pub to_addresses: &'a [String],
    pub cc_addresses: &'a [String],
    pub subject: &'a str,
    pub body: &'a str,
}

type EmailDraftRow = (
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    String,
    String,
    Option<String>,
);

fn decode_addresses(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
}

fn email_draft_from_row(row: EmailDraftRow) -> EmailDraft {
    EmailDraft {
        id: row.0,
        bot_id: row.1,
        thread_id: row.2,
        run_id: row.3,
        to_addresses: decode_addresses(&row.4),
        cc_addresses: decode_addresses(&row.5),
        subject: row.6,
        body: row.7,
        status: row.8,
        error: row.9,
        created_at: row.10,
        updated_at: row.11,
        sent_at: row.12,
    }
}

/// The error written when a process died between claiming a draft for sending
/// and recording the outcome. The text tells the person to verify delivery
/// before retrying, because the message may already have left the machine.
pub const SEND_INTERRUPTED_ERROR: &str =
    "The send was interrupted. Verify with the recipient before retrying.";

/// Outbound email drafts. Every status transition is guarded in SQL: a draft is
/// claimed atomically before SMTP, a claimed row can only become sent or return
/// to draft, and two clicks cannot send the same draft twice.
pub struct EmailDraftQueries;

impl EmailDraftQueries {
    pub async fn create(
        pool: &SqlitePool,
        draft: &NewEmailDraft<'_>,
    ) -> Result<EmailDraft, sqlx::Error> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let to = serde_json::to_string(draft.to_addresses).unwrap_or_else(|_| "[]".to_string());
        let cc = serde_json::to_string(draft.cc_addresses).unwrap_or_else(|_| "[]".to_string());
        sqlx::query(
            "INSERT INTO email_drafts
               (id, bot_id, thread_id, run_id, to_addresses, cc_addresses,
                subject, body, status, error, created_at, updated_at, sent_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'draft', NULL, ?, ?, NULL)",
        )
        .bind(&id)
        .bind(draft.bot_id)
        .bind(draft.thread_id)
        .bind(draft.run_id)
        .bind(to)
        .bind(cc)
        .bind(draft.subject)
        .bind(draft.body)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await?;
        Self::get(pool, &id)
            .await?
            .ok_or_else(|| sqlx::Error::RowNotFound)
    }

    /// Newest first, capped at 100 summaries. The body is never selected.
    pub async fn list(
        pool: &SqlitePool,
        limit: i64,
    ) -> Result<Vec<EmailDraftSummary>, sqlx::Error> {
        let limit = limit.clamp(1, 100);
        type SummaryRow = (
            String,
            String,
            Option<String>,
            Option<String>,
            String,
            String,
            String,
            String,
            Option<String>,
            String,
            String,
            Option<String>,
        );
        let rows: Vec<SummaryRow> = sqlx::query_as(
            "SELECT id, bot_id, thread_id, run_id, to_addresses, cc_addresses,
                    subject, status, error, created_at, updated_at, sent_at
               FROM email_drafts
              ORDER BY created_at DESC, id DESC
              LIMIT ?",
        )
        .bind(limit)
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(
                    id,
                    bot_id,
                    thread_id,
                    run_id,
                    to,
                    cc,
                    subject,
                    status,
                    error,
                    created_at,
                    updated_at,
                    sent_at,
                )| EmailDraftSummary {
                    id,
                    bot_id,
                    thread_id,
                    run_id,
                    to_addresses: decode_addresses(&to),
                    cc_addresses: decode_addresses(&cc),
                    subject,
                    status,
                    error,
                    created_at,
                    updated_at,
                    sent_at,
                },
            )
            .collect())
    }

    pub async fn get(pool: &SqlitePool, id: &str) -> Result<Option<EmailDraft>, sqlx::Error> {
        let row: Option<EmailDraftRow> = sqlx::query_as(
            "SELECT id, bot_id, thread_id, run_id, to_addresses, cc_addresses,
                    subject, body, status, error, created_at, updated_at, sent_at
               FROM email_drafts WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;
        Ok(row.map(email_draft_from_row))
    }

    /// Replace the editable content. Returns false when the row is missing or
    /// is no longer a draft.
    pub async fn update_content(
        pool: &SqlitePool,
        id: &str,
        to_addresses: &[String],
        cc_addresses: &[String],
        subject: &str,
        body: &str,
    ) -> Result<bool, sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let to = serde_json::to_string(to_addresses).unwrap_or_else(|_| "[]".to_string());
        let cc = serde_json::to_string(cc_addresses).unwrap_or_else(|_| "[]".to_string());
        let result = sqlx::query(
            "UPDATE email_drafts
                SET to_addresses = ?, cc_addresses = ?, subject = ?, body = ?,
                    error = NULL, updated_at = ?
              WHERE id = ? AND status = 'draft'",
        )
        .bind(to)
        .bind(cc)
        .bind(subject)
        .bind(body)
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Atomically claim a draft for sending.
    ///
    /// Only one caller can move `draft` to `sending`. The winner is the only
    /// caller allowed to open SMTP; a concurrent invocation loses here instead
    /// of sending a second copy.
    pub async fn begin_send(pool: &SqlitePool, id: &str) -> Result<bool, sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE email_drafts
                SET status = 'sending', error = NULL, sent_at = NULL, updated_at = ?
              WHERE id = ? AND status = 'draft'",
        )
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Mark a claimed draft sent. Only a row still in `sending` can transition;
    /// a draft, a failed send or a second success cannot become sent again.
    pub async fn mark_sent(pool: &SqlitePool, id: &str) -> Result<bool, sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE email_drafts
                SET status = 'sent', sent_at = ?, error = NULL, updated_at = ?
              WHERE id = ? AND status = 'sending'",
        )
        .bind(&now)
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn mark_discarded(pool: &SqlitePool, id: &str) -> Result<bool, sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE email_drafts
                SET status = 'discarded', updated_at = ?
              WHERE id = ? AND status = 'draft'",
        )
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Return a claimed draft to `draft` after a normal send failure, so it can
    /// be retried. The caller must pass a sanitized error and never the password
    /// or body.
    pub async fn mark_failed(
        pool: &SqlitePool,
        id: &str,
        error: &str,
    ) -> Result<bool, sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE email_drafts
                SET status = 'draft', error = ?, sent_at = NULL, updated_at = ?
              WHERE id = ? AND status = 'sending'",
        )
        .bind(error)
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Recover rows claimed for sending by a process that died before recording
    /// the outcome.
    ///
    /// SMTP has a 15-second timeout, so a row in `sending` for ten minutes is
    /// not an active send. It is reset to `draft` with an explicit warning:
    /// the message may have been delivered, so the person must verify before
    /// retrying. Returns the number of rows recovered.
    pub async fn recover_interrupted_sends(
        pool: &SqlitePool,
        older_than_secs: i64,
    ) -> Result<u64, sqlx::Error> {
        let now = Utc::now();
        let cutoff = now - chrono::Duration::seconds(older_than_secs.max(0));
        let result = sqlx::query(
            "UPDATE email_drafts
                SET status = 'draft', error = ?, sent_at = NULL, updated_at = ?
              WHERE status = 'sending' AND datetime(updated_at) < datetime(?)",
        )
        .bind(SEND_INTERRUPTED_ERROR)
        .bind(now.to_rfc3339())
        .bind(cutoff.to_rfc3339())
        .execute(pool)
        .await?;
        Ok(result.rows_affected())
    }
}

/// A spend request and the owner's decision. The URL is the merchant's own
/// checkout page; RAVENBOT opens it in the browser and never posts to it.
#[derive(Debug, Clone, Serialize)]
pub struct SpendRequest {
    pub id: String,
    pub bot_id: String,
    pub thread_id: Option<String>,
    pub run_id: Option<String>,
    pub title: String,
    pub merchant: String,
    pub merchant_url: String,
    pub amount_minor: i64,
    pub currency: String,
    pub reason: String,
    pub status: String,
    pub decided_by: Option<String>,
    pub decided_at: Option<String>,
    pub decision_note: Option<String>,
    pub purchased_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Values written when an agent creates a pending request.
#[derive(Debug, Clone)]
pub struct NewSpendRequest<'a> {
    pub bot_id: &'a str,
    pub thread_id: Option<&'a str>,
    pub run_id: Option<&'a str>,
    pub title: &'a str,
    pub merchant: &'a str,
    pub merchant_url: &'a str,
    pub amount_minor: i64,
    pub currency: &'a str,
    pub reason: &'a str,
}

/// Result of an approval attempt. The daily limit is re-checked inside the same
/// transaction that flips the row, so two approvals cannot both spend the last
/// of the day's budget.
#[derive(Debug, Clone)]
pub enum ApproveOutcome {
    Approved(SpendRequest),
    NotFound,
    NotPending,
    DailyLimitExceeded {
        spent_today_minor: u64,
        limit_minor: u64,
    },
}

#[derive(sqlx::FromRow)]
struct SpendRequestRow {
    id: String,
    bot_id: String,
    thread_id: Option<String>,
    run_id: Option<String>,
    title: String,
    merchant: String,
    merchant_url: String,
    amount_minor: i64,
    currency: String,
    reason: String,
    status: String,
    decided_by: Option<String>,
    decided_at: Option<String>,
    decision_note: Option<String>,
    purchased_at: Option<String>,
    created_at: String,
    updated_at: String,
}

fn spend_request_from_row(row: SpendRequestRow) -> SpendRequest {
    SpendRequest {
        id: row.id,
        bot_id: row.bot_id,
        thread_id: row.thread_id,
        run_id: row.run_id,
        title: row.title,
        merchant: row.merchant,
        merchant_url: row.merchant_url,
        amount_minor: row.amount_minor,
        currency: row.currency,
        reason: row.reason,
        status: row.status,
        decided_by: row.decided_by,
        decided_at: row.decided_at,
        decision_note: row.decision_note,
        purchased_at: row.purchased_at,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

/// Spend requests. Every decision transition is guarded by the current status,
/// so a browser retry or a second click cannot approve or purchase twice.
pub struct SpendRequestQueries;

impl SpendRequestQueries {
    pub async fn create(
        pool: &SqlitePool,
        request: &NewSpendRequest<'_>,
    ) -> Result<SpendRequest, sqlx::Error> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO spend_requests
               (id, bot_id, thread_id, run_id, title, merchant, merchant_url,
                amount_minor, currency, reason, status, decided_by, decided_at,
                decision_note, purchased_at, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', NULL, NULL,
                     NULL, NULL, ?, ?)",
        )
        .bind(&id)
        .bind(request.bot_id)
        .bind(request.thread_id)
        .bind(request.run_id)
        .bind(request.title)
        .bind(request.merchant)
        .bind(request.merchant_url)
        .bind(request.amount_minor)
        .bind(request.currency)
        .bind(request.reason)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await?;
        Self::get(pool, &id)
            .await?
            .ok_or_else(|| sqlx::Error::RowNotFound)
    }

    /// Newest first, capped at 100. Pending and decided rows share the list so
    /// the owner can see what was refused or bought, not only the queue.
    pub async fn list(pool: &SqlitePool, limit: i64) -> Result<Vec<SpendRequest>, sqlx::Error> {
        let limit = limit.clamp(1, 100);
        let rows: Vec<SpendRequestRow> = sqlx::query_as(
            "SELECT id, bot_id, thread_id, run_id, title, merchant, merchant_url,
                    amount_minor, currency, reason, status, decided_by, decided_at,
                    decision_note, purchased_at, created_at, updated_at
               FROM spend_requests
              ORDER BY created_at DESC, id DESC
              LIMIT ?",
        )
        .bind(limit)
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(spend_request_from_row).collect())
    }

    pub async fn get(pool: &SqlitePool, id: &str) -> Result<Option<SpendRequest>, sqlx::Error> {
        let row: Option<SpendRequestRow> = sqlx::query_as(
            "SELECT id, bot_id, thread_id, run_id, title, merchant, merchant_url,
                    amount_minor, currency, reason, status, decided_by, decided_at,
                    decision_note, purchased_at, created_at, updated_at
               FROM spend_requests WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;
        Ok(row.map(spend_request_from_row))
    }

    /// Sum of requests that counted against the day's budget. `day` is a
    /// `YYYY-MM-DD` UTC date; approved and purchased rows count on the day they
    /// were approved.
    pub async fn spent_on(pool: &SqlitePool, day: &str) -> Result<u64, sqlx::Error> {
        let total: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount_minor), 0)
               FROM spend_requests
              WHERE status IN ('approved', 'purchased')
                AND substr(decided_at, 1, 10) = ?",
        )
        .bind(day)
        .fetch_one(pool)
        .await?;
        Ok(total.max(0) as u64)
    }

    /// Atomically approve a pending request if the day's budget still allows
    /// it. Returns the committed row, or the exact reason nothing changed.
    pub async fn approve(
        pool: &SqlitePool,
        id: &str,
        decided_by: &str,
        daily_limit_minor: u64,
    ) -> Result<ApproveOutcome, sqlx::Error> {
        let now = Utc::now();
        let now_text = now.to_rfc3339();
        let day = now.date_naive().to_string();
        let mut tx = pool.begin().await?;
        let row: Option<(String, i64)> =
            sqlx::query_as("SELECT status, amount_minor FROM spend_requests WHERE id = ?")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        let Some((status, amount_minor)) = row else {
            tx.rollback().await?;
            return Ok(ApproveOutcome::NotFound);
        };
        if status != "pending" {
            tx.rollback().await?;
            return Ok(ApproveOutcome::NotPending);
        }
        let spent: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount_minor), 0)
               FROM spend_requests
              WHERE status IN ('approved', 'purchased')
                AND substr(decided_at, 1, 10) = ?
                AND id != ?",
        )
        .bind(&day)
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        let spent = spent.max(0) as u64;
        let amount = amount_minor.max(0) as u64;
        let exceeds = spent
            .checked_add(amount)
            .map(|total| total > daily_limit_minor)
            .unwrap_or(true);
        if exceeds {
            tx.rollback().await?;
            return Ok(ApproveOutcome::DailyLimitExceeded {
                spent_today_minor: spent,
                limit_minor: daily_limit_minor,
            });
        }
        sqlx::query(
            "UPDATE spend_requests
                SET status = 'approved', decided_by = ?, decided_at = ?,
                    decision_note = NULL, updated_at = ?
              WHERE id = ? AND status = 'pending'",
        )
        .bind(decided_by)
        .bind(&now_text)
        .bind(&now_text)
        .bind(id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(match Self::get(pool, id).await? {
            Some(request) => ApproveOutcome::Approved(request),
            None => ApproveOutcome::NotFound,
        })
    }

    /// Deny a pending request. A decided row cannot change status here.
    pub async fn deny(
        pool: &SqlitePool,
        id: &str,
        decided_by: &str,
        note: Option<&str>,
    ) -> Result<bool, sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE spend_requests
                SET status = 'denied', decided_by = ?, decided_at = ?,
                    decision_note = ?, updated_at = ?
              WHERE id = ? AND status = 'pending'",
        )
        .bind(decided_by)
        .bind(&now)
        .bind(note)
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Record that the person completed the purchase. Only an approved request
    /// can become purchased, and this records no payment details.
    pub async fn mark_purchased(pool: &SqlitePool, id: &str) -> Result<bool, sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE spend_requests
                SET status = 'purchased', purchased_at = ?, updated_at = ?
              WHERE id = ? AND status = 'approved'",
        )
        .bind(&now)
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Cancel a pending or approved request before it is marked purchased.
    pub async fn cancel(
        pool: &SqlitePool,
        id: &str,
        decided_by: &str,
        note: Option<&str>,
    ) -> Result<bool, sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE spend_requests
                SET status = 'cancelled',
                    decided_by = COALESCE(decided_by, ?),
                    decided_at = COALESCE(decided_at, ?),
                    decision_note = COALESCE(?, decision_note),
                    updated_at = ?
              WHERE id = ? AND status IN ('pending', 'approved')",
        )
        .bind(decided_by)
        .bind(&now)
        .bind(note)
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }
}

/// A Telegram chat paired with a RAVENBOT conversation.
#[derive(Debug, Clone, Serialize)]
pub struct TelegramLink {
    pub chat_id: String,
    pub chat_title: Option<String>,
    pub bot_id: String,
    pub thread_id: String,
    pub created_at: String,
    pub last_seen_at: Option<String>,
}

type TelegramLinkRow = (
    String,
    Option<String>,
    String,
    String,
    String,
    Option<String>,
);

pub struct TelegramQueries;

impl TelegramQueries {
    /// One active code at a time. A new code invalidates the previous one.
    pub async fn create_pairing(
        pool: &SqlitePool,
        code_hash: &str,
        bot_id: Uuid,
        ttl_secs: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE telegram_pairings SET used_at = datetime('now') WHERE used_at IS NULL")
            .execute(pool)
            .await?;
        sqlx::query(
            "DELETE FROM telegram_pairings
             WHERE datetime(expires_at) < datetime('now', '-1 day')",
        )
        .execute(pool)
        .await?;
        let expires = chrono::Utc::now() + chrono::Duration::seconds(ttl_secs.clamp(60, 600));
        sqlx::query(
            "INSERT INTO telegram_pairings (code_hash, bot_id, expires_at)
             VALUES (?, ?, ?)",
        )
        .bind(code_hash)
        .bind(bot_id.to_string())
        .bind(expires.to_rfc3339())
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn consume_pairing(
        pool: &SqlitePool,
        code_hash: &str,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT bot_id FROM telegram_pairings
             WHERE code_hash = ? AND used_at IS NULL
               AND datetime(expires_at) > datetime('now')
             LIMIT 1",
        )
        .bind(code_hash)
        .fetch_optional(pool)
        .await?;
        let Some((bot_id,)) = row else {
            return Ok(None);
        };
        let result = sqlx::query(
            "UPDATE telegram_pairings SET used_at = datetime('now')
             WHERE code_hash = ? AND used_at IS NULL",
        )
        .bind(code_hash)
        .execute(pool)
        .await?;
        if result.rows_affected() != 1 {
            return Ok(None);
        }
        Ok(Uuid::parse_str(&bot_id).ok())
    }

    pub async fn upsert_link(
        pool: &SqlitePool,
        chat_id: &str,
        chat_title: Option<&str>,
        bot_id: Uuid,
        thread_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO telegram_links (chat_id, chat_title, bot_id, thread_id, last_seen_at)
             VALUES (?, ?, ?, ?, datetime('now'))
             ON CONFLICT(chat_id) DO UPDATE SET
               chat_title = excluded.chat_title,
               bot_id = excluded.bot_id,
               thread_id = excluded.thread_id,
               last_seen_at = datetime('now')",
        )
        .bind(chat_id)
        .bind(chat_title)
        .bind(bot_id.to_string())
        .bind(thread_id.to_string())
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn link_for_chat(
        pool: &SqlitePool,
        chat_id: &str,
    ) -> Result<Option<TelegramLink>, sqlx::Error> {
        let row: Option<TelegramLinkRow> = sqlx::query_as(
            "SELECT chat_id, chat_title, bot_id, thread_id, created_at, last_seen_at
                 FROM telegram_links WHERE chat_id = ?",
        )
        .bind(chat_id)
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|row| TelegramLink {
            chat_id: row.0,
            chat_title: row.1,
            bot_id: row.2,
            thread_id: row.3,
            created_at: row.4,
            last_seen_at: row.5,
        }))
    }

    pub async fn list_links(pool: &SqlitePool) -> Result<Vec<TelegramLink>, sqlx::Error> {
        let rows: Vec<TelegramLinkRow> = sqlx::query_as(
            "SELECT chat_id, chat_title, bot_id, thread_id, created_at, last_seen_at
             FROM telegram_links ORDER BY created_at DESC",
        )
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| TelegramLink {
                chat_id: row.0,
                chat_title: row.1,
                bot_id: row.2,
                thread_id: row.3,
                created_at: row.4,
                last_seen_at: row.5,
            })
            .collect())
    }

    pub async fn unlink(pool: &SqlitePool, chat_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM telegram_links WHERE chat_id = ?")
            .bind(chat_id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn touch(pool: &SqlitePool, chat_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE telegram_links SET last_seen_at = datetime('now') WHERE chat_id = ?")
            .bind(chat_id)
            .execute(pool)
            .await?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SlackLink {
    pub channel_id: String,
    pub chat_title: Option<String>,
    pub bot_id: String,
    pub thread_id: String,
    pub created_at: String,
    pub last_seen_at: Option<String>,
}

type SlackLinkRow = (
    String,
    Option<String>,
    String,
    String,
    String,
    Option<String>,
);

pub struct SlackQueries;

impl SlackQueries {
    pub async fn create_pairing(
        pool: &SqlitePool,
        code_hash: &str,
        bot_id: Uuid,
        ttl_secs: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE slack_pairings SET used_at = datetime('now') WHERE used_at IS NULL")
            .execute(pool)
            .await?;
        sqlx::query(
            "DELETE FROM slack_pairings
             WHERE datetime(expires_at) < datetime('now', '-1 day')",
        )
        .execute(pool)
        .await?;
        let expires = chrono::Utc::now() + chrono::Duration::seconds(ttl_secs.clamp(60, 600));
        sqlx::query(
            "INSERT INTO slack_pairings (code_hash, bot_id, expires_at)
             VALUES (?, ?, ?)",
        )
        .bind(code_hash)
        .bind(bot_id.to_string())
        .bind(expires.to_rfc3339())
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn consume_pairing(
        pool: &SqlitePool,
        code_hash: &str,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT bot_id FROM slack_pairings
             WHERE code_hash = ? AND used_at IS NULL
               AND datetime(expires_at) > datetime('now')
             LIMIT 1",
        )
        .bind(code_hash)
        .fetch_optional(pool)
        .await?;
        let Some((bot_id,)) = row else {
            return Ok(None);
        };
        let result = sqlx::query(
            "UPDATE slack_pairings SET used_at = datetime('now')
             WHERE code_hash = ? AND used_at IS NULL",
        )
        .bind(code_hash)
        .execute(pool)
        .await?;
        if result.rows_affected() != 1 {
            return Ok(None);
        }
        Ok(Uuid::parse_str(&bot_id).ok())
    }

    pub async fn upsert_link(
        pool: &SqlitePool,
        channel_id: &str,
        chat_title: Option<&str>,
        bot_id: Uuid,
        thread_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO slack_links (channel_id, chat_title, bot_id, thread_id, last_seen_at)
             VALUES (?, ?, ?, ?, datetime('now'))
             ON CONFLICT(channel_id) DO UPDATE SET
               chat_title = excluded.chat_title,
               bot_id = excluded.bot_id,
               thread_id = excluded.thread_id,
               last_seen_at = datetime('now')",
        )
        .bind(channel_id)
        .bind(chat_title)
        .bind(bot_id.to_string())
        .bind(thread_id.to_string())
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn link_for_channel(
        pool: &SqlitePool,
        channel_id: &str,
    ) -> Result<Option<SlackLink>, sqlx::Error> {
        let row: Option<SlackLinkRow> = sqlx::query_as(
            "SELECT channel_id, chat_title, bot_id, thread_id, created_at, last_seen_at
             FROM slack_links WHERE channel_id = ?",
        )
        .bind(channel_id)
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|row| SlackLink {
            channel_id: row.0,
            chat_title: row.1,
            bot_id: row.2,
            thread_id: row.3,
            created_at: row.4,
            last_seen_at: row.5,
        }))
    }

    pub async fn list_links(pool: &SqlitePool) -> Result<Vec<SlackLink>, sqlx::Error> {
        let rows: Vec<SlackLinkRow> = sqlx::query_as(
            "SELECT channel_id, chat_title, bot_id, thread_id, created_at, last_seen_at
             FROM slack_links ORDER BY created_at DESC",
        )
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| SlackLink {
                channel_id: row.0,
                chat_title: row.1,
                bot_id: row.2,
                thread_id: row.3,
                created_at: row.4,
                last_seen_at: row.5,
            })
            .collect())
    }

    pub async fn unlink(pool: &SqlitePool, channel_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM slack_links WHERE channel_id = ?")
            .bind(channel_id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn touch(pool: &SqlitePool, channel_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE slack_links SET last_seen_at = datetime('now') WHERE channel_id = ?")
            .bind(channel_id)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Claim an event for handling. Returns false when this channel has
    /// already handled the same Slack timestamp.
    pub async fn claim_event(
        pool: &SqlitePool,
        channel_id: &str,
        event_ts: &str,
    ) -> Result<bool, sqlx::Error> {
        sqlx::query(
            "DELETE FROM slack_events
             WHERE datetime(created_at) < datetime('now', '-1 day')",
        )
        .execute(pool)
        .await?;
        let result =
            sqlx::query("INSERT OR IGNORE INTO slack_events (channel_id, event_ts) VALUES (?, ?)")
                .bind(channel_id)
                .bind(event_ts)
                .execute(pool)
                .await?;
        Ok(result.rows_affected() == 1)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DiscordLink {
    pub channel_id: String,
    pub guild_id: Option<String>,
    pub chat_title: Option<String>,
    pub bot_id: String,
    pub thread_id: String,
    pub created_at: String,
    pub last_seen_at: Option<String>,
}

type DiscordLinkRow = (
    String,
    Option<String>,
    Option<String>,
    String,
    String,
    String,
    Option<String>,
);

pub struct DiscordQueries;

impl DiscordQueries {
    pub async fn create_pairing(
        pool: &SqlitePool,
        code_hash: &str,
        bot_id: Uuid,
        ttl_secs: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE discord_pairings SET used_at = datetime('now') WHERE used_at IS NULL")
            .execute(pool)
            .await?;
        sqlx::query(
            "DELETE FROM discord_pairings
             WHERE datetime(expires_at) < datetime('now', '-1 day')",
        )
        .execute(pool)
        .await?;
        let expires = chrono::Utc::now() + chrono::Duration::seconds(ttl_secs.clamp(60, 600));
        sqlx::query(
            "INSERT INTO discord_pairings (code_hash, bot_id, expires_at)
             VALUES (?, ?, ?)",
        )
        .bind(code_hash)
        .bind(bot_id.to_string())
        .bind(expires.to_rfc3339())
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn consume_pairing(
        pool: &SqlitePool,
        code_hash: &str,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT bot_id FROM discord_pairings
             WHERE code_hash = ? AND used_at IS NULL
               AND datetime(expires_at) > datetime('now')
             LIMIT 1",
        )
        .bind(code_hash)
        .fetch_optional(pool)
        .await?;
        let Some((bot_id,)) = row else {
            return Ok(None);
        };
        let result = sqlx::query(
            "UPDATE discord_pairings SET used_at = datetime('now')
             WHERE code_hash = ? AND used_at IS NULL",
        )
        .bind(code_hash)
        .execute(pool)
        .await?;
        if result.rows_affected() != 1 {
            return Ok(None);
        }
        Ok(Uuid::parse_str(&bot_id).ok())
    }

    pub async fn upsert_link(
        pool: &SqlitePool,
        channel_id: &str,
        guild_id: Option<&str>,
        chat_title: Option<&str>,
        bot_id: Uuid,
        thread_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO discord_links
               (channel_id, guild_id, chat_title, bot_id, thread_id, last_seen_at)
             VALUES (?, ?, ?, ?, ?, datetime('now'))
             ON CONFLICT(channel_id) DO UPDATE SET
               guild_id = excluded.guild_id,
               chat_title = excluded.chat_title,
               bot_id = excluded.bot_id,
               thread_id = excluded.thread_id,
               last_seen_at = datetime('now')",
        )
        .bind(channel_id)
        .bind(guild_id)
        .bind(chat_title)
        .bind(bot_id.to_string())
        .bind(thread_id.to_string())
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn link_for_channel(
        pool: &SqlitePool,
        channel_id: &str,
    ) -> Result<Option<DiscordLink>, sqlx::Error> {
        let row: Option<DiscordLinkRow> = sqlx::query_as(
            "SELECT channel_id, guild_id, chat_title, bot_id, thread_id, created_at, last_seen_at
             FROM discord_links WHERE channel_id = ?",
        )
        .bind(channel_id)
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|row| DiscordLink {
            channel_id: row.0,
            guild_id: row.1,
            chat_title: row.2,
            bot_id: row.3,
            thread_id: row.4,
            created_at: row.5,
            last_seen_at: row.6,
        }))
    }

    pub async fn list_links(pool: &SqlitePool) -> Result<Vec<DiscordLink>, sqlx::Error> {
        let rows: Vec<DiscordLinkRow> = sqlx::query_as(
            "SELECT channel_id, guild_id, chat_title, bot_id, thread_id, created_at, last_seen_at
             FROM discord_links ORDER BY created_at DESC",
        )
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| DiscordLink {
                channel_id: row.0,
                guild_id: row.1,
                chat_title: row.2,
                bot_id: row.3,
                thread_id: row.4,
                created_at: row.5,
                last_seen_at: row.6,
            })
            .collect())
    }

    pub async fn unlink(pool: &SqlitePool, channel_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM discord_links WHERE channel_id = ?")
            .bind(channel_id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn touch(pool: &SqlitePool, channel_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE discord_links SET last_seen_at = datetime('now') WHERE channel_id = ?")
            .bind(channel_id)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Claim a dispatch for handling. Returns false when this channel has
    /// already handled the same Discord message id.
    pub async fn claim_event(
        pool: &SqlitePool,
        channel_id: &str,
        message_id: &str,
    ) -> Result<bool, sqlx::Error> {
        sqlx::query(
            "DELETE FROM discord_events
             WHERE datetime(created_at) < datetime('now', '-1 day')",
        )
        .execute(pool)
        .await?;
        let result = sqlx::query(
            "INSERT OR IGNORE INTO discord_events (channel_id, message_id) VALUES (?, ?)",
        )
        .bind(channel_id)
        .bind(message_id)
        .execute(pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct WhatsAppLink {
    pub wa_id: String,
    pub display_name: Option<String>,
    pub bot_id: String,
    pub thread_id: String,
    pub created_at: String,
    pub last_seen_at: Option<String>,
}

type WhatsAppLinkRow = (
    String,
    Option<String>,
    String,
    String,
    String,
    Option<String>,
);

pub struct WhatsAppQueries;

impl WhatsAppQueries {
    pub async fn create_pairing(
        pool: &SqlitePool,
        code_hash: &str,
        bot_id: Uuid,
        ttl_secs: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE whatsapp_pairings SET used_at = datetime('now') WHERE used_at IS NULL")
            .execute(pool)
            .await?;
        sqlx::query(
            "DELETE FROM whatsapp_pairings
             WHERE datetime(expires_at) < datetime('now', '-1 day')",
        )
        .execute(pool)
        .await?;
        let expires = chrono::Utc::now() + chrono::Duration::seconds(ttl_secs.clamp(60, 600));
        sqlx::query(
            "INSERT INTO whatsapp_pairings (code_hash, bot_id, expires_at)
             VALUES (?, ?, ?)",
        )
        .bind(code_hash)
        .bind(bot_id.to_string())
        .bind(expires.to_rfc3339())
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn consume_pairing(
        pool: &SqlitePool,
        code_hash: &str,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT bot_id FROM whatsapp_pairings
             WHERE code_hash = ? AND used_at IS NULL
               AND datetime(expires_at) > datetime('now')
             LIMIT 1",
        )
        .bind(code_hash)
        .fetch_optional(pool)
        .await?;
        let Some((bot_id,)) = row else {
            return Ok(None);
        };
        let result = sqlx::query(
            "UPDATE whatsapp_pairings SET used_at = datetime('now')
             WHERE code_hash = ? AND used_at IS NULL",
        )
        .bind(code_hash)
        .execute(pool)
        .await?;
        if result.rows_affected() != 1 {
            return Ok(None);
        }
        Ok(Uuid::parse_str(&bot_id).ok())
    }

    pub async fn upsert_link(
        pool: &SqlitePool,
        wa_id: &str,
        display_name: Option<&str>,
        bot_id: Uuid,
        thread_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO whatsapp_links
               (wa_id, display_name, bot_id, thread_id, last_seen_at)
             VALUES (?, ?, ?, ?, datetime('now'))
             ON CONFLICT(wa_id) DO UPDATE SET
               display_name = excluded.display_name,
               bot_id = excluded.bot_id,
               thread_id = excluded.thread_id,
               last_seen_at = datetime('now')",
        )
        .bind(wa_id)
        .bind(display_name)
        .bind(bot_id.to_string())
        .bind(thread_id.to_string())
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn link_for_number(
        pool: &SqlitePool,
        wa_id: &str,
    ) -> Result<Option<WhatsAppLink>, sqlx::Error> {
        let row: Option<WhatsAppLinkRow> = sqlx::query_as(
            "SELECT wa_id, display_name, bot_id, thread_id, created_at, last_seen_at
             FROM whatsapp_links WHERE wa_id = ?",
        )
        .bind(wa_id)
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|row| WhatsAppLink {
            wa_id: row.0,
            display_name: row.1,
            bot_id: row.2,
            thread_id: row.3,
            created_at: row.4,
            last_seen_at: row.5,
        }))
    }

    pub async fn list_links(pool: &SqlitePool) -> Result<Vec<WhatsAppLink>, sqlx::Error> {
        let rows: Vec<WhatsAppLinkRow> = sqlx::query_as(
            "SELECT wa_id, display_name, bot_id, thread_id, created_at, last_seen_at
             FROM whatsapp_links ORDER BY created_at DESC",
        )
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| WhatsAppLink {
                wa_id: row.0,
                display_name: row.1,
                bot_id: row.2,
                thread_id: row.3,
                created_at: row.4,
                last_seen_at: row.5,
            })
            .collect())
    }

    pub async fn unlink(pool: &SqlitePool, wa_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM whatsapp_links WHERE wa_id = ?")
            .bind(wa_id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn touch(pool: &SqlitePool, wa_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE whatsapp_links SET last_seen_at = datetime('now') WHERE wa_id = ?")
            .bind(wa_id)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Claim a webhook message. Returns false when this number has already
    /// handled the same Meta message id.
    pub async fn claim_event(
        pool: &SqlitePool,
        wa_id: &str,
        message_id: &str,
    ) -> Result<bool, sqlx::Error> {
        sqlx::query(
            "DELETE FROM whatsapp_events
             WHERE datetime(created_at) < datetime('now', '-1 day')",
        )
        .execute(pool)
        .await?;
        let result =
            sqlx::query("INSERT OR IGNORE INTO whatsapp_events (wa_id, message_id) VALUES (?, ?)")
                .bind(wa_id)
                .bind(message_id)
                .execute(pool)
                .await?;
        Ok(result.rows_affected() == 1)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TeamsLink {
    pub conversation_id: String,
    pub tenant_id: String,
    pub conversation_type: Option<String>,
    pub service_url: String,
    pub chat_title: Option<String>,
    pub bot_id: String,
    pub thread_id: String,
    pub created_at: String,
    pub last_seen_at: Option<String>,
}

type TeamsLinkRow = (
    String,
    String,
    Option<String>,
    String,
    Option<String>,
    String,
    String,
    String,
    Option<String>,
);

pub struct TeamsQueries;

impl TeamsQueries {
    pub async fn create_pairing(
        pool: &SqlitePool,
        code_hash: &str,
        bot_id: Uuid,
        ttl_secs: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE teams_pairings SET used_at = datetime('now') WHERE used_at IS NULL")
            .execute(pool)
            .await?;
        sqlx::query(
            "DELETE FROM teams_pairings
             WHERE datetime(expires_at) < datetime('now', '-1 day')",
        )
        .execute(pool)
        .await?;
        let expires = chrono::Utc::now() + chrono::Duration::seconds(ttl_secs.clamp(60, 600));
        sqlx::query(
            "INSERT INTO teams_pairings (code_hash, bot_id, expires_at)
             VALUES (?, ?, ?)",
        )
        .bind(code_hash)
        .bind(bot_id.to_string())
        .bind(expires.to_rfc3339())
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn consume_pairing(
        pool: &SqlitePool,
        code_hash: &str,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT bot_id FROM teams_pairings
             WHERE code_hash = ? AND used_at IS NULL
               AND datetime(expires_at) > datetime('now')
             LIMIT 1",
        )
        .bind(code_hash)
        .fetch_optional(pool)
        .await?;
        let Some((bot_id,)) = row else {
            return Ok(None);
        };
        let result = sqlx::query(
            "UPDATE teams_pairings SET used_at = datetime('now')
             WHERE code_hash = ? AND used_at IS NULL",
        )
        .bind(code_hash)
        .execute(pool)
        .await?;
        if result.rows_affected() != 1 {
            return Ok(None);
        }
        Ok(Uuid::parse_str(&bot_id).ok())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_link(
        pool: &SqlitePool,
        conversation_id: &str,
        tenant_id: &str,
        conversation_type: Option<&str>,
        service_url: &str,
        chat_title: Option<&str>,
        bot_id: Uuid,
        thread_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO teams_links
               (conversation_id, tenant_id, conversation_type, service_url,
                chat_title, bot_id, thread_id, last_seen_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, datetime('now'))
             ON CONFLICT(conversation_id) DO UPDATE SET
               tenant_id = excluded.tenant_id,
               conversation_type = excluded.conversation_type,
               service_url = excluded.service_url,
               chat_title = excluded.chat_title,
               bot_id = excluded.bot_id,
               thread_id = excluded.thread_id,
               last_seen_at = datetime('now')",
        )
        .bind(conversation_id)
        .bind(tenant_id)
        .bind(conversation_type)
        .bind(service_url)
        .bind(chat_title)
        .bind(bot_id.to_string())
        .bind(thread_id.to_string())
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn link_for_conversation(
        pool: &SqlitePool,
        conversation_id: &str,
    ) -> Result<Option<TeamsLink>, sqlx::Error> {
        let row: Option<TeamsLinkRow> = sqlx::query_as(
            "SELECT conversation_id, tenant_id, conversation_type, service_url,
                    chat_title, bot_id, thread_id, created_at, last_seen_at
             FROM teams_links WHERE conversation_id = ?",
        )
        .bind(conversation_id)
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|row| TeamsLink {
            conversation_id: row.0,
            tenant_id: row.1,
            conversation_type: row.2,
            service_url: row.3,
            chat_title: row.4,
            bot_id: row.5,
            thread_id: row.6,
            created_at: row.7,
            last_seen_at: row.8,
        }))
    }

    pub async fn list_links(pool: &SqlitePool) -> Result<Vec<TeamsLink>, sqlx::Error> {
        let rows: Vec<TeamsLinkRow> = sqlx::query_as(
            "SELECT conversation_id, tenant_id, conversation_type, service_url,
                    chat_title, bot_id, thread_id, created_at, last_seen_at
             FROM teams_links ORDER BY created_at DESC",
        )
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| TeamsLink {
                conversation_id: row.0,
                tenant_id: row.1,
                conversation_type: row.2,
                service_url: row.3,
                chat_title: row.4,
                bot_id: row.5,
                thread_id: row.6,
                created_at: row.7,
                last_seen_at: row.8,
            })
            .collect())
    }

    pub async fn unlink(pool: &SqlitePool, conversation_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM teams_links WHERE conversation_id = ?")
            .bind(conversation_id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn touch(pool: &SqlitePool, conversation_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE teams_links SET last_seen_at = datetime('now') WHERE conversation_id = ?",
        )
        .bind(conversation_id)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn claim_event(
        pool: &SqlitePool,
        conversation_id: &str,
        activity_id: &str,
    ) -> Result<bool, sqlx::Error> {
        sqlx::query(
            "DELETE FROM teams_events
             WHERE datetime(created_at) < datetime('now', '-1 day')",
        )
        .execute(pool)
        .await?;
        let result = sqlx::query(
            "INSERT OR IGNORE INTO teams_events (conversation_id, activity_id) VALUES (?, ?)",
        )
        .bind(conversation_id)
        .bind(activity_id)
        .execute(pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct WorkflowTaskRecord {
    pub bot_id: String,
    pub label: String,
    /// "trigger", "agent", "llm", "approval", or "output".
    #[serde(default = "default_node_type")]
    pub node_type: String,
    pub depends_on: Vec<usize>,
    #[serde(default)]
    pub condition: Option<WorkflowCondition>,
    /// Explicit input dependencies. None means all dependencies; Some([]) means
    /// no upstream output is injected.
    #[serde(default)]
    pub data_from: Option<Vec<usize>>,
    /// Additional attempts after the first failure, capped at 3.
    #[serde(default)]
    pub retries: u8,
    /// Optional JSON Schema the node's final answer must satisfy.
    #[serde(default)]
    pub output_schema: Option<String>,
    /// Total successful runs for this task (1 = no loop), capped at 10.
    #[serde(default = "default_loop_iterations")]
    pub loop_iterations: u8,
    /// Optional early stop for a looping task.
    #[serde(default)]
    pub loop_until: Option<WorkflowLoopStop>,
    /// Optional named group. Every task with the same name repeats as a unit.
    #[serde(default)]
    pub loop_group: Option<String>,
    /// Marks the group's terminal task. Its output is checked for the stop
    /// phrase and its completion publishes the group to external dependents.
    #[serde(default)]
    pub loop_group_terminal: bool,
    /// Total group iterations, stored on the terminal task.
    #[serde(default = "default_loop_iterations")]
    pub loop_group_iterations: u8,
    /// Optional early stop for the group, stored on the terminal task.
    #[serde(default)]
    pub loop_group_stop: Option<WorkflowLoopStop>,
}

fn default_loop_iterations() -> u8 {
    1
}

fn default_node_type() -> String {
    "agent".to_string()
}

#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct WorkflowLoopStop {
    /// "contains" or "not_contains".
    pub operator: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct WorkflowCondition {
    /// Earlier task index whose output is inspected.
    pub source: usize,
    /// "contains" or "not_contains".
    pub operator: String,
    /// Case-insensitive phrase.
    pub value: String,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct WorkflowRecord {
    pub id: String,
    pub name: String,
    pub goal: String,
    pub tasks: Vec<WorkflowTaskRecord>,
    pub office_id: Option<String>,
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct WorkflowVersionRecord {
    pub id: String,
    pub workflow_id: String,
    pub version: i64,
    pub name: String,
    pub goal: String,
    pub tasks: Vec<WorkflowTaskRecord>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct WorkflowRunRecord {
    pub id: String,
    pub workflow_id: String,
    pub workflow_version: i64,
    pub thread_id: Option<String>,
    pub goal: String,
    pub state: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub detail: Option<String>,
}

/// Reusable office workflow definitions. The tasks are stored as JSON because
/// they are a plan, not relational rows; execution still goes through the
/// runtime's TaskGraph.
pub struct WorkflowQueries;

type WorkflowRow = (
    String,
    String,
    String,
    String,
    Option<String>,
    i64,
    String,
    String,
);

impl WorkflowQueries {
    pub async fn list(
        pool: &SqlitePool,
        office_id: Option<Uuid>,
    ) -> Result<Vec<WorkflowRecord>, sqlx::Error> {
        let rows: Vec<WorkflowRow> = if let Some(office_id) = office_id {
            sqlx::query_as(
                "SELECT id, name, goal, tasks, office_id, version, created_at, updated_at
                 FROM workflows
                 WHERE office_id = ? OR office_id IS NULL
                 ORDER BY updated_at DESC",
            )
            .bind(office_id.to_string())
            .fetch_all(pool)
            .await?
        } else {
            sqlx::query_as(
                "SELECT id, name, goal, tasks, office_id, version, created_at, updated_at
                 FROM workflows
                 ORDER BY updated_at DESC",
            )
            .fetch_all(pool)
            .await?
        };
        Ok(rows
            .into_iter()
            .filter_map(
                |(id, name, goal, tasks, office_id, version, created_at, updated_at)| {
                    let tasks = serde_json::from_str::<Vec<WorkflowTaskRecord>>(&tasks).ok()?;
                    Some(WorkflowRecord {
                        id,
                        name,
                        goal,
                        tasks,
                        office_id,
                        version,
                        created_at,
                        updated_at,
                    })
                },
            )
            .collect())
    }

    pub async fn get(
        pool: &SqlitePool,
        workflow_id: &str,
    ) -> Result<Option<WorkflowRecord>, sqlx::Error> {
        let row: Option<WorkflowRow> = sqlx::query_as(
            "SELECT id, name, goal, tasks, office_id, version, created_at, updated_at
             FROM workflows WHERE id = ?",
        )
        .bind(workflow_id)
        .fetch_optional(pool)
        .await?;
        let Some((id, name, goal, tasks, office_id, version, created_at, updated_at)) = row else {
            return Ok(None);
        };
        let tasks = serde_json::from_str::<Vec<WorkflowTaskRecord>>(&tasks)
            .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
        Ok(Some(WorkflowRecord {
            id,
            name,
            goal,
            tasks,
            office_id,
            version,
            created_at,
            updated_at,
        }))
    }

    /// Save a workflow and capture an immutable version snapshot.
    #[allow(clippy::too_many_arguments)]
    pub async fn upsert(
        pool: &SqlitePool,
        id: &str,
        name: &str,
        goal: &str,
        tasks_json: &str,
        office_id: Option<Uuid>,
    ) -> Result<i64, sqlx::Error> {
        let mut tx = pool.begin().await?;
        let existing: Option<(i64,)> = sqlx::query_as("SELECT version FROM workflows WHERE id = ?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
        let version = existing.map(|(version,)| version + 1).unwrap_or(1);
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO workflows (id, name, goal, tasks, office_id, version, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
               name = excluded.name,
               goal = excluded.goal,
               tasks = excluded.tasks,
               office_id = excluded.office_id,
               version = excluded.version,
               updated_at = excluded.updated_at",
        )
        .bind(id)
        .bind(name)
        .bind(goal)
        .bind(tasks_json)
        .bind(office_id.map(|value| value.to_string()))
        .bind(version)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT OR REPLACE INTO workflow_versions
             (id, workflow_id, version, name, goal, tasks, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(format!("{id}-v{version}"))
        .bind(id)
        .bind(version)
        .bind(name)
        .bind(goal)
        .bind(tasks_json)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
        let cutoff = version - 50;
        if cutoff > 0 {
            sqlx::query("DELETE FROM workflow_versions WHERE workflow_id = ? AND version <= ?")
                .bind(id)
                .bind(cutoff)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(version)
    }

    pub async fn list_versions(
        pool: &SqlitePool,
        workflow_id: &str,
        limit: i64,
    ) -> Result<Vec<WorkflowVersionRecord>, sqlx::Error> {
        type Row = (String, String, i64, String, String, String, String);
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT id, workflow_id, version, name, goal, tasks, created_at
             FROM workflow_versions
             WHERE workflow_id = ?
             ORDER BY version DESC
             LIMIT ?",
        )
        .bind(workflow_id)
        .bind(limit)
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(
                |(id, workflow_id, version, name, goal, tasks, created_at)| {
                    let tasks = serde_json::from_str::<Vec<WorkflowTaskRecord>>(&tasks).ok()?;
                    Some(WorkflowVersionRecord {
                        id,
                        workflow_id,
                        version,
                        name,
                        goal,
                        tasks,
                        created_at,
                    })
                },
            )
            .collect())
    }

    pub async fn get_version(
        pool: &SqlitePool,
        workflow_id: &str,
        version: i64,
    ) -> Result<Option<WorkflowVersionRecord>, sqlx::Error> {
        let row: Option<(String, String, i64, String, String, String, String)> = sqlx::query_as(
            "SELECT id, workflow_id, version, name, goal, tasks, created_at
                 FROM workflow_versions
                 WHERE workflow_id = ? AND version = ?",
        )
        .bind(workflow_id)
        .bind(version)
        .fetch_optional(pool)
        .await?;
        Ok(row.and_then(
            |(id, workflow_id, version, name, goal, tasks, created_at)| {
                let tasks = serde_json::from_str::<Vec<WorkflowTaskRecord>>(&tasks).ok()?;
                Some(WorkflowVersionRecord {
                    id,
                    workflow_id,
                    version,
                    name,
                    goal,
                    tasks,
                    created_at,
                })
            },
        ))
    }

    pub async fn delete(pool: &SqlitePool, id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM workflows WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn record_run(
        pool: &SqlitePool,
        workflow_id: &str,
        workflow_version: i64,
        thread_id: Option<&str>,
        goal: &str,
        state: &str,
        detail: Option<&str>,
    ) -> Result<String, sqlx::Error> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO workflow_runs
             (id, workflow_id, workflow_version, thread_id, goal, state, started_at, finished_at, detail)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(workflow_id)
        .bind(workflow_version)
        .bind(thread_id)
        .bind(goal)
        .bind(state)
        .bind(&now)
        .bind(&now)
        .bind(detail)
        .execute(pool)
        .await?;
        Ok(id)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn begin_run(
        pool: &SqlitePool,
        workflow_id: &str,
        workflow_version: i64,
        thread_id: Option<&str>,
        goal: &str,
    ) -> Result<String, sqlx::Error> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO workflow_runs
             (id, workflow_id, workflow_version, thread_id, goal, state, started_at, finished_at, detail)
             VALUES (?, ?, ?, ?, ?, 'running', ?, NULL, NULL)",
        )
        .bind(&id)
        .bind(workflow_id)
        .bind(workflow_version)
        .bind(thread_id)
        .bind(goal)
        .bind(&now)
        .execute(pool)
        .await?;
        Ok(id)
    }

    pub async fn finish_run(
        pool: &SqlitePool,
        run_id: &str,
        state: &str,
        thread_id: Option<&str>,
        detail: Option<&str>,
    ) -> Result<bool, sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE workflow_runs
             SET state = ?, thread_id = COALESCE(?, thread_id),
                 finished_at = ?, detail = ?
             WHERE id = ? AND state = 'running'",
        )
        .bind(state)
        .bind(thread_id)
        .bind(&now)
        .bind(detail)
        .bind(run_id)
        .execute(pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn list_runs(
        pool: &SqlitePool,
        workflow_id: &str,
        limit: i64,
    ) -> Result<Vec<WorkflowRunRecord>, sqlx::Error> {
        type Row = (
            String,
            String,
            i64,
            Option<String>,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
        );
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT id, workflow_id, workflow_version, thread_id, goal, state,
                    started_at, finished_at, detail
             FROM workflow_runs
             WHERE workflow_id = ?
             ORDER BY started_at DESC
             LIMIT ?",
        )
        .bind(workflow_id)
        .bind(limit)
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(
                    id,
                    workflow_id,
                    workflow_version,
                    thread_id,
                    goal,
                    state,
                    started_at,
                    finished_at,
                    detail,
                )| WorkflowRunRecord {
                    id,
                    workflow_id,
                    workflow_version,
                    thread_id,
                    goal,
                    state,
                    started_at,
                    finished_at,
                    detail,
                },
            )
            .collect())
    }
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct BrowserProfileRecord {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    /// "inherit", "always", or "never".
    pub cookie_policy: String,
}

pub struct BrowserProfileQueries;

impl BrowserProfileQueries {
    pub async fn list(pool: &SqlitePool) -> Result<Vec<BrowserProfileRecord>, sqlx::Error> {
        let rows: Vec<(String, String, String, String, String)> = sqlx::query_as(
            "SELECT id, name, created_at, updated_at, cookie_policy
             FROM browser_profiles
             ORDER BY name COLLATE NOCASE ASC",
        )
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(id, name, created_at, updated_at, cookie_policy)| BrowserProfileRecord {
                    id,
                    name,
                    created_at,
                    updated_at,
                    cookie_policy,
                },
            )
            .collect())
    }

    pub async fn get(
        pool: &SqlitePool,
        id: &str,
    ) -> Result<Option<BrowserProfileRecord>, sqlx::Error> {
        let row: Option<(String, String, String, String, String)> = sqlx::query_as(
            "SELECT id, name, created_at, updated_at, cookie_policy
             FROM browser_profiles WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;
        Ok(row.map(
            |(id, name, created_at, updated_at, cookie_policy)| BrowserProfileRecord {
                id,
                name,
                created_at,
                updated_at,
                cookie_policy,
            },
        ))
    }

    pub async fn create(pool: &SqlitePool, id: &str, name: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO browser_profiles (id, name, created_at, updated_at)
             VALUES (?, ?, ?, ?)",
        )
        .bind(id)
        .bind(name)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn rename(pool: &SqlitePool, id: &str, name: &str) -> Result<bool, sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let result =
            sqlx::query("UPDATE browser_profiles SET name = ?, updated_at = ? WHERE id = ?")
                .bind(name)
                .bind(&now)
                .bind(id)
                .execute(pool)
                .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn set_cookie_policy(
        pool: &SqlitePool,
        id: &str,
        policy: &str,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE browser_profiles SET cookie_policy = ?, updated_at = ? WHERE id = ?",
        )
        .bind(policy)
        .bind(Utc::now().to_rfc3339())
        .bind(id)
        .execute(pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn delete(pool: &SqlitePool, id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM browser_profiles WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct WorkflowScheduleRecord {
    pub workflow_id: String,
    pub interval_hours: i64,
    pub enabled: bool,
    pub next_run_at: Option<String>,
    pub last_run_at: Option<String>,
    pub last_error: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct ClaimedWorkflow {
    pub run_id: String,
    pub workflow_id: String,
    pub name: String,
    pub version: i64,
    pub goal: String,
    pub tasks: Vec<WorkflowTaskRecord>,
    pub office_id: String,
    pub next_run_at: String,
}

impl WorkflowQueries {
    pub async fn get_schedule(
        pool: &SqlitePool,
        workflow_id: &str,
    ) -> Result<Option<WorkflowScheduleRecord>, sqlx::Error> {
        let row: Option<(
            String,
            i64,
            i64,
            Option<String>,
            Option<String>,
            Option<String>,
            String,
        )> = sqlx::query_as(
            "SELECT workflow_id, interval_hours, enabled, next_run_at, last_run_at,
                        last_error, updated_at
                 FROM workflow_schedules WHERE workflow_id = ?",
        )
        .bind(workflow_id)
        .fetch_optional(pool)
        .await?;
        Ok(row.map(
            |(
                workflow_id,
                interval_hours,
                enabled,
                next_run_at,
                last_run_at,
                last_error,
                updated_at,
            )| {
                WorkflowScheduleRecord {
                    workflow_id,
                    interval_hours,
                    enabled: enabled != 0,
                    next_run_at,
                    last_run_at,
                    last_error,
                    updated_at,
                }
            },
        ))
    }

    pub async fn set_schedule(
        pool: &SqlitePool,
        workflow_id: &str,
        interval_hours: i64,
        enabled: bool,
        next_run_at: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO workflow_schedules
             (workflow_id, interval_hours, enabled, next_run_at, last_run_at, last_error, updated_at)
             VALUES (?, ?, ?, ?, NULL, NULL, ?)
             ON CONFLICT(workflow_id) DO UPDATE SET
               interval_hours = excluded.interval_hours,
               enabled = excluded.enabled,
               next_run_at = excluded.next_run_at,
               updated_at = excluded.updated_at",
        )
        .bind(workflow_id)
        .bind(interval_hours)
        .bind(if enabled { 1 } else { 0 })
        .bind(next_run_at)
        .bind(&now)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn mark_schedule_error(
        pool: &SqlitePool,
        workflow_id: &str,
        error: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE workflow_schedules SET last_error = ?, updated_at = ? WHERE workflow_id = ?",
        )
        .bind(error)
        .bind(Utc::now().to_rfc3339())
        .bind(workflow_id)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Claim one due workflow, create its running row, and advance the schedule.
    pub async fn claim_due(
        pool: &SqlitePool,
        now: &str,
    ) -> Result<Option<ClaimedWorkflow>, sqlx::Error> {
        let mut tx = pool.begin().await?;
        let row: Option<(String, String, i64, String, String, Option<String>, i64)> =
            sqlx::query_as(
                "SELECT ws.workflow_id, w.name, w.version, w.goal, w.tasks,
                        w.office_id, ws.interval_hours
                 FROM workflow_schedules ws
                 JOIN workflows w ON w.id = ws.workflow_id
                 WHERE ws.enabled = 1
                   AND ws.next_run_at IS NOT NULL
                   AND ws.next_run_at <= ?
                   AND w.office_id IS NOT NULL
                 ORDER BY ws.next_run_at ASC
                 LIMIT 1",
            )
            .bind(now)
            .fetch_optional(&mut *tx)
            .await?;
        let Some((workflow_id, name, version, goal, tasks_json, office_id, interval_hours)) = row
        else {
            tx.commit().await?;
            return Ok(None);
        };
        let Some(office_id) = office_id else {
            tx.commit().await?;
            return Ok(None);
        };
        let Ok(tasks) = serde_json::from_str::<Vec<WorkflowTaskRecord>>(&tasks_json) else {
            let next = (Utc::now() + chrono::Duration::hours(interval_hours.max(1))).to_rfc3339();
            sqlx::query(
                "UPDATE workflow_schedules SET next_run_at = ?, last_run_at = ?,
                    last_error = 'Stored workflow tasks are corrupt', updated_at = ?
                 WHERE workflow_id = ?",
            )
            .bind(&next)
            .bind(now)
            .bind(now)
            .bind(&workflow_id)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            return Ok(None);
        };
        let run_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO workflow_runs
             (id, workflow_id, workflow_version, thread_id, goal, state, started_at, finished_at, detail)
             VALUES (?, ?, ?, NULL, ?, 'running', ?, NULL, NULL)",
        )
        .bind(&run_id)
        .bind(&workflow_id)
        .bind(version)
        .bind(&goal)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        let next_run_at =
            (Utc::now() + chrono::Duration::hours(interval_hours.max(1))).to_rfc3339();
        sqlx::query(
            "UPDATE workflow_schedules SET next_run_at = ?, last_run_at = ?,
                last_error = NULL, updated_at = ?
             WHERE workflow_id = ?",
        )
        .bind(&next_run_at)
        .bind(now)
        .bind(now)
        .bind(&workflow_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Some(ClaimedWorkflow {
            run_id,
            workflow_id,
            name,
            version,
            goal,
            tasks,
            office_id,
            next_run_at,
        }))
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
    pub async fn create(
        pool: &SqlitePool,
        req: &ravenbot_core::ApprovalRequest,
    ) -> Result<(), sqlx::Error> {
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
    pub async fn get(
        pool: &SqlitePool,
        id: Uuid,
    ) -> Result<Option<ravenbot_core::ApprovalRequest>, sqlx::Error> {
        type Row = (
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            Option<String>,
            String,
            Option<String>,
        );
        let row: Option<Row> = sqlx::query_as(
            "SELECT id, bot_id, thread_id, run_id, tool_name, tool_label, arguments, risk, status, note, created_at, decided_at
             FROM approvals WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|r| {
            Self::row_to_domain(r.0, r.1, r.2, r.3, r.4, r.5, r.6, r.7, r.8, r.9, r.10, r.11)
        }))
    }

    /// Pending requests for a thread (composer block + inline cards)
    pub async fn list_pending_for_thread(
        pool: &SqlitePool,
        thread_id: Uuid,
    ) -> Result<Vec<ravenbot_core::ApprovalRequest>, sqlx::Error> {
        type Row = (
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            Option<String>,
            String,
            Option<String>,
        );
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT id, bot_id, thread_id, run_id, tool_name, tool_label, arguments, risk, status, note, created_at, decided_at
             FROM approvals WHERE thread_id = ? AND status = 'pending' ORDER BY created_at ASC",
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

    /// Every pending request anywhere under one bot.
    ///
    /// Needed because a run does not stay on the thread the user is reading.
    /// A delegated child runs on its own thread and parks its approvals there,
    /// so a lookup scoped to the open thread cannot find them: after a reload
    /// the run would sit waiting for a decision nothing on screen could show or
    /// make. Same user, same bot, same conversation — just a different row.
    pub async fn list_pending_for_bot(
        pool: &SqlitePool,
        bot_id: Uuid,
    ) -> Result<Vec<ravenbot_core::ApprovalRequest>, sqlx::Error> {
        type Row = (
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            Option<String>,
            String,
            Option<String>,
        );
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT id, bot_id, thread_id, run_id, tool_name, tool_label, arguments, risk, status, note, created_at, decided_at
             FROM approvals
             WHERE status = 'pending'
               AND EXISTS (SELECT 1 FROM threads WHERE threads.id = approvals.thread_id AND threads.bot_id = ?)
             ORDER BY created_at ASC",
        )
        .bind(bot_id.to_string())
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                Self::row_to_domain(r.0, r.1, r.2, r.3, r.4, r.5, r.6, r.7, r.8, r.9, r.10, r.11)
            })
            .collect())
    }

    /// Every pending request, for a remote attention surface. Bounded because
    /// this is a glanceable list, not an archive.
    pub async fn list_pending_all(
        pool: &SqlitePool,
        limit: i64,
    ) -> Result<Vec<ravenbot_core::ApprovalRequest>, sqlx::Error> {
        type Row = (
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            Option<String>,
            String,
            Option<String>,
        );
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT id, bot_id, thread_id, run_id, tool_name, tool_label, arguments, risk, status, note, created_at, decided_at
             FROM approvals WHERE status = 'pending' ORDER BY created_at ASC LIMIT ?",
        )
        .bind(limit.clamp(1, 200))
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                Self::row_to_domain(r.0, r.1, r.2, r.3, r.4, r.5, r.6, r.7, r.8, r.9, r.10, r.11)
            })
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
        sqlx::query(
            "UPDATE approvals SET status = 'expired' WHERE run_id = ? AND status = 'pending'",
        )
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
        kind: String,
        payload: Option<String>,
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
            kind: ravenbot_core::QuestionKind::from_db(&kind),
            payload: payload
                .as_deref()
                .and_then(|value| serde_json::from_str(value).ok()),
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
            "INSERT INTO questions (id, bot_id, thread_id, run_id, header, question, options, allow_custom, kind, payload, status, answer, created_at, answered_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(req.id.to_string())
        .bind(req.bot_id.to_string())
        .bind(req.thread_id.to_string())
        .bind(req.run_id.to_string())
        .bind(&req.header)
        .bind(&req.question)
        .bind(serde_json::to_string(&req.options).unwrap_or_else(|_| "[]".into()))
        .bind(if req.allow_custom { 1 } else { 0 })
        .bind(req.kind.as_str())
        .bind(
            req.payload
                .as_ref()
                .and_then(|payload| serde_json::to_string(payload).ok()),
        )
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
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            i64,
            String,
            Option<String>,
            String,
            Option<String>,
            String,
            Option<String>,
        );
        let row: Option<Row> = sqlx::query_as(
            "SELECT id, bot_id, thread_id, run_id, header, question, options, allow_custom, kind, payload, status, answer, created_at, answered_at
             FROM questions WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|r| {
            Self::row_to_domain(
                r.0, r.1, r.2, r.3, r.4, r.5, r.6, r.7, r.8, r.9, r.10, r.11, r.12, r.13,
            )
        }))
    }

    pub async fn list_pending_for_thread(
        pool: &SqlitePool,
        thread_id: Uuid,
    ) -> Result<Vec<ravenbot_core::QuestionRequest>, sqlx::Error> {
        type Row = (
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            i64,
            String,
            Option<String>,
            String,
            Option<String>,
            String,
            Option<String>,
        );
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT id, bot_id, thread_id, run_id, header, question, options, allow_custom, kind, payload, status, answer, created_at, answered_at
             FROM questions WHERE thread_id = ? AND status = 'pending' ORDER BY created_at ASC",
        )
        .bind(thread_id.to_string())
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                Self::row_to_domain(
                    r.0, r.1, r.2, r.3, r.4, r.5, r.6, r.7, r.8, r.9, r.10, r.11, r.12, r.13,
                )
            })
            .collect())
    }

    /// Every pending question anywhere under one bot — same reasoning as
    /// `ApprovalQueries::list_pending_for_bot`: a parked `ask_user` on a child
    /// thread is invisible to a thread-scoped lookup, and an invisible question
    /// is a run that never resumes.
    pub async fn list_pending_for_bot(
        pool: &SqlitePool,
        bot_id: Uuid,
    ) -> Result<Vec<ravenbot_core::QuestionRequest>, sqlx::Error> {
        type Row = (
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            i64,
            String,
            Option<String>,
            String,
            Option<String>,
            String,
            Option<String>,
        );
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT id, bot_id, thread_id, run_id, header, question, options, allow_custom, kind, payload, status, answer, created_at, answered_at
             FROM questions
             WHERE status = 'pending'
               AND EXISTS (SELECT 1 FROM threads WHERE threads.id = questions.thread_id AND threads.bot_id = ?)
             ORDER BY created_at ASC",
        )
        .bind(bot_id.to_string())
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                Self::row_to_domain(
                    r.0, r.1, r.2, r.3, r.4, r.5, r.6, r.7, r.8, r.9, r.10, r.11, r.12, r.13,
                )
            })
            .collect())
    }

    /// Every pending question, bounded for a remote attention surface.
    pub async fn list_pending_all(
        pool: &SqlitePool,
        limit: i64,
    ) -> Result<Vec<ravenbot_core::QuestionRequest>, sqlx::Error> {
        type Row = (
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            i64,
            String,
            Option<String>,
            String,
            Option<String>,
            String,
            Option<String>,
        );
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT id, bot_id, thread_id, run_id, header, question, options, allow_custom, kind, payload, status, answer, created_at, answered_at
             FROM questions WHERE status = 'pending' ORDER BY created_at ASC LIMIT ?",
        )
        .bind(limit.clamp(1, 200))
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                Self::row_to_domain(
                    r.0, r.1, r.2, r.3, r.4, r.5, r.6, r.7, r.8, r.9, r.10, r.11, r.12, r.13,
                )
            })
            .collect())
    }

    /// Record the user's answer. Returns false if it wasn't pending.
    pub async fn answer(pool: &SqlitePool, id: Uuid, answer: &str) -> Result<bool, sqlx::Error> {
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
        sqlx::query(
            "UPDATE questions SET status = 'expired' WHERE run_id = ? AND status = 'pending'",
        )
        .bind(run_id.to_string())
        .execute(pool)
        .await?;
        Ok(())
    }
}

/// Provider API keys persistence (migration 009)
pub struct ProviderKeyQueries;

impl ProviderKeyQueries {
    /// List all configured provider API keys.
    ///
    /// A row whose value is the keychain marker is resolved from the OS
    /// keychain. A legacy plaintext row is migrated on first read when the
    /// platform store accepts it; if it does not, the row keeps working from
    /// SQLite so a headless box is not locked out of its own provider.
    pub async fn list(
        pool: &SqlitePool,
    ) -> Result<std::collections::HashMap<String, String>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String, String)>(
            "SELECT provider, api_key FROM provider_keys WHERE api_key != ''",
        )
        .fetch_all(pool)
        .await?;
        let mut out = std::collections::HashMap::new();
        for (provider, stored) in rows {
            if stored == crate::secrets::KEYCHAIN_MARKER {
                if let Some(key) = crate::secrets::get_provider_key(&provider).await {
                    out.insert(provider, key);
                }
                continue;
            }
            if stored.trim().is_empty() {
                continue;
            }
            if crate::secrets::set_provider_key(&provider, &stored).await {
                let _ = sqlx::query(
                    "UPDATE provider_keys SET api_key = ?, updated_at = datetime('now') WHERE provider = ?",
                )
                .bind(crate::secrets::KEYCHAIN_MARKER)
                .bind(&provider)
                .execute(pool)
                .await;
            }
            out.insert(provider, stored);
        }
        Ok(out)
    }

    /// Get API key for a specific provider.
    pub async fn get(pool: &SqlitePool, provider: &str) -> Result<Option<String>, sqlx::Error> {
        let row =
            sqlx::query_as::<_, (String,)>("SELECT api_key FROM provider_keys WHERE provider = ?")
                .bind(provider)
                .fetch_optional(pool)
                .await?;
        let Some((stored,)) = row else {
            return Ok(None);
        };
        if stored == crate::secrets::KEYCHAIN_MARKER {
            return Ok(crate::secrets::get_provider_key(provider).await);
        }
        if stored.trim().is_empty() {
            return Ok(None);
        }
        if crate::secrets::set_provider_key(provider, &stored).await {
            let _ = sqlx::query(
                "UPDATE provider_keys SET api_key = ?, updated_at = datetime('now') WHERE provider = ?",
            )
            .bind(crate::secrets::KEYCHAIN_MARKER)
            .bind(provider)
            .execute(pool)
            .await;
        }
        Ok(Some(stored))
    }

    /// Set (upsert) an API key for a provider, or delete if empty.
    ///
    /// The keychain is tried first. The database only receives a marker on
    /// success; on failure the value is stored as before and the UI keeps
    /// working, but `list`/`get` will try to migrate it again next time.
    pub async fn set(pool: &SqlitePool, provider: &str, api_key: &str) -> Result<(), sqlx::Error> {
        let trimmed = api_key.trim();
        if trimmed.is_empty() {
            crate::secrets::delete_provider_key(provider).await;
            sqlx::query("DELETE FROM provider_keys WHERE provider = ?")
                .bind(provider)
                .execute(pool)
                .await?;
        } else {
            let stored = if crate::secrets::set_provider_key(provider, trimmed).await {
                crate::secrets::KEYCHAIN_MARKER
            } else {
                trimmed
            };
            sqlx::query(
                "INSERT INTO provider_keys (provider, api_key, updated_at) VALUES (?, ?, datetime('now'))
                 ON CONFLICT(provider) DO UPDATE SET api_key = excluded.api_key, updated_at = excluded.updated_at"
            )
            .bind(provider)
            .bind(stored)
            .execute(pool)
            .await?;
        }
        Ok(())
    }

    /// Delete an API key for a provider
    pub async fn delete(pool: &SqlitePool, provider: &str) -> Result<(), sqlx::Error> {
        crate::secrets::delete_provider_key(provider).await;
        sqlx::query("DELETE FROM provider_keys WHERE provider = ?")
            .bind(provider)
            .execute(pool)
            .await?;
        Ok(())
    }
}

/// Resolve a custom provider's headers, migrating a legacy plaintext JSON
/// value to the keychain on first read. A marker whose keychain entry cannot be
/// read is treated as no headers rather than exposing the marker as JSON.
async fn resolve_custom_headers(
    pool: &SqlitePool,
    provider_id: &str,
    stored: Option<String>,
) -> Result<Option<String>, sqlx::Error> {
    let Some(stored) = stored else {
        return Ok(None);
    };
    if stored == crate::secrets::KEYCHAIN_MARKER {
        return Ok(crate::secrets::get_custom_provider_headers(provider_id).await);
    }
    if stored.trim().is_empty() {
        return Ok(None);
    }
    if crate::secrets::set_custom_provider_headers(provider_id, &stored).await {
        let _ = sqlx::query(
            "UPDATE custom_providers SET headers_json = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(crate::secrets::KEYCHAIN_MARKER)
        .bind(provider_id)
        .execute(pool)
        .await;
    }
    Ok(Some(stored))
}

/// User-defined model providers (migration 018).
/// Rows map to (id, display_name, kind, base_url, default_model, supports_tools, enabled).
pub struct CustomProviderQueries;

impl CustomProviderQueries {
    pub async fn list(
        pool: &SqlitePool,
    ) -> Result<
        Vec<(
            String,
            String,
            String,
            String,
            String,
            bool,
            bool,
            Option<String>,
        )>,
        sqlx::Error,
    > {
        let rows = sqlx::query_as::<_, (String, String, String, String, String, i64, i64, Option<String>)>(
            "SELECT id, display_name, kind, base_url, default_model, supports_tools, enabled, headers_json FROM custom_providers ORDER BY display_name"
        )
        .fetch_all(pool)
        .await?;
        let mut out = Vec::with_capacity(rows.len());
        for (id, dn, kind, base, dm, tools, en, headers) in rows {
            let headers = resolve_custom_headers(pool, &id, headers).await?;
            out.push((id, dn, kind, base, dm, tools != 0, en != 0, headers));
        }
        Ok(out)
    }

    pub async fn get(
        pool: &SqlitePool,
        id: &str,
    ) -> Result<
        Option<(
            String,
            String,
            String,
            String,
            String,
            bool,
            bool,
            Option<String>,
        )>,
        sqlx::Error,
    > {
        let row = sqlx::query_as::<_, (String, String, String, String, String, i64, i64, Option<String>)>(
            "SELECT id, display_name, kind, base_url, default_model, supports_tools, enabled, headers_json FROM custom_providers WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;
        let Some((id, dn, kind, base, dm, tools, en, headers)) = row else {
            return Ok(None);
        };
        let headers = resolve_custom_headers(pool, &id, headers).await?;
        Ok(Some((id, dn, kind, base, dm, tools != 0, en != 0, headers)))
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
        headers_json: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let headers_stored = match headers_json
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            None => {
                crate::secrets::delete_custom_provider_headers(id).await;
                None
            }
            Some(value) => {
                if crate::secrets::set_custom_provider_headers(id, value).await {
                    Some(crate::secrets::KEYCHAIN_MARKER.to_string())
                } else {
                    Some(value.to_string())
                }
            }
        };
        sqlx::query(
            "INSERT INTO custom_providers (id, display_name, kind, base_url, default_model, supports_tools, enabled, headers_json, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))
             ON CONFLICT(id) DO UPDATE SET display_name = excluded.display_name, kind = excluded.kind,
               base_url = excluded.base_url, default_model = excluded.default_model,
               supports_tools = excluded.supports_tools, enabled = excluded.enabled,
               headers_json = excluded.headers_json, updated_at = excluded.updated_at"
        )
        .bind(id)
        .bind(display_name)
        .bind(kind)
        .bind(base_url)
        .bind(default_model)
        .bind(if supports_tools { 1i64 } else { 0i64 })
        .bind(if enabled { 1i64 } else { 0i64 })
        .bind(headers_stored)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn delete(pool: &SqlitePool, id: &str) -> Result<(), sqlx::Error> {
        crate::secrets::delete_custom_provider_headers(id).await;
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
            "SELECT provider, base_url FROM provider_base_urls",
        )
        .fetch_all(pool)
        .await?;
        Ok(rows)
    }

    pub async fn get(pool: &SqlitePool, provider: &str) -> Result<Option<String>, sqlx::Error> {
        let row = sqlx::query_as::<_, (String,)>(
            "SELECT base_url FROM provider_base_urls WHERE provider = ?",
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
        let row = sqlx::query_as::<_, (String,)>("SELECT value FROM app_settings WHERE key = ?")
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
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    i64,
    Option<String>,
    String,
    String,
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
        let rows: Vec<ChannelRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {} FROM channels ORDER BY position ASC, name ASC",
            Self::COLS
        )))
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(Self::row_to_domain).collect())
    }

    pub async fn get(
        pool: &SqlitePool,
        id: Uuid,
    ) -> Result<Option<ravenbot_core::Channel>, sqlx::Error> {
        let row: Option<ChannelRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {} FROM channels WHERE id = ?",
            Self::COLS
        )))
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

/// An MCP notification that starts a routine.
///
/// `event` is a JSON-RPC notification method (`notifications/message`,
/// `notifications/resources/updated`, or `*`). `resource_uri` is only needed
/// for servers that gate updates behind `resources/subscribe`.
#[derive(Debug, Clone, Serialize)]
pub struct McpEventTrigger {
    pub id: Uuid,
    pub routine_id: Uuid,
    pub server_id: String,
    pub event: String,
    pub resource_uri: String,
    pub match_text: String,
    pub cooldown_secs: i64,
    pub enabled: bool,
    pub last_fired_at: Option<String>,
    pub last_error: String,
    pub created_at: String,
    pub updated_at: String,
}

type McpEventTriggerRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    i64,
    i64,
    Option<String>,
    String,
    String,
    String,
);

fn mcp_event_trigger_from_row(row: McpEventTriggerRow) -> Option<McpEventTrigger> {
    Some(McpEventTrigger {
        id: Uuid::parse_str(&row.0).ok()?,
        routine_id: Uuid::parse_str(&row.1).ok()?,
        server_id: row.2,
        event: row.3,
        resource_uri: row.4,
        match_text: row.5,
        cooldown_secs: row.6,
        enabled: row.7 != 0,
        last_fired_at: row.8,
        last_error: row.9,
        created_at: row.10,
        updated_at: row.11,
    })
}

const MCP_EVENT_TRIGGER_COLUMNS: &str =
    "id, routine_id, server_id, event, resource_uri, match_text, cooldown_secs,
     enabled, last_fired_at, last_error, created_at, updated_at";

const MCP_EVENT_TRIGGER_COLUMNS_T: &str =
    "t.id, t.routine_id, t.server_id, t.event, t.resource_uri, t.match_text,
     t.cooldown_secs, t.enabled, t.last_fired_at, t.last_error, t.created_at,
     t.updated_at";

pub struct McpEventTriggerQueries;

impl McpEventTriggerQueries {
    pub async fn create(
        pool: &SqlitePool,
        routine_id: Uuid,
        server_id: &str,
        event: &str,
        resource_uri: &str,
        match_text: &str,
        cooldown_secs: i64,
    ) -> Result<McpEventTrigger, sqlx::Error> {
        let id = Uuid::new_v4();
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO mcp_event_triggers
                (id, routine_id, server_id, event, resource_uri, match_text,
                 cooldown_secs, enabled, last_fired_at, last_error, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, 1, NULL, '', ?, ?)",
        )
        .bind(id.to_string())
        .bind(routine_id.to_string())
        .bind(server_id.trim())
        .bind(event.trim())
        .bind(resource_uri.trim())
        .bind(match_text.trim())
        .bind(cooldown_secs.clamp(0, 3_600))
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await?;
        Self::get(pool, id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    pub async fn get(pool: &SqlitePool, id: Uuid) -> Result<Option<McpEventTrigger>, sqlx::Error> {
        let row: Option<McpEventTriggerRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {MCP_EVENT_TRIGGER_COLUMNS} FROM mcp_event_triggers WHERE id = ?"
        )))
        .bind(id.to_string())
        .fetch_optional(pool)
        .await?;
        Ok(row.and_then(mcp_event_trigger_from_row))
    }

    /// Triggers belonging to one bot's routines, for the routine panel.
    pub async fn list_for_bot(
        pool: &SqlitePool,
        bot_id: Uuid,
    ) -> Result<Vec<McpEventTrigger>, sqlx::Error> {
        let rows: Vec<McpEventTriggerRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {MCP_EVENT_TRIGGER_COLUMNS_T}
             FROM mcp_event_triggers t
             JOIN routines r ON r.id = t.routine_id
             WHERE r.bot_id = ?
             ORDER BY t.created_at, t.event"
        )))
        .bind(bot_id.to_string())
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(mcp_event_trigger_from_row)
            .collect())
    }

    /// Enabled triggers on enabled routines, for the listener supervisor.
    pub async fn list_enabled(pool: &SqlitePool) -> Result<Vec<McpEventTrigger>, sqlx::Error> {
        let rows: Vec<McpEventTriggerRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {MCP_EVENT_TRIGGER_COLUMNS_T}
             FROM mcp_event_triggers t
             JOIN routines r ON r.id = t.routine_id
             WHERE t.enabled = 1 AND r.is_enabled = 1
             ORDER BY t.server_id, t.created_at"
        )))
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(mcp_event_trigger_from_row)
            .collect())
    }

    pub async fn set_enabled(
        pool: &SqlitePool,
        id: Uuid,
        enabled: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE mcp_event_triggers SET enabled = ?, updated_at = ? WHERE id = ?")
            .bind(i64::from(enabled))
            .bind(Utc::now().to_rfc3339())
            .bind(id.to_string())
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn delete(pool: &SqlitePool, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM mcp_event_triggers WHERE id = ?")
            .bind(id.to_string())
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn delete_for_routine(
        pool: &SqlitePool,
        routine_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM mcp_event_triggers WHERE routine_id = ?")
            .bind(routine_id.to_string())
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Atomically claim one firing slot.
    ///
    /// `cutoff` is `now - cooldown_secs`. The update is the concurrency gate:
    /// two notifications arriving together cannot both start the same routine,
    /// and a log storm cannot run it dozens of times per minute.
    pub async fn claim(
        pool: &SqlitePool,
        id: Uuid,
        now: &str,
        cutoff: &str,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE mcp_event_triggers
             SET last_fired_at = ?, last_error = '', updated_at = ?
             WHERE id = ? AND enabled = 1
               AND (last_fired_at IS NULL OR last_fired_at <= ?)",
        )
        .bind(now)
        .bind(now)
        .bind(id.to_string())
        .bind(cutoff)
        .execute(pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn record_error(pool: &SqlitePool, id: Uuid, error: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE mcp_event_triggers SET last_error = ?, updated_at = ? WHERE id = ?")
            .bind(error.chars().take(1_000).collect::<String>())
            .bind(Utc::now().to_rfc3339())
            .bind(id.to_string())
            .execute(pool)
            .await?;
        Ok(())
    }
}

/// A read-only public link to one conversation.
///
/// The token itself is never stored; `for_token` hashes the supplied value and
/// looks that up. `created_at` is also the snapshot boundary: messages after it
/// are not part of the share.
#[derive(Debug, Clone, Serialize)]
pub struct ThreadShare {
    pub id: Uuid,
    pub thread_id: Uuid,
    pub created_at: String,
    pub expires_at: String,
    pub revoked_at: Option<String>,
    pub view_count: i64,
    pub last_viewed_at: Option<String>,
}

type ThreadShareRow = (
    String,
    String,
    String,
    String,
    Option<String>,
    i64,
    Option<String>,
);

fn thread_share_from_row(row: ThreadShareRow) -> Option<ThreadShare> {
    Some(ThreadShare {
        id: Uuid::parse_str(&row.0).ok()?,
        thread_id: Uuid::parse_str(&row.1).ok()?,
        created_at: row.2,
        expires_at: row.3,
        revoked_at: row.4,
        view_count: row.5,
        last_viewed_at: row.6,
    })
}

pub struct ThreadShareQueries;

impl ThreadShareQueries {
    pub async fn create(
        pool: &SqlitePool,
        thread_id: Uuid,
        token_hash: &str,
        expires_at: &str,
    ) -> Result<ThreadShare, sqlx::Error> {
        let id = Uuid::new_v4();
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO thread_shares
                (id, thread_id, token_hash, created_at, expires_at, revoked_at,
                 view_count, last_viewed_at)
             VALUES (?, ?, ?, ?, ?, NULL, 0, NULL)",
        )
        .bind(id.to_string())
        .bind(thread_id.to_string())
        .bind(token_hash)
        .bind(&now)
        .bind(expires_at)
        .execute(pool)
        .await?;
        Self::get(pool, id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    pub async fn get(pool: &SqlitePool, id: Uuid) -> Result<Option<ThreadShare>, sqlx::Error> {
        let row: Option<ThreadShareRow> = sqlx::query_as(
            "SELECT id, thread_id, created_at, expires_at, revoked_at, view_count, last_viewed_at
             FROM thread_shares WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(pool)
        .await?;
        Ok(row.and_then(thread_share_from_row))
    }

    /// All shares for a thread, including revoked/expired ones for the owner's
    /// management list. No token or token hash is returned.
    pub async fn list_for_thread(
        pool: &SqlitePool,
        thread_id: Uuid,
    ) -> Result<Vec<ThreadShare>, sqlx::Error> {
        let rows: Vec<ThreadShareRow> = sqlx::query_as(
            "SELECT id, thread_id, created_at, expires_at, revoked_at, view_count, last_viewed_at
             FROM thread_shares
             WHERE thread_id = ?
             ORDER BY created_at DESC",
        )
        .bind(thread_id.to_string())
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().filter_map(thread_share_from_row).collect())
    }

    /// Resolve a usable public link. Expired and revoked rows are not returned;
    /// the caller responds with the same 404 as an unknown token.
    pub async fn for_token(
        pool: &SqlitePool,
        token_hash: &str,
    ) -> Result<Option<ThreadShare>, sqlx::Error> {
        let row: Option<ThreadShareRow> = sqlx::query_as(
            "SELECT id, thread_id, created_at, expires_at, revoked_at, view_count, last_viewed_at
             FROM thread_shares
             WHERE token_hash = ? AND revoked_at IS NULL
               AND datetime(expires_at) > datetime('now')
             LIMIT 1",
        )
        .bind(token_hash)
        .fetch_optional(pool)
        .await?;
        Ok(row.and_then(thread_share_from_row))
    }

    pub async fn revoke(pool: &SqlitePool, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE thread_shares SET revoked_at = ? WHERE id = ? AND revoked_at IS NULL")
            .bind(Utc::now().to_rfc3339())
            .bind(id.to_string())
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn delete_for_thread(pool: &SqlitePool, thread_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM thread_shares WHERE thread_id = ?")
            .bind(thread_id.to_string())
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn record_view(pool: &SqlitePool, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE thread_shares
             SET view_count = view_count + 1, last_viewed_at = ?
             WHERE id = ?",
        )
        .bind(Utc::now().to_rfc3339())
        .bind(id.to_string())
        .execute(pool)
        .await?;
        Ok(())
    }
}

/// Webhook trigger queries on routines.
pub struct WebhookQueries;

pub fn hash_webhook_secret(secret: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(secret.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

impl WebhookQueries {
    /// Set (or rotate) a routine's webhook secret and enable/disable it.
    ///
    /// The lookup value stored in SQL is a SHA-256 of the token, never the
    /// token itself. The token is held in the keychain (or the owner-only DB
    /// fallback), so an incoming request can be routed without a secret query.
    pub async fn set(
        pool: &SqlitePool,
        routine_id: Uuid,
        secret: Option<&str>,
        enabled: bool,
    ) -> Result<(), sqlx::Error> {
        let secret = secret.map(str::trim).filter(|value| !value.is_empty());
        let (stored, hash) = match secret {
            None => {
                crate::secrets::delete_webhook_secret(&routine_id.to_string()).await;
                (None, None)
            }
            Some(value) => {
                let stored =
                    if crate::secrets::set_webhook_secret(&routine_id.to_string(), value).await {
                        crate::secrets::KEYCHAIN_MARKER.to_string()
                    } else {
                        value.to_string()
                    };
                (Some(stored), Some(hash_webhook_secret(value)))
            }
        };
        sqlx::query(
            "UPDATE routines SET webhook_secret = ?, webhook_secret_hash = ?, webhook_enabled = ?, updated_at = ? WHERE id = ?",
        )
        .bind(stored)
        .bind(hash)
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
        let row: Option<(Option<String>, i64)> =
            sqlx::query_as("SELECT webhook_secret, webhook_enabled FROM routines WHERE id = ?")
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
        let hash = hash_webhook_secret(secret);
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM routines WHERE webhook_secret_hash = ? AND webhook_enabled = 1",
        )
        .bind(hash)
        .fetch_optional(pool)
        .await?;
        Ok(row.and_then(|(s,)| Uuid::parse_str(&s).ok()))
    }

    /// Backfill hashes and migrate legacy plaintext tokens.
    ///
    /// Runs before the webhook listener starts. A marker whose keychain entry
    /// cannot be read is left alone; it is not a valid route until the keychain
    /// is available again, and silently hashing the marker would be wrong.
    pub async fn backfill_legacy(pool: &SqlitePool) -> Result<usize, sqlx::Error> {
        let rows: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT id, webhook_secret, webhook_secret_hash FROM routines
             WHERE webhook_secret IS NOT NULL AND webhook_secret != ''
               AND (webhook_secret_hash IS NULL OR webhook_secret_hash = '')",
        )
        .fetch_all(pool)
        .await?;
        let mut migrated = 0;
        for (id, stored, _) in rows {
            let Some(stored) = stored else {
                continue;
            };
            let secret = if stored == crate::secrets::KEYCHAIN_MARKER {
                crate::secrets::get_webhook_secret(&id).await
            } else {
                if crate::secrets::set_webhook_secret(&id, &stored).await {
                    let _ = sqlx::query("UPDATE routines SET webhook_secret = ? WHERE id = ?")
                        .bind(crate::secrets::KEYCHAIN_MARKER)
                        .bind(&id)
                        .execute(pool)
                        .await;
                }
                Some(stored)
            };
            let Some(secret) = secret else {
                continue;
            };
            sqlx::query("UPDATE routines SET webhook_secret_hash = ? WHERE id = ?")
                .bind(hash_webhook_secret(&secret))
                .bind(&id)
                .execute(pool)
                .await?;
            migrated += 1;
        }
        Ok(migrated)
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

/// Per-agent file-change journal.
///
/// Written by the runtime the moment a write succeeds, so the numbers are live
/// rather than assembled on demand from a transcript that may have reloaded.
pub struct FileChangeQueries;

/// One `file_changes` row as sqlx hands it back.
///
/// Named rather than written inline, because `row_to_domain` takes it and a
/// ten-field tuple spelled out twice is a type no reader verifies — which is
/// how the diff column would end up bound to `created_at` and every ledger row
/// in the table would carry a timestamp where its lines were supposed to be.
type FileChangeRow = (
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    String,
    i64,
    i64,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
);

impl FileChangeQueries {
    pub async fn insert(
        pool: &SqlitePool,
        change: &ravenbot_core::FileChange,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO file_changes
                (id, bot_id, run_id, thread_id, path, skill, lines_added, lines_deleted,
                 diff, snapshot_path, before_sha256, after_sha256, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(change.id.to_string())
        .bind(change.bot_id.to_string())
        .bind(change.run_id.map(|id| id.to_string()))
        .bind(change.thread_id.map(|id| id.to_string()))
        .bind(&change.path)
        .bind(&change.skill)
        .bind(change.lines_added)
        .bind(change.lines_deleted)
        .bind(&change.diff)
        .bind(&change.snapshot_path)
        .bind(&change.before_sha256)
        .bind(&change.after_sha256)
        .bind(&change.created_at)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// The most recent writes for one agent.
    ///
    /// Bounded and newest-first: the useful question is "what is it doing now",
    /// and an unbounded list would hand the UI a table that grows forever on
    /// every run. Ordered by time rather than grouped, so two writes to the same
    /// file in one run both appear — the second one is often the correction.
    ///
    /// `diff` is deliberately selected as `NULL`, which is the one rule across
    /// this table: **the list never carries lines, the single-row fetch does.**
    /// A diff can be a quarter of a megabyte, so `limit = 1000` would otherwise
    /// mean 256MB through IPC to fill a two-digit badge — and a caller cannot
    /// know that before it has asked. [`Self::diff_for`] fetches one, when
    /// someone opens one.
    pub async fn recent_for_bot(
        pool: &SqlitePool,
        bot_id: Uuid,
        limit: i64,
    ) -> Result<Vec<ravenbot_core::FileChange>, sqlx::Error> {
        let rows: Vec<FileChangeRow> = sqlx::query_as(
            "SELECT id, bot_id, run_id, thread_id, path, skill, lines_added, lines_deleted,
                    NULL AS diff, snapshot_path, before_sha256, after_sha256, created_at
             FROM file_changes WHERE bot_id = ?
             ORDER BY created_at DESC, rowid DESC
             LIMIT ?",
        )
        .bind(bot_id.to_string())
        .bind(limit)
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().filter_map(Self::row_to_domain).collect())
    }

    /// Distinct paths one run changed. This is what turns a completed work
    /// item's artifacts into facts instead of prose the agent remembered.
    pub async fn paths_for_run(
        pool: &SqlitePool,
        run_id: Uuid,
    ) -> Result<Vec<String>, sqlx::Error> {
        sqlx::query_scalar::<_, String>(
            "SELECT DISTINCT path FROM file_changes
             WHERE run_id = ? ORDER BY path ASC",
        )
        .bind(run_id.to_string())
        .fetch_all(pool)
        .await
    }

    /// Distinct files this thread's runs changed since a timestamp.
    ///
    /// Used by diagnostics: a goal round should check the files that round
    /// touched, not every file that ever lived in the conversation. The
    /// absolute snapshot path wins when one exists because that is the path
    /// the language server will see.
    pub async fn paths_for_thread_since(
        pool: &SqlitePool,
        thread_id: Uuid,
        since: &str,
    ) -> Result<Vec<String>, sqlx::Error> {
        sqlx::query_scalar::<_, String>(
            "SELECT DISTINCT COALESCE(snapshot_path, path) AS target
             FROM file_changes
             WHERE thread_id = ? AND created_at >= ?
               AND COALESCE(snapshot_path, path) IS NOT NULL
             LIMIT 200",
        )
        .bind(thread_id.to_string())
        .bind(since)
        .fetch_all(pool)
        .await
    }

    /// The lines behind one ledger row.
    ///
    /// `None` means "there is nothing to expand into" — a write whose content
    /// could not be lined up (binary, past the diff cap) or one recorded
    /// before this column existed. Both are honest absences, and the UI shows
    /// a sentence rather than opening onto a blank pane.
    pub async fn diff_for(pool: &SqlitePool, id: Uuid) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar::<_, Option<String>>("SELECT diff FROM file_changes WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(pool)
            .await
            .map(|row: Option<Option<String>>| row.flatten())
    }

    /// Totals for every agent that has ever changed a file, in one query.
    ///
    /// `COUNT(DISTINCT path)` rather than `COUNT(*)` for the file count: an
    /// agent that wrote the same file six times in a loop has still changed one
    /// file, and reporting six would make a fix-up loop look like a large
    /// change. Agents with no changes are absent rather than zeroed — the
    /// caller decides whether a missing agent means "nothing yet" or "not
    /// started", which is context this table does not have.
    pub async fn totals(
        pool: &SqlitePool,
    ) -> Result<Vec<(Uuid, ravenbot_core::FileChangeTotals)>, sqlx::Error> {
        let rows: Vec<(String, i64, i64, i64, i64)> = sqlx::query_as(
            "SELECT bot_id,
                    COUNT(DISTINCT path),
                    COUNT(*),
                    COALESCE(SUM(lines_added), 0),
                    COALESCE(SUM(lines_deleted), 0)
             FROM file_changes
             GROUP BY bot_id
             ORDER BY COALESCE(SUM(lines_added), 0) + COALESCE(SUM(lines_deleted), 0) DESC",
        )
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|(bot_id, files, writes, added, deleted)| {
                Uuid::parse_str(&bot_id).ok().map(|id| {
                    (
                        id,
                        ravenbot_core::FileChangeTotals {
                            files,
                            writes,
                            lines_added: added,
                            lines_deleted: deleted,
                        },
                    )
                })
            })
            .collect())
    }

    /// One ledger row by id, with its diff.
    pub async fn get(
        pool: &SqlitePool,
        id: Uuid,
    ) -> Result<Option<ravenbot_core::FileChange>, sqlx::Error> {
        let row: Option<FileChangeRow> = sqlx::query_as(
            "SELECT id, bot_id, run_id, thread_id, path, skill, lines_added, lines_deleted,
                    diff, snapshot_path, before_sha256, after_sha256, created_at
             FROM file_changes WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(pool)
        .await?;
        Ok(row.and_then(Self::row_to_domain))
    }

    /// Every write one run made, in write order.
    pub async fn for_run(
        pool: &SqlitePool,
        run_id: Uuid,
    ) -> Result<Vec<ravenbot_core::FileChange>, sqlx::Error> {
        let rows: Vec<FileChangeRow> = sqlx::query_as(
            "SELECT id, bot_id, run_id, thread_id, path, skill, lines_added, lines_deleted,
                    NULL AS diff, snapshot_path, before_sha256, after_sha256, created_at
             FROM file_changes WHERE run_id = ?
             ORDER BY created_at ASC, rowid ASC",
        )
        .bind(run_id.to_string())
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().filter_map(Self::row_to_domain).collect())
    }

    /// The newest write this run made to one absolute path.
    pub async fn latest_for_run_path(
        pool: &SqlitePool,
        run_id: Uuid,
        path: &str,
    ) -> Result<Option<ravenbot_core::FileChange>, sqlx::Error> {
        let row: Option<FileChangeRow> = sqlx::query_as(
            "SELECT id, bot_id, run_id, thread_id, path, skill, lines_added, lines_deleted,
                    NULL AS diff, snapshot_path, before_sha256, after_sha256, created_at
             FROM file_changes
             WHERE run_id = ? AND snapshot_path = ?
             ORDER BY created_at DESC, rowid DESC LIMIT 1",
        )
        .bind(run_id.to_string())
        .bind(path)
        .fetch_optional(pool)
        .await?;
        Ok(row.and_then(Self::row_to_domain))
    }

    /// The first write to a path after `after`, optionally ignoring one run.
    ///
    /// Used for two conflicts: a later write in another run blocks a whole-run
    /// revert, and a later write in any run blocks a single-change revert.
    pub async fn first_after_path(
        pool: &SqlitePool,
        path: &str,
        after: &str,
        exclude_run: Option<Uuid>,
    ) -> Result<Option<ravenbot_core::FileChange>, sqlx::Error> {
        let row: Option<FileChangeRow> = sqlx::query_as(
            "SELECT id, bot_id, run_id, thread_id, path, skill, lines_added, lines_deleted,
                    NULL AS diff, snapshot_path, before_sha256, after_sha256, created_at
             FROM file_changes
             WHERE snapshot_path = ? AND created_at > ?
               AND (? IS NULL OR run_id IS NOT ?)
             ORDER BY created_at ASC, rowid ASC LIMIT 1",
        )
        .bind(path)
        .bind(after)
        .bind(exclude_run.map(|id| id.to_string()))
        .bind(exclude_run.map(|id| id.to_string()))
        .fetch_optional(pool)
        .await?;
        Ok(row.and_then(Self::row_to_domain))
    }

    /// One tuple in, one domain row out.
    ///
    /// Takes the row rather than nine loose fields: sqlx hands back a tuple and
    /// unpacking it at the call site is the same nine names twice, which is how
    /// a field gets swapped with its neighbour and nobody notices until the
    /// ledger reports lines in the wrong column.
    fn row_to_domain(r: FileChangeRow) -> Option<ravenbot_core::FileChange> {
        Some(ravenbot_core::FileChange {
            id: Uuid::parse_str(&r.0).ok()?,
            bot_id: Uuid::parse_str(&r.1).ok()?,
            run_id: r.2.as_deref().and_then(|s| Uuid::parse_str(s).ok()),
            thread_id: r.3.as_deref().and_then(|s| Uuid::parse_str(s).ok()),
            path: r.4,
            skill: r.5,
            lines_added: r.6,
            lines_deleted: r.7,
            diff: r.8,
            snapshot_path: r.9,
            before_sha256: r.10,
            after_sha256: r.11,
            created_at: r.12,
        })
    }
}

/// One conversation's opt-in Git worktree.
#[derive(Debug, Clone, Serialize)]
pub struct ThreadWorktreeRecord {
    pub thread_id: String,
    pub original_root: String,
    pub worktree_path: String,
    /// The directory the agent itself used (a subdirectory of the checkout
    /// when the configured project folder was nested).
    pub agent_path: String,
    pub base_commit: String,
    /// The most recent run that used this checkout.
    pub last_run_id: Option<String>,
    pub status: String,
    pub note: String,
    pub created_at: String,
    pub updated_at: String,
    pub finished_at: Option<String>,
}

pub struct ThreadWorktreeQueries;

type ThreadWorktreeRow = (
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    String,
    String,
    String,
    String,
    Option<String>,
);

fn thread_worktree_from_row(row: ThreadWorktreeRow) -> ThreadWorktreeRecord {
    ThreadWorktreeRecord {
        thread_id: row.0,
        original_root: row.1,
        worktree_path: row.2,
        agent_path: row.3,
        base_commit: row.4,
        last_run_id: row.5,
        status: row.6,
        note: row.7,
        created_at: row.8,
        updated_at: row.9,
        finished_at: row.10,
    }
}

impl ThreadWorktreeQueries {
    const COLUMNS: &'static str = "thread_id, original_root, worktree_path, agent_path, \
         base_commit, last_run_id, status, note, created_at, updated_at, finished_at";

    /// The checkout still accepting edits, if this conversation has one.
    pub async fn active_for_thread(
        pool: &SqlitePool,
        thread_id: Uuid,
    ) -> Result<Option<ThreadWorktreeRecord>, sqlx::Error> {
        let row: Option<ThreadWorktreeRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {} FROM thread_worktrees WHERE thread_id = ? AND status = 'active'",
            Self::COLUMNS
        )))
        .bind(thread_id.to_string())
        .fetch_optional(pool)
        .await?;
        Ok(row.map(thread_worktree_from_row))
    }

    /// The most recent checkout for a conversation, including a finished one.
    pub async fn latest_for_thread(
        pool: &SqlitePool,
        thread_id: Uuid,
    ) -> Result<Option<ThreadWorktreeRecord>, sqlx::Error> {
        let row: Option<ThreadWorktreeRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {} FROM thread_worktrees WHERE thread_id = ?
             ORDER BY updated_at DESC LIMIT 1",
            Self::COLUMNS
        )))
        .bind(thread_id.to_string())
        .fetch_optional(pool)
        .await?;
        Ok(row.map(thread_worktree_from_row))
    }

    /// Start a new active checkout for a conversation.
    ///
    /// A previous applied/discarded row is replaced; history is represented by
    /// the run ledger and the worktree's own committed base, not by keeping
    /// every checkout row forever.
    pub async fn activate(
        pool: &SqlitePool,
        thread_id: Uuid,
        original_root: &str,
        worktree_path: &str,
        agent_path: &str,
        base_commit: &str,
        run_id: Uuid,
    ) -> Result<ThreadWorktreeRecord, sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO thread_worktrees
                (thread_id, original_root, worktree_path, agent_path, base_commit,
                 last_run_id, status, note, created_at, updated_at, finished_at)
             VALUES (?, ?, ?, ?, ?, ?, 'active', '', ?, ?, NULL)
             ON CONFLICT(thread_id) DO UPDATE SET
                original_root = excluded.original_root,
                worktree_path = excluded.worktree_path,
                agent_path = excluded.agent_path,
                base_commit = excluded.base_commit,
                last_run_id = excluded.last_run_id,
                status = 'active',
                note = '',
                created_at = excluded.created_at,
                updated_at = excluded.updated_at,
                finished_at = NULL",
        )
        .bind(thread_id.to_string())
        .bind(original_root)
        .bind(worktree_path)
        .bind(agent_path)
        .bind(base_commit)
        .bind(run_id.to_string())
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await?;
        Self::active_for_thread(pool, thread_id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)
    }

    /// Record that an existing checkout is serving a new run.
    pub async fn touch_run(
        pool: &SqlitePool,
        thread_id: Uuid,
        run_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let changed = sqlx::query(
            "UPDATE thread_worktrees SET last_run_id = ?, updated_at = ?
             WHERE thread_id = ? AND status = 'active'",
        )
        .bind(run_id.to_string())
        .bind(Utc::now().to_rfc3339())
        .bind(thread_id.to_string())
        .execute(pool)
        .await?
        .rows_affected();
        Ok(changed == 1)
    }

    /// Close an active checkout after it was applied or discarded.
    pub async fn finish(
        pool: &SqlitePool,
        thread_id: Uuid,
        status: &str,
        note: &str,
    ) -> Result<bool, sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let changed = sqlx::query(
            "UPDATE thread_worktrees
             SET status = ?, note = ?, updated_at = ?, finished_at = ?
             WHERE thread_id = ? AND status = 'active'",
        )
        .bind(status)
        .bind(note)
        .bind(&now)
        .bind(&now)
        .bind(thread_id.to_string())
        .execute(pool)
        .await?
        .rows_affected();
        Ok(changed == 1)
    }
}

/// One run's revert snapshot.
///
/// Created lazily on the first file-writing tool call. `root` is the
/// canonical workspace root; entries may still sit in another configured
/// folder of the same run, so each entry carries its own root too.
#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceSnapshotRecord {
    pub id: String,
    pub run_id: String,
    pub thread_id: Option<String>,
    pub root: String,
    pub kind: String,
    /// False when a file/size cap stopped the walk. A partial snapshot cannot
    /// prove a path was absent, so revert refuses it instead of deleting files
    /// it never saw.
    pub complete: bool,
    pub created_at: String,
}

/// One path in a run snapshot.
///
/// `existed = false` means the run created the path; reverting deletes it.
/// `existed = true` with `blob = None` means the before-image could not be
/// stored (too large or unreadable) and the path is reported as unavailable
/// rather than restored as an empty file.
#[derive(Debug, Clone, Serialize)]
pub struct SnapshotEntryRecord {
    pub snapshot_id: String,
    pub path: String,
    pub root: String,
    pub blob: Option<String>,
    pub existed: bool,
    pub sha256: Option<String>,
    pub mode: Option<i64>,
    pub size: i64,
    pub note: String,
    pub created_at: String,
}

pub struct SnapshotQueries;

impl SnapshotQueries {
    /// Return the run's snapshot, creating it if this is the first write.
    ///
    /// The unique index on `run_id` makes the insert race-safe: two parallel
    /// nodes that touch a file at the same moment still share one snapshot.
    pub async fn ensure_for_run(
        pool: &SqlitePool,
        run_id: Uuid,
        thread_id: Uuid,
        root: &str,
    ) -> Result<WorkspaceSnapshotRecord, sqlx::Error> {
        if let Some(existing) = Self::for_run(pool, run_id).await? {
            return Ok(existing);
        }
        let id = Uuid::new_v4();
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT OR IGNORE INTO workspace_snapshots
                (id, run_id, thread_id, root, kind, complete, created_at)
             VALUES (?, ?, ?, ?, 'files', 1, ?)",
        )
        .bind(id.to_string())
        .bind(run_id.to_string())
        .bind(thread_id.to_string())
        .bind(root)
        .bind(&now)
        .execute(pool)
        .await?;
        Self::for_run(pool, run_id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)
    }

    /// Mark a snapshot as incomplete.
    ///
    /// A partial walk cannot prove a path was absent, so revert refuses it
    /// rather than deleting a file it never saw.
    pub async fn mark_partial(pool: &SqlitePool, id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE workspace_snapshots SET complete = 0 WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn for_run(
        pool: &SqlitePool,
        run_id: Uuid,
    ) -> Result<Option<WorkspaceSnapshotRecord>, sqlx::Error> {
        let row: Option<(String, String, Option<String>, String, String, bool, String)> =
            sqlx::query_as(
                "SELECT id, run_id, thread_id, root, kind, complete, created_at
                 FROM workspace_snapshots WHERE run_id = ?",
            )
            .bind(run_id.to_string())
            .fetch_optional(pool)
            .await?;
        Ok(row.map(|row| WorkspaceSnapshotRecord {
            id: row.0,
            run_id: row.1,
            thread_id: row.2,
            root: row.3,
            kind: row.4,
            complete: row.5,
            created_at: row.6,
        }))
    }

    pub async fn get(
        pool: &SqlitePool,
        id: Uuid,
    ) -> Result<Option<WorkspaceSnapshotRecord>, sqlx::Error> {
        let row: Option<(String, String, Option<String>, String, String, bool, String)> =
            sqlx::query_as(
                "SELECT id, run_id, thread_id, root, kind, complete, created_at
                 FROM workspace_snapshots WHERE id = ?",
            )
            .bind(id.to_string())
            .fetch_optional(pool)
            .await?;
        Ok(row.map(|row| WorkspaceSnapshotRecord {
            id: row.0,
            run_id: row.1,
            thread_id: row.2,
            root: row.3,
            kind: row.4,
            complete: row.5,
            created_at: row.6,
        }))
    }

    /// Record the before-image for one path, once per run.
    ///
    /// `INSERT OR IGNORE` is the important part: the second write to a path in
    /// the same run must not replace the state from before the run. Reverting
    /// a turn means going back to the version the user started with.
    pub async fn insert_entry_if_absent(
        pool: &SqlitePool,
        entry: &SnapshotEntryRecord,
    ) -> Result<bool, sqlx::Error> {
        let changed = sqlx::query(
            "INSERT OR IGNORE INTO workspace_snapshot_entries
                (snapshot_id, path, root, blob, existed, sha256, mode, size, note, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&entry.snapshot_id)
        .bind(&entry.path)
        .bind(&entry.root)
        .bind(&entry.blob)
        .bind(entry.existed)
        .bind(&entry.sha256)
        .bind(entry.mode)
        .bind(entry.size)
        .bind(&entry.note)
        .bind(&entry.created_at)
        .execute(pool)
        .await?
        .rows_affected();
        Ok(changed == 1)
    }

    /// Insert a whole walk's entries in one transaction.
    ///
    /// A workspace snapshot can name tens of thousands of files; one autocommit
    /// round trip per file would turn a safety feature into a visible pause.
    pub async fn insert_entries_if_absent(
        pool: &SqlitePool,
        entries: &[SnapshotEntryRecord],
    ) -> Result<(), sqlx::Error> {
        if entries.is_empty() {
            return Ok(());
        }
        let mut tx = pool.begin().await?;
        for entry in entries {
            sqlx::query(
                "INSERT OR IGNORE INTO workspace_snapshot_entries
                    (snapshot_id, path, root, blob, existed, sha256, mode, size, note, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&entry.snapshot_id)
            .bind(&entry.path)
            .bind(&entry.root)
            .bind(&entry.blob)
            .bind(entry.existed)
            .bind(&entry.sha256)
            .bind(entry.mode)
            .bind(entry.size)
            .bind(&entry.note)
            .bind(&entry.created_at)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Delete snapshots older than `cutoff` and their manifests.
    ///
    /// The `file_changes` ledger is untouched: it is history. A revert after
    /// retention simply says the before-image is gone.
    pub async fn prune_older_than(pool: &SqlitePool, cutoff: &str) -> Result<u64, sqlx::Error> {
        sqlx::query(
            "DELETE FROM workspace_snapshot_entries
             WHERE snapshot_id IN (SELECT id FROM workspace_snapshots WHERE created_at < ?)",
        )
        .bind(cutoff)
        .execute(pool)
        .await?;
        let deleted = sqlx::query("DELETE FROM workspace_snapshots WHERE created_at < ?")
            .bind(cutoff)
            .execute(pool)
            .await?
            .rows_affected();
        Ok(deleted)
    }

    /// Every blob still named by a manifest, for garbage collection.
    pub async fn referenced_blobs(pool: &SqlitePool) -> Result<Vec<String>, sqlx::Error> {
        sqlx::query_scalar::<_, String>(
            "SELECT DISTINCT sha256 FROM workspace_snapshot_entries WHERE sha256 IS NOT NULL",
        )
        .fetch_all(pool)
        .await
    }

    pub async fn entries(
        pool: &SqlitePool,
        snapshot_id: &str,
    ) -> Result<Vec<SnapshotEntryRecord>, sqlx::Error> {
        let rows: Vec<(
            String,
            String,
            String,
            Option<String>,
            bool,
            Option<String>,
            Option<i64>,
            i64,
            String,
            String,
        )> = sqlx::query_as(
            "SELECT snapshot_id, path, root, blob, existed, sha256, mode, size, note, created_at
             FROM workspace_snapshot_entries
             WHERE snapshot_id = ?
             ORDER BY path ASC",
        )
        .bind(snapshot_id)
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| SnapshotEntryRecord {
                snapshot_id: row.0,
                path: row.1,
                root: row.2,
                blob: row.3,
                existed: row.4,
                sha256: row.5,
                mode: row.6,
                size: row.7,
                note: row.8,
                created_at: row.9,
            })
            .collect())
    }

    pub async fn entry_for_path(
        pool: &SqlitePool,
        snapshot_id: &str,
        path: &str,
    ) -> Result<Option<SnapshotEntryRecord>, sqlx::Error> {
        let row: Option<(
            String,
            String,
            String,
            Option<String>,
            bool,
            Option<String>,
            Option<i64>,
            i64,
            String,
            String,
        )> = sqlx::query_as(
            "SELECT snapshot_id, path, root, blob, existed, sha256, mode, size, note, created_at
             FROM workspace_snapshot_entries
             WHERE snapshot_id = ? AND path = ?",
        )
        .bind(snapshot_id)
        .bind(path)
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|row| SnapshotEntryRecord {
            snapshot_id: row.0,
            path: row.1,
            root: row.2,
            blob: row.3,
            existed: row.4,
            sha256: row.5,
            mode: row.6,
            size: row.7,
            note: row.8,
            created_at: row.9,
        }))
    }
}

#[cfg(test)]
mod file_change_tests {
    use super::*;
    use crate::migrations;

    /// An in-memory database, on one connection.
    ///
    /// `sqlite::memory:` is per-*connection*: a pool would give every query its
    /// own empty database and the test would pass while asserting nothing. One
    /// connection is also what the app itself effectively gets for a short
    /// ledger read, so this is not a different shape of thing.
    async fn db() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        migrations::run(&pool).await.expect("migrations");
        pool
    }

    fn change(
        bot: Uuid,
        path: &str,
        added: i64,
        deleted: i64,
        diff: Option<&str>,
    ) -> ravenbot_core::FileChange {
        ravenbot_core::FileChange::new(bot, None, None, path, "file_write", added, deleted)
            .with_diff(diff.map(str::to_string))
    }

    #[tokio::test]
    async fn thread_paths_are_scoped_to_the_turn() {
        let pool = db().await;
        let bot = Uuid::new_v4();
        let thread = Uuid::new_v4();
        let mut old = change(bot, "old.rs", 1, 0, None);
        old.thread_id = Some(thread);
        old.created_at = "2026-01-01T00:00:00Z".to_string();
        let mut new = change(bot, "new.rs", 1, 0, None);
        new.thread_id = Some(thread);
        new.created_at = "2026-01-01T00:10:00Z".to_string();
        FileChangeQueries::insert(&pool, &old).await.expect("old");
        FileChangeQueries::insert(&pool, &new).await.expect("new");

        let paths =
            FileChangeQueries::paths_for_thread_since(&pool, thread, "2026-01-01T00:05:00Z")
                .await
                .expect("paths");
        assert_eq!(paths, vec!["new.rs".to_string()]);
        assert!(FileChangeQueries::paths_for_thread_since(
            &pool,
            Uuid::new_v4(),
            "2026-01-01T00:00:00Z",
        )
        .await
        .expect("other thread")
        .is_empty());
    }

    #[tokio::test]
    async fn a_list_carries_counts_but_not_lines() {
        let pool = db().await;
        let bot = Uuid::new_v4();
        let with_diff = change(
            bot,
            "src/lib.rs",
            3,
            1,
            Some("--- a/src/lib.rs\n+++ b/src/lib.rs\n"),
        );
        let without = change(bot, "README.md", 1, 0, None);

        FileChangeQueries::insert(&pool, &with_diff)
            .await
            .expect("insert 1");
        FileChangeQueries::insert(&pool, &without)
            .await
            .expect("insert 2");

        let listed = FileChangeQueries::recent_for_bot(&pool, bot, 10)
            .await
            .expect("list");
        assert_eq!(listed.len(), 2);
        // The rule the view depends on: asking for "Sam's changes" must not
        // also hand over two hundred megabytes of lines nobody opened.
        assert!(
            listed.iter().all(|c| c.diff.is_none()),
            "the list must not carry diffs: {listed:?}"
        );
        // …but nothing else is lost on the way through.
        let first = listed
            .iter()
            .find(|c| c.id == with_diff.id)
            .expect("row survived");
        assert_eq!(first.path, "src/lib.rs");
        assert_eq!((first.lines_added, first.lines_deleted), (3, 1));

        // The lines are still reachable, one row at a time.
        let diff = FileChangeQueries::diff_for(&pool, with_diff.id)
            .await
            .expect("diff_for");
        assert_eq!(
            diff.as_deref(),
            Some("--- a/src/lib.rs\n+++ b/src/lib.rs\n")
        );

        // And "no diff" reads as absence, not as an empty string the UI would
        // render as an expandable row that opens onto nothing.
        assert!(FileChangeQueries::diff_for(&pool, without.id)
            .await
            .expect("diff_for")
            .is_none());
        assert!(FileChangeQueries::diff_for(&pool, Uuid::new_v4())
            .await
            .expect("diff_for")
            .is_none());
    }

    #[tokio::test]
    async fn totals_agree_with_the_rows_that_produced_them() {
        let pool = db().await;
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        FileChangeQueries::insert(&pool, &change(a, "one.rs", 10, 2, Some("+10 -2\n")))
            .await
            .expect("insert");
        FileChangeQueries::insert(&pool, &change(a, "two.rs", 0, 5, None))
            .await
            .expect("insert");
        FileChangeQueries::insert(&pool, &change(b, "other.rs", 1, 1, None))
            .await
            .expect("insert");

        let mut totals: Vec<(Uuid, ravenbot_core::FileChangeTotals)> =
            FileChangeQueries::totals(&pool).await.expect("totals");
        totals.sort_by_key(|(id, _)| *id);
        let mut expected = vec![(a, (2, 10, 7)), (b, (1, 1, 1))];
        expected.sort_by_key(|(id, _)| *id);
        for ((got_id, got), (want_id, want)) in totals.iter().zip(&expected) {
            assert_eq!(*got_id, *want_id);
            assert_eq!(
                (got.files, got.lines_added, got.lines_deleted),
                *want,
                "totals for {got_id}"
            );
        }
    }
}

/// Persisted emoji reactions.
///
/// A reaction is a toggle, not a counter: the composite primary key
/// `(message_id, emoji)` means adding twice is impossible and removing is the
/// same row lookup. The local-first UI has one human, so there is no per-user
/// dimension to store.
pub struct ReactionQueries;

impl ReactionQueries {
    /// Every reaction on the messages in one conversation thread, oldest first.
    pub async fn list_for_thread(
        pool: &SqlitePool,
        thread_id: Uuid,
    ) -> Result<Vec<ravenbot_core::MessageReaction>, sqlx::Error> {
        let rows: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT r.message_id, r.emoji, r.created_at
             FROM message_reactions r
             JOIN messages m ON m.id = r.message_id
             WHERE m.thread_id = ?
             ORDER BY r.created_at ASC, r.rowid ASC",
        )
        .bind(thread_id.to_string())
        .fetch_all(pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(message_id, emoji, created_at)| ravenbot_core::MessageReaction {
                    message_id: Uuid::parse_str(&message_id).unwrap_or_default(),
                    emoji,
                    created_at: chrono::DateTime::parse_from_rfc3339(&created_at)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now()),
                },
            )
            .collect())
    }

    /// Add the reaction if it is absent, remove it if it is present, and return
    /// the message's reactions in stable order.
    pub async fn toggle(
        pool: &SqlitePool,
        message_id: Uuid,
        emoji: &str,
    ) -> Result<Vec<String>, sqlx::Error> {
        let removed =
            sqlx::query("DELETE FROM message_reactions WHERE message_id = ? AND emoji = ?")
                .bind(message_id.to_string())
                .bind(emoji)
                .execute(pool)
                .await?
                .rows_affected();

        if removed == 0 {
            sqlx::query("INSERT INTO message_reactions (message_id, emoji) VALUES (?, ?)")
                .bind(message_id.to_string())
                .bind(emoji)
                .execute(pool)
                .await?;
        }

        sqlx::query_scalar(
            "SELECT emoji FROM message_reactions
             WHERE message_id = ?
             ORDER BY created_at ASC, rowid ASC",
        )
        .bind(message_id.to_string())
        .fetch_all(pool)
        .await
    }
}

/// Shared channel timeline messages.
pub struct ChannelMessageQueries;

type ChannelMessageRow = (
    String,
    String,
    String,
    Option<String>,
    String,
    String,
    Option<String>,
    String,
);

impl ChannelMessageQueries {
    fn row_to_domain(row: ChannelMessageRow) -> ravenbot_core::ChannelMessage {
        ravenbot_core::ChannelMessage {
            id: Uuid::parse_str(&row.0).unwrap_or_default(),
            channel_id: Uuid::parse_str(&row.1).unwrap_or_default(),
            author_kind: ravenbot_core::ChannelAuthorKind::parse(&row.2),
            author_id: row
                .3
                .as_deref()
                .and_then(|value| Uuid::parse_str(value).ok()),
            author_name: row.4,
            content: row.5,
            reply_to_id: row
                .6
                .as_deref()
                .and_then(|value| Uuid::parse_str(value).ok()),
            created_at: chrono::DateTime::parse_from_rfc3339(&row.7)
                .map(|value| value.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
        }
    }

    const COLS: &'static str =
        "id, channel_id, author_kind, author_id, author_name, content, reply_to_id, created_at";

    /// The newest `limit` messages, returned oldest-first so a timeline can
    /// append without reversing in the renderer.
    pub async fn list_recent(
        pool: &SqlitePool,
        channel_id: Uuid,
        limit: i64,
    ) -> Result<Vec<ravenbot_core::ChannelMessage>, sqlx::Error> {
        let limit = limit.clamp(1, 500);
        let rows: Vec<ChannelMessageRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {} FROM channel_messages WHERE channel_id = ? ORDER BY created_at DESC LIMIT ?",
            Self::COLS
        )))
        .bind(channel_id.to_string())
        .bind(limit)
        .fetch_all(pool)
        .await?;
        let mut messages: Vec<_> = rows.into_iter().map(Self::row_to_domain).collect();
        messages.reverse();
        Ok(messages)
    }

    pub async fn insert(
        pool: &SqlitePool,
        message: &ravenbot_core::ChannelMessage,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO channel_messages (id, channel_id, author_kind, author_id, author_name, content, reply_to_id, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(message.id.to_string())
        .bind(message.channel_id.to_string())
        .bind(message.author_kind.as_str())
        .bind(message.author_id.map(|value| value.to_string()))
        .bind(&message.author_name)
        .bind(&message.content)
        .bind(message.reply_to_id.map(|value| value.to_string()))
        .bind(message.created_at.to_rfc3339())
        .execute(pool)
        .await?;
        Ok(())
    }
}

/// Channel tasks and their ownership transitions.
pub struct ChannelTaskQueries;

type ChannelTaskRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    String,
    Option<String>,
);

impl ChannelTaskQueries {
    fn row_to_domain(row: ChannelTaskRow) -> ravenbot_core::ChannelTask {
        let parse_dt = |value: &str| {
            chrono::DateTime::parse_from_rfc3339(value)
                .map(|date| date.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now())
        };
        ravenbot_core::ChannelTask {
            id: Uuid::parse_str(&row.0).unwrap_or_default(),
            channel_id: Uuid::parse_str(&row.1).unwrap_or_default(),
            title: row.2,
            description: row.3,
            owner_bot_id: Uuid::parse_str(&row.4).unwrap_or_default(),
            state: ravenbot_core::ChannelTaskState::parse(&row.5),
            created_by: row.6,
            source_thread_id: row
                .7
                .as_deref()
                .and_then(|value| Uuid::parse_str(value).ok()),
            result: row.8,
            created_at: parse_dt(&row.9),
            updated_at: parse_dt(&row.10),
            archived_at: row.11.as_deref().map(parse_dt),
        }
    }

    const COLS: &'static str = "id, channel_id, title, description, owner_bot_id, state, created_by, source_thread_id, result, created_at, updated_at, archived_at";

    pub async fn list(
        pool: &SqlitePool,
        channel_id: Uuid,
        include_archived: bool,
    ) -> Result<Vec<ravenbot_core::ChannelTask>, sqlx::Error> {
        let sql = if include_archived {
            format!(
                "SELECT {} FROM channel_tasks WHERE channel_id = ? ORDER BY created_at DESC",
                Self::COLS
            )
        } else {
            format!(
                "SELECT {} FROM channel_tasks WHERE channel_id = ? AND state != 'archived' ORDER BY created_at DESC",
                Self::COLS
            )
        };
        let rows: Vec<ChannelTaskRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(channel_id.to_string())
            .fetch_all(pool)
            .await?;
        Ok(rows.into_iter().map(Self::row_to_domain).collect())
    }

    pub async fn get(
        pool: &SqlitePool,
        task_id: Uuid,
    ) -> Result<Option<ravenbot_core::ChannelTask>, sqlx::Error> {
        let row: Option<ChannelTaskRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {} FROM channel_tasks WHERE id = ?",
            Self::COLS
        )))
        .bind(task_id.to_string())
        .fetch_optional(pool)
        .await?;
        Ok(row.map(Self::row_to_domain))
    }

    pub async fn create(
        pool: &SqlitePool,
        task: &ravenbot_core::ChannelTask,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO channel_tasks (id, channel_id, title, description, owner_bot_id, state, created_by, source_thread_id, result, created_at, updated_at, archived_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(task.id.to_string())
        .bind(task.channel_id.to_string())
        .bind(&task.title)
        .bind(&task.description)
        .bind(task.owner_bot_id.to_string())
        .bind(task.state.as_str())
        .bind(&task.created_by)
        .bind(task.source_thread_id.map(|value| value.to_string()))
        .bind(&task.result)
        .bind(task.created_at.to_rfc3339())
        .bind(task.updated_at.to_rfc3339())
        .bind(task.archived_at.map(|value| value.to_rfc3339()))
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Atomically claim a task for execution. The `WHERE state IN (...)` is the
    /// one-owner rule: two workers can call this at once and only one update
    /// changes a row.
    pub async fn claim(pool: &SqlitePool, task_id: Uuid) -> Result<bool, sqlx::Error> {
        let now = chrono::Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE channel_tasks SET state = 'running', updated_at = ? WHERE id = ? AND state IN ('pending', 'paused')",
        )
        .bind(&now)
        .bind(task_id.to_string())
        .execute(pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn set_thread(
        pool: &SqlitePool,
        task_id: Uuid,
        thread_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE channel_tasks SET source_thread_id = ?, updated_at = ? WHERE id = ?")
            .bind(thread_id.to_string())
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(task_id.to_string())
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Finish a running task. A task that is not running cannot be finished:
    /// that would turn a paused/archived row into a result without a run.
    pub async fn finish(
        pool: &SqlitePool,
        task_id: Uuid,
        success: bool,
        result: Option<&str>,
    ) -> Result<bool, sqlx::Error> {
        let state = if success { "done" } else { "failed" };
        let outcome = sqlx::query(
            "UPDATE channel_tasks SET state = ?, result = ?, updated_at = ? WHERE id = ? AND state = 'running'",
        )
        .bind(state)
        .bind(result)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(task_id.to_string())
        .execute(pool)
        .await?;
        Ok(outcome.rows_affected() == 1)
    }

    /// Apply a user-visible transition and record it. Each branch has a state
    /// guard in its `UPDATE`, so a task cannot be started from archived, paused
    /// from pending, or reassigned while it is running.
    pub async fn apply(
        pool: &SqlitePool,
        task_id: Uuid,
        action: &str,
        actor: &str,
        new_owner: Option<Uuid>,
        note: Option<String>,
    ) -> Result<ravenbot_core::ChannelTask, String> {
        let task = Self::get(pool, task_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Channel task not found.".to_string())?;
        let now = chrono::Utc::now().to_rfc3339();
        let (changed, to_owner) = match action {
            "start" => {
                if task.state == ravenbot_core::ChannelTaskState::Archived {
                    return Err("Archived tasks must be restored before they can run.".to_string());
                }
                let changed = Self::claim(pool, task_id)
                    .await
                    .map_err(|error| error.to_string())?;
                (changed, Some(task.owner_bot_id))
            }
            "pause" | "stop" => {
                let outcome = sqlx::query(
                    "UPDATE channel_tasks SET state = 'paused', updated_at = ? WHERE id = ? AND state = 'running'",
                )
                .bind(&now)
                .bind(task_id.to_string())
                .execute(pool)
                .await
                .map_err(|error| error.to_string())?;
                (outcome.rows_affected() == 1, Some(task.owner_bot_id))
            }
            "resume" => {
                let outcome = sqlx::query(
                    "UPDATE channel_tasks SET state = 'pending', updated_at = ? WHERE id = ? AND state = 'paused'",
                )
                .bind(&now)
                .bind(task_id.to_string())
                .execute(pool)
                .await
                .map_err(|error| error.to_string())?;
                (outcome.rows_affected() == 1, Some(task.owner_bot_id))
            }
            "reassign" => {
                let owner = new_owner.ok_or_else(|| "A new owner is required.".to_string())?;
                if task.state == ravenbot_core::ChannelTaskState::Running {
                    return Err(
                        "Stop the task before reassigning it, so two agents never own one run."
                            .to_string(),
                    );
                }
                let outcome = sqlx::query(
                    "UPDATE channel_tasks SET owner_bot_id = ?, updated_at = ? WHERE id = ? AND state IN ('pending', 'paused')",
                )
                .bind(owner.to_string())
                .bind(&now)
                .bind(task_id.to_string())
                .execute(pool)
                .await
                .map_err(|error| error.to_string())?;
                (outcome.rows_affected() == 1, Some(owner))
            }
            "archive" => {
                if task.state == ravenbot_core::ChannelTaskState::Running {
                    return Err("Stop the task before archiving it.".to_string());
                }
                let outcome = sqlx::query(
                    "UPDATE channel_tasks SET state = 'archived', archived_at = ?, updated_at = ? WHERE id = ? AND state != 'archived'",
                )
                .bind(&now)
                .bind(&now)
                .bind(task_id.to_string())
                .execute(pool)
                .await
                .map_err(|error| error.to_string())?;
                (outcome.rows_affected() == 1, Some(task.owner_bot_id))
            }
            "restore" => {
                let outcome = sqlx::query(
                    "UPDATE channel_tasks SET state = 'paused', archived_at = NULL, updated_at = ? WHERE id = ? AND state = 'archived'",
                )
                .bind(&now)
                .bind(task_id.to_string())
                .execute(pool)
                .await
                .map_err(|error| error.to_string())?;
                (outcome.rows_affected() == 1, Some(task.owner_bot_id))
            }
            "finish" | "fail" => {
                let success = action == "finish";
                let changed = Self::finish(pool, task_id, success, note.as_deref())
                    .await
                    .map_err(|error| error.to_string())?;
                (changed, Some(task.owner_bot_id))
            }
            other => return Err(format!("Unknown channel task action: {other}")),
        };
        if !changed {
            return Err(format!(
                "The task is {} and cannot '{}' from there.",
                task.state.as_str(),
                action
            ));
        }
        Self::insert_event(
            pool,
            &ravenbot_core::ChannelTaskEvent {
                id: Uuid::new_v4(),
                task_id,
                kind: action.to_string(),
                actor: actor.to_string(),
                from_owner: Some(task.owner_bot_id),
                to_owner,
                note,
                created_at: chrono::Utc::now(),
            },
        )
        .await
        .map_err(|error| error.to_string())?;
        Self::get(pool, task_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Channel task disappeared after its transition.".to_string())
    }

    pub async fn insert_event(
        pool: &SqlitePool,
        event: &ravenbot_core::ChannelTaskEvent,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO channel_task_events (id, task_id, kind, actor, from_owner, to_owner, note, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(event.id.to_string())
        .bind(event.task_id.to_string())
        .bind(&event.kind)
        .bind(&event.actor)
        .bind(event.from_owner.map(|value| value.to_string()))
        .bind(event.to_owner.map(|value| value.to_string()))
        .bind(&event.note)
        .bind(event.created_at.to_rfc3339())
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn events(
        pool: &SqlitePool,
        task_id: Uuid,
    ) -> Result<Vec<ravenbot_core::ChannelTaskEvent>, sqlx::Error> {
        type EventRow = (
            String,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            String,
        );
        let rows: Vec<EventRow> = sqlx::query_as(
            "SELECT id, task_id, kind, actor, from_owner, to_owner, note, created_at FROM channel_task_events WHERE task_id = ? ORDER BY created_at ASC",
        )
        .bind(task_id.to_string())
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| ravenbot_core::ChannelTaskEvent {
                id: Uuid::parse_str(&row.0).unwrap_or_default(),
                task_id: Uuid::parse_str(&row.1).unwrap_or_default(),
                kind: row.2,
                actor: row.3,
                from_owner: row
                    .4
                    .as_deref()
                    .and_then(|value| Uuid::parse_str(value).ok()),
                to_owner: row
                    .5
                    .as_deref()
                    .and_then(|value| Uuid::parse_str(value).ok()),
                note: row.6,
                created_at: chrono::DateTime::parse_from_rfc3339(&row.7)
                    .map(|value| value.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now()),
            })
            .collect())
    }
}

#[cfg(test)]
mod channel_task_tests {
    use super::*;
    use crate::migrations;

    async fn db() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        migrations::run(&pool).await.expect("migrations");
        pool
    }

    async fn seed(
        pool: &SqlitePool,
    ) -> (
        ravenbot_core::Channel,
        ravenbot_core::Bot,
        ravenbot_core::Bot,
    ) {
        let channel = ravenbot_core::Channel::new("Platform");
        ChannelQueries::create(pool, &channel)
            .await
            .expect("channel");
        let lead = ravenbot_core::Bot::new("Lead", "owns the channel");
        let other = ravenbot_core::Bot::new("Other", "second owner");
        BotQueries::insert(pool, &lead).await.expect("lead");
        BotQueries::insert(pool, &other).await.expect("other");
        ChannelQueries::set_bots(pool, channel.id, &[lead.id, other.id])
            .await
            .expect("roster");
        (channel, lead, other)
    }

    #[tokio::test]
    async fn messages_round_trip_oldest_first() {
        let pool = db().await;
        let (channel, lead, _) = seed(&pool).await;
        let first = ravenbot_core::ChannelMessage::human(channel.id, "You", "first");
        let second = ravenbot_core::ChannelMessage::agent(channel.id, lead.id, "Lead", "second");
        ChannelMessageQueries::insert(&pool, &first)
            .await
            .expect("first");
        ChannelMessageQueries::insert(&pool, &second)
            .await
            .expect("second");
        let messages = ChannelMessageQueries::list_recent(&pool, channel.id, 10)
            .await
            .expect("list");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].content, "first");
        assert_eq!(
            messages[1].author_kind,
            ravenbot_core::ChannelAuthorKind::Agent
        );
        assert_eq!(messages[1].author_name, "Lead");
    }

    #[tokio::test]
    async fn claim_allows_exactly_one_owner_to_start() {
        let pool = db().await;
        let (channel, lead, _) = seed(&pool).await;
        let task = ravenbot_core::ChannelTask::new(
            channel.id,
            "Ship the audit",
            "Check the schema",
            lead.id,
            "You",
        );
        ChannelTaskQueries::create(&pool, &task)
            .await
            .expect("task");

        assert!(ChannelTaskQueries::claim(&pool, task.id)
            .await
            .expect("claim"));
        assert!(!ChannelTaskQueries::claim(&pool, task.id)
            .await
            .expect("second claim"));
        let loaded = ChannelTaskQueries::get(&pool, task.id)
            .await
            .expect("get")
            .expect("exists");
        assert_eq!(loaded.state, ravenbot_core::ChannelTaskState::Running);
    }

    #[tokio::test]
    async fn reassign_changes_the_owner_without_creating_a_second_task() {
        let pool = db().await;
        let (channel, lead, other) = seed(&pool).await;
        let task =
            ravenbot_core::ChannelTask::new(channel.id, "Review the migration", "", lead.id, "You");
        ChannelTaskQueries::create(&pool, &task)
            .await
            .expect("task");

        let changed = ChannelTaskQueries::apply(
            &pool,
            task.id,
            "reassign",
            "You",
            Some(other.id),
            Some("Lead is blocked".to_string()),
        )
        .await
        .expect("reassign");
        assert_eq!(changed.owner_bot_id, other.id);
        assert_eq!(changed.state, ravenbot_core::ChannelTaskState::Pending);

        let tasks = ChannelTaskQueries::list(&pool, channel.id, true)
            .await
            .expect("list");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].owner_bot_id, other.id);
        let events = ChannelTaskQueries::events(&pool, task.id)
            .await
            .expect("events");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, "reassign");
        assert_eq!(events[0].from_owner, Some(lead.id));
        assert_eq!(events[0].to_owner, Some(other.id));
        assert_eq!(events[0].note.as_deref(), Some("Lead is blocked"));
    }

    #[tokio::test]
    async fn archived_tasks_are_readable_but_cannot_start_without_restore() {
        let pool = db().await;
        let (channel, lead, other) = seed(&pool).await;
        let task = ravenbot_core::ChannelTask::new(channel.id, "Old work", "", lead.id, "You");
        ChannelTaskQueries::create(&pool, &task)
            .await
            .expect("task");
        let archived = ChannelTaskQueries::apply(&pool, task.id, "archive", "You", None, None)
            .await
            .expect("archive");
        assert_eq!(archived.state, ravenbot_core::ChannelTaskState::Archived);
        assert!(archived.archived_at.is_some());
        assert!(!ChannelTaskQueries::claim(&pool, task.id)
            .await
            .expect("claim archived"));
        let refused = ChannelTaskQueries::apply(&pool, task.id, "start", "You", None, None)
            .await
            .expect_err("archived start");
        assert!(refused.contains("restored"));
        let reassigned =
            ChannelTaskQueries::apply(&pool, task.id, "reassign", "You", Some(other.id), None)
                .await
                .expect_err("archived reassign");
        assert!(reassigned.contains("archived"));

        let hidden = ChannelTaskQueries::list(&pool, channel.id, false)
            .await
            .expect("visible");
        assert!(hidden.is_empty());
        let visible = ChannelTaskQueries::list(&pool, channel.id, true)
            .await
            .expect("all");
        assert_eq!(visible.len(), 1);

        let restored = ChannelTaskQueries::apply(&pool, task.id, "restore", "You", None, None)
            .await
            .expect("restore");
        assert_eq!(restored.state, ravenbot_core::ChannelTaskState::Paused);
        assert!(restored.archived_at.is_none());
        assert!(ChannelTaskQueries::claim(&pool, task.id)
            .await
            .expect("claim restored"));
    }

    #[tokio::test]
    async fn running_tasks_cannot_be_reassigned_or_archived() {
        let pool = db().await;
        let (channel, lead, other) = seed(&pool).await;
        let task = ravenbot_core::ChannelTask::new(channel.id, "Do work", "", lead.id, "You");
        ChannelTaskQueries::create(&pool, &task)
            .await
            .expect("task");
        assert!(ChannelTaskQueries::claim(&pool, task.id)
            .await
            .expect("claim"));

        let reassign =
            ChannelTaskQueries::apply(&pool, task.id, "reassign", "You", Some(other.id), None)
                .await
                .expect_err("running reassign");
        assert!(reassign.contains("Stop the task"));
        let archive = ChannelTaskQueries::apply(&pool, task.id, "archive", "You", None, None)
            .await
            .expect_err("running archive");
        assert!(archive.contains("Stop the task"));

        let paused = ChannelTaskQueries::apply(&pool, task.id, "pause", "You", None, None)
            .await
            .expect("pause");
        assert_eq!(paused.state, ravenbot_core::ChannelTaskState::Paused);
    }
}

#[cfg(test)]
mod message_page_tests {
    use super::*;
    use crate::migrations;

    async fn db() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        migrations::run(&pool).await.expect("migrations");
        pool
    }

    #[tokio::test]
    async fn a_malformed_row_becomes_a_visible_placeholder() {
        let pool = db().await;
        let bot = ravenbot_core::Bot::new("Pager", "corrupt row test");
        BotQueries::insert(&pool, &bot).await.unwrap();
        let thread = ravenbot_core::Thread::new(bot.id, "corrupt thread");
        ThreadQueries::create(&pool, &thread).await.unwrap();
        let valid = ravenbot_core::Message::user(thread.id, "valid message");
        MessageQueries::insert(&pool, &valid).await.unwrap();
        sqlx::query(
            "INSERT INTO messages (id, thread_id, role, content, attachments, created_at)
             VALUES (?, ?, 'user', 'not-json', 'also-not-json', ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(thread.id.to_string())
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&pool)
        .await
        .unwrap();

        let page = MessageQueries::list_page(&pool, thread.id, None, 10)
            .await
            .expect("a corrupt row must not fail the page");
        assert_eq!(page.messages.len(), 2);
        let placeholder = page
            .messages
            .iter()
            .find(|message| message.id != valid.id)
            .expect("placeholder message");
        match &placeholder.content {
            ravenbot_core::MessageContent::Text { text, .. } => {
                assert!(text.contains("could not be decoded"));
            }
            other => panic!("expected a text placeholder, got {other:?}"),
        }
        assert!(placeholder.attachments.is_empty());

        let full = MessageQueries::list_by_thread(&pool, thread.id)
            .await
            .unwrap();
        assert_eq!(full.len(), 2);
    }

    #[tokio::test]
    async fn renaming_a_thread_trims_and_caps_the_label() {
        let pool = db().await;
        let bot = ravenbot_core::Bot::new("Renamer", "thread title test");
        BotQueries::insert(&pool, &bot).await.unwrap();
        let thread = ravenbot_core::Thread::new(bot.id, "Thread #1");
        ThreadQueries::create(&pool, &thread).await.unwrap();

        let long = format!("  {}  ", "a".repeat(300));
        ThreadQueries::rename(&pool, thread.id, &long)
            .await
            .expect("rename");
        let renamed = ThreadQueries::get(&pool, thread.id)
            .await
            .unwrap()
            .expect("thread");
        assert_eq!(renamed.title.chars().count(), 200);
        assert_eq!(renamed.title, "a".repeat(200));

        assert!(ThreadQueries::rename(&pool, thread.id, "   ")
            .await
            .is_err());
        let unchanged = ThreadQueries::get(&pool, thread.id)
            .await
            .unwrap()
            .expect("thread");
        assert_eq!(unchanged.title, "a".repeat(200));
    }

    #[tokio::test]
    async fn internal_threads_are_kept_out_of_the_chat_list() {
        let pool = db().await;
        let bot = ravenbot_core::Bot::new("Threads", "visibility test");
        BotQueries::insert(&pool, &bot).await.unwrap();
        let chat = ravenbot_core::Thread::new(bot.id, "Chat");
        let node =
            ravenbot_core::Thread::new_internal(ravenbot_core::ThreadKind::Node, bot.id, "Node");
        ThreadQueries::create(&pool, &chat).await.unwrap();
        ThreadQueries::create(&pool, &node).await.unwrap();

        let visible = ThreadQueries::list_visible_by_bot(&pool, bot.id)
            .await
            .unwrap();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].id, chat.id);
        assert!(!ThreadQueries::is_visible(&pool, node.id).await.unwrap());
        assert!(ThreadQueries::is_visible(&pool, chat.id).await.unwrap());
        assert_eq!(
            ThreadQueries::list_by_bot(&pool, bot.id)
                .await
                .unwrap()
                .len(),
            2,
            "internal threads stay queryable by id"
        );
    }

    #[tokio::test]
    async fn context_usage_survives_a_message_reload() {
        let pool = db().await;
        let bot = ravenbot_core::Bot::new("Meter", "context persistence");
        BotQueries::insert(&pool, &bot).await.unwrap();
        let thread = ravenbot_core::Thread::new(bot.id, "long turn");
        ThreadQueries::create(&pool, &thread).await.unwrap();

        let mut message = ravenbot_core::Message::assistant(thread.id, "done");
        message.context_used_tokens = Some(12_345);
        message.context_window_tokens = Some(131_072);
        message.context_compacted = true;
        message.context_system_tokens = Some(2_000);
        message.context_tool_tokens = Some(3_000);
        message.context_memory_tokens = Some(500);
        message.context_message_tokens = Some(6_845);
        MessageQueries::insert(&pool, &message).await.unwrap();

        let loaded = MessageQueries::list_by_thread(&pool, thread.id)
            .await
            .expect("reload message");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].context_used_tokens, Some(12_345));
        assert_eq!(loaded[0].context_window_tokens, Some(131_072));
        assert!(loaded[0].context_compacted);
        assert_eq!(loaded[0].context_system_tokens, Some(2_000));
        assert_eq!(loaded[0].context_tool_tokens, Some(3_000));
        assert_eq!(loaded[0].context_memory_tokens, Some(500));
        assert_eq!(loaded[0].context_message_tokens, Some(6_845));

        let user = ravenbot_core::Message::user(thread.id, "next turn");
        MessageQueries::insert(&pool, &user).await.unwrap();
        let loaded = MessageQueries::list_by_thread(&pool, thread.id)
            .await
            .expect("reload thread");
        let user = loaded
            .iter()
            .find(|message| message.id == user.id)
            .expect("user message");
        assert_eq!(user.context_used_tokens, None);
        assert_eq!(user.context_window_tokens, None);
        assert!(!user.context_compacted);
        assert_eq!(user.context_system_tokens, None);
        assert_eq!(user.context_tool_tokens, None);
        assert_eq!(user.context_memory_tokens, None);
        assert_eq!(user.context_message_tokens, None);
    }

    #[tokio::test]
    async fn pages_walk_backwards_without_duplicates_or_gaps() {
        let pool = db().await;
        let bot = ravenbot_core::Bot::new("Pager", "pagination test");
        BotQueries::insert(&pool, &bot).await.unwrap();
        let thread = ravenbot_core::Thread::new(bot.id, "paged thread");
        ThreadQueries::create(&pool, &thread).await.unwrap();

        let mut inserted = Vec::new();
        for index in 0..5 {
            let message = ravenbot_core::Message::user(thread.id, format!("message {index}"));
            MessageQueries::insert(&pool, &message).await.unwrap();
            inserted.push(message.id);
        }
        ReactionQueries::toggle(&pool, inserted[2], "👍")
            .await
            .unwrap();

        let newest = MessageQueries::list_page(&pool, thread.id, None, 2)
            .await
            .unwrap();
        assert_eq!(
            newest.messages.iter().map(|m| m.id).collect::<Vec<_>>(),
            vec![inserted[3], inserted[4]]
        );
        assert!(newest.has_more);

        let middle = MessageQueries::list_page(&pool, thread.id, Some(inserted[3]), 2)
            .await
            .unwrap();
        assert_eq!(
            middle.messages.iter().map(|m| m.id).collect::<Vec<_>>(),
            vec![inserted[1], inserted[2]]
        );
        assert!(middle.has_more);
        assert_eq!(middle.messages[1].reactions, vec!["👍".to_string()]);

        let oldest = MessageQueries::list_page(&pool, thread.id, Some(inserted[1]), 2)
            .await
            .unwrap();
        assert_eq!(
            oldest.messages.iter().map(|m| m.id).collect::<Vec<_>>(),
            vec![inserted[0]]
        );
        assert!(!oldest.has_more);

        // A reply target older than the page can be fetched directly.
        let targets = MessageQueries::list_by_ids(&pool, thread.id, &[inserted[0], Uuid::new_v4()])
            .await
            .unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].id, inserted[0]);
    }
}

#[cfg(test)]
mod reaction_tests {
    use super::*;
    use crate::migrations;

    async fn db() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        migrations::run(&pool).await.expect("migrations");
        pool
    }

    /// A reaction is a toggle that survives a reload, and it leaves with the
    /// message it was placed on.
    #[tokio::test]
    async fn reactions_persist_toggle_and_are_deleted_with_their_message() {
        let pool = db().await;
        // A thread needs its bot and a message needs its thread: the migrations
        // enable foreign keys, so the fixture has to be a real chain rather
        // than three unrelated UUIDs.
        let bot = ravenbot_core::Bot::new("Reaction Tester", "test bot");
        BotQueries::insert(&pool, &bot).await.expect("insert bot");
        let thread = ravenbot_core::Thread::new(bot.id, "reactions");
        ThreadQueries::create(&pool, &thread)
            .await
            .expect("insert thread");
        let thread_id = thread.id;
        let message = ravenbot_core::Message::user(thread_id, "ship it");
        MessageQueries::insert(&pool, &message)
            .await
            .expect("insert message");

        let added = ReactionQueries::toggle(&pool, message.id, "👍")
            .await
            .expect("add reaction");
        assert_eq!(added, vec!["👍".to_string()]);

        // The same read path the UI uses after a reload.
        let loaded = MessageQueries::list_by_thread(&pool, thread_id)
            .await
            .expect("list messages");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].reactions, vec!["👍".to_string()]);

        let listed = ReactionQueries::list_for_thread(&pool, thread_id)
            .await
            .expect("list reactions");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].message_id, message.id);
        assert_eq!(listed[0].emoji, "👍");

        let removed = ReactionQueries::toggle(&pool, message.id, "👍")
            .await
            .expect("remove reaction");
        assert!(removed.is_empty());
        let loaded = MessageQueries::list_by_thread(&pool, thread_id)
            .await
            .expect("list messages after removal");
        assert!(loaded[0].reactions.is_empty());

        ReactionQueries::toggle(&pool, message.id, "🎉")
            .await
            .expect("re-add");
        MessageQueries::delete(&pool, message.id)
            .await
            .expect("delete message");
        let leftovers: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM message_reactions WHERE message_id = ?")
                .bind(message.id.to_string())
                .fetch_one(&pool)
                .await
                .expect("count leftovers");
        assert_eq!(leftovers, 0, "reactions must not outlive their message");
    }
}

#[cfg(test)]
mod search_tests {
    use super::*;
    use crate::migrations;

    async fn db() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        migrations::run(&pool).await.expect("migrations");
        pool
    }

    #[tokio::test]
    async fn message_hits_carry_the_owning_agent() {
        let pool = db().await;
        let bot = ravenbot_core::Bot::new("Ledger", "billing");
        BotQueries::insert(&pool, &bot).await.expect("bot");
        let thread = ravenbot_core::Thread::new(bot.id, "Invoice review");
        ThreadQueries::create(&pool, &thread).await.expect("thread");
        let message = ravenbot_core::Message::user(thread.id, "The invoice is overdue");
        MessageQueries::insert(&pool, &message)
            .await
            .expect("message");

        let hits = SearchQueries::messages_with_bot(&pool, "invoice", 10)
            .await
            .expect("search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].1, "Invoice review");
        assert_eq!(hits[0].2, bot.id.to_string());
        assert_eq!(hits[0].0.id, message.id);

        // The older shape remains for callers that only need the title.
        let simple = SearchQueries::messages(&pool, "invoice", 10)
            .await
            .expect("search");
        assert_eq!(simple.len(), 1);
        assert_eq!(simple[0].1, "Invoice review");
    }
}

#[cfg(test)]
mod fts_tests {
    use super::*;
    use crate::migrations;

    async fn db() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        migrations::run(&pool).await.expect("migrations");
        pool
    }

    #[tokio::test]
    async fn transcript_search_is_ranked_filterable_and_kept_in_sync() {
        let pool = db().await;
        let bot = ravenbot_core::Bot::new("Ledger", "billing");
        BotQueries::insert(&pool, &bot).await.expect("bot");
        let thread = ravenbot_core::Thread::new(bot.id, "Dashboard review");
        ThreadQueries::create(&pool, &thread).await.expect("thread");
        let message = ravenbot_core::Message::user(
            thread.id,
            "The dashboard latency regressed after the cache change",
        );
        MessageQueries::insert(&pool, &message)
            .await
            .expect("message");

        let hits = SearchQueries::search_fts(&pool, "dashboard latency", None, 10)
            .await
            .expect("fts search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].message_id, message.id.to_string());
        assert_eq!(hits[0].thread_title, "Dashboard review");
        assert!(
            hits[0].snippet.contains(">>>dashboard"),
            "{}",
            hits[0].snippet
        );

        let scoped = SearchQueries::search_fts(&pool, "dashboard", Some(thread.id), 10)
            .await
            .expect("scoped search");
        assert_eq!(scoped.len(), 1);
        assert!(
            SearchQueries::search_fts(&pool, "dashboard", Some(Uuid::new_v4()), 10)
                .await
                .expect("empty scope")
                .is_empty()
        );

        // FTS syntax in a pasted error must be quoted, not parsed.
        assert!(
            SearchQueries::search_fts(&pool, "latency:", None, 10)
                .await
                .expect("special query")
                .len()
                <= 1
        );

        MessageQueries::delete(&pool, message.id)
            .await
            .expect("delete");
        assert!(SearchQueries::search_fts(&pool, "dashboard", None, 10)
            .await
            .expect("after delete")
            .is_empty());
    }
}

#[cfg(test)]
mod progress_tests {
    use super::*;
    use crate::migrations;

    async fn db() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        migrations::run(&pool).await.expect("migrations");
        pool
    }

    async fn run_fixture(pool: &SqlitePool) -> ravenbot_core::Run {
        let bot = ravenbot_core::Bot::new("Runner", "long work");
        BotQueries::insert(pool, &bot).await.expect("bot");
        let thread = ravenbot_core::Thread::new(bot.id, "Long project");
        ThreadQueries::create(pool, &thread).await.expect("thread");
        let run = ravenbot_core::Run::new(bot.id, thread.id);
        RunQueries::insert(pool, &run).await.expect("run");
        run
    }

    #[tokio::test]
    async fn heartbeats_distinguish_a_slow_run_from_a_stalled_one() {
        let pool = db().await;
        let run = run_fixture(&pool).await;

        RunProgressQueries::start(&pool, run.id)
            .await
            .expect("start");
        let progress = RunProgressQueries::get(&pool, run.id)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(progress.phase, "starting");
        assert!(progress.heartbeat_at.is_some());

        RunProgressQueries::touch(&pool, run.id, "tool", Some("shell_exec"))
            .await
            .expect("tool touch");
        let progress = RunProgressQueries::get(&pool, run.id)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(progress.phase, "tool");
        assert_eq!(progress.last_tool.as_deref(), Some("shell_exec"));

        // Make the heartbeat look old without changing the run state. A run
        // that is still doing work is exactly the one this must catch.
        sqlx::query("UPDATE runs SET heartbeat_at = ? WHERE id = ?")
            .bind((Utc::now() - chrono::Duration::seconds(120)).to_rfc3339())
            .bind(run.id.to_string())
            .execute(&pool)
            .await
            .expect("age heartbeat");
        let stalled = RunProgressQueries::stalled(&pool, 60, 10)
            .await
            .expect("stalled");
        assert_eq!(stalled.len(), 1);
        assert_eq!(stalled[0].run_id, run.id.to_string());
        assert_eq!(stalled[0].last_tool.as_deref(), Some("shell_exec"));

        RunProgressQueries::touch(&pool, run.id, "thinking", None)
            .await
            .expect("fresh touch");
        assert!(RunProgressQueries::stalled(&pool, 60, 10)
            .await
            .expect("fresh")
            .is_empty());

        RunProgressQueries::finish(&pool, run.id, "done", None)
            .await
            .expect("finish");
        let progress = RunProgressQueries::get(&pool, run.id)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(progress.phase, "done");
    }

    #[tokio::test]
    async fn work_events_are_ordered_bounded_and_serialized() {
        let pool = db().await;
        let run = run_fixture(&pool).await;

        for (kind, step) in [("run_started", 1), ("model_round", 2), ("run_done", 3)] {
            RunProgressQueries::record(
                &pool,
                Some(run.id),
                Some(run.thread_id),
                kind,
                serde_json::json!({ "step": step }),
            )
            .await
            .expect("event");
            // created_at has millisecond precision; keep the ordering stable.
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }

        let events = RunProgressQueries::events_for_run(&pool, run.id, 10)
            .await
            .expect("events");
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].kind, "run_started");
        assert_eq!(events[0].payload["step"], 1);
        assert_eq!(events[2].kind, "run_done");

        let removed = RunProgressQueries::prune(&pool, 30, 2)
            .await
            .expect("prune");
        assert_eq!(removed, 1);
        let events = RunProgressQueries::events_for_run(&pool, run.id, 10)
            .await
            .expect("events after prune");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].kind, "model_round");
        assert_eq!(events[1].kind, "run_done");
    }
}

#[cfg(test)]
mod work_item_tests {
    use super::*;
    use crate::migrations;

    async fn db() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        migrations::run(&pool).await.expect("migrations");
        pool
    }

    fn item(id: Uuid, office: Uuid, status: &str, max_attempts: i64) -> NewWorkItem {
        NewWorkItem {
            id,
            kind: "office_task".to_string(),
            parent_id: None,
            office_id: Some(office),
            thread_id: None,
            run_id: None,
            title: "Build the dashboard".to_string(),
            instruction: "Create the dashboard and verify it".to_string(),
            contract: serde_json::json!({ "outcome": "dashboard exists" }),
            status: status.to_string(),
            owner_bot_id: None,
            attempts: 0,
            max_attempts,
            priority: 0,
            metadata: serde_json::json!({ "cycle": 1 }),
        }
    }

    #[tokio::test]
    async fn seeding_is_idempotent_and_never_moves_finished_work_backwards() {
        let pool = db().await;
        let office = Uuid::new_v4();
        let id = Uuid::new_v4();
        let planned = item(id, office, "pending", 1);
        WorkItemQueries::seed(&pool, &planned).await.expect("seed");
        WorkItemQueries::seed(&pool, &planned)
            .await
            .expect("reseed");

        let stored = WorkItemQueries::get(&pool, &id.to_string())
            .await
            .expect("get")
            .expect("row");
        assert_eq!(stored.status, "pending");
        assert_eq!(stored.attempts, 0);

        WorkItemQueries::mark_done(&pool, &id.to_string(), None, "shipped")
            .await
            .expect("done");
        WorkItemQueries::seed(&pool, &planned)
            .await
            .expect("seed after done");
        let stored = WorkItemQueries::get(&pool, &id.to_string())
            .await
            .expect("get")
            .expect("row");
        assert_eq!(stored.status, "done");
        assert_eq!(stored.result.as_deref(), Some("shipped"));

        let listed = WorkItemQueries::list_by_office(&pool, office, 10)
            .await
            .expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(
            listed[0].metadata["cycle"], 1,
            "metadata survives the round trip"
        );
    }

    #[tokio::test]
    async fn a_ready_item_can_only_be_claimed_once_and_retries_respect_the_cap() {
        let pool = db().await;
        let office = Uuid::new_v4();
        let id = Uuid::new_v4();
        WorkItemQueries::seed(&pool, &item(id, office, "pending", 2))
            .await
            .expect("seed");
        let key = id.to_string();

        let first_run = Uuid::new_v4();
        assert!(
            WorkItemQueries::claim_for_run(&pool, &key, first_run, "worker-a", 60)
                .await
                .expect("first claim")
        );
        assert!(
            !WorkItemQueries::claim_for_run(&pool, &key, Uuid::new_v4(), "worker-b", 60)
                .await
                .expect("second claim"),
            "a live lease must not be stolen"
        );
        // The same run may re-lease its own item after a restart.
        assert!(
            WorkItemQueries::claim_for_run(&pool, &key, first_run, "worker-a", 60)
                .await
                .expect("resume claim")
        );
        let stored = WorkItemQueries::get(&pool, &key)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(stored.attempts, 1, "resume is not a new attempt");
        assert_eq!(stored.status, "running");

        WorkItemQueries::mark_failed(&pool, &key, Some(first_run), "provider down")
            .await
            .expect("fail");
        let second_run = Uuid::new_v4();
        assert!(
            WorkItemQueries::claim_for_run(&pool, &key, second_run, "worker-b", 60)
                .await
                .expect("retry claim")
        );
        let stored = WorkItemQueries::get(&pool, &key)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(stored.attempts, 2);

        WorkItemQueries::mark_failed(&pool, &key, Some(second_run), "still down")
            .await
            .expect("fail again");
        assert!(
            !WorkItemQueries::claim_for_run(&pool, &key, Uuid::new_v4(), "worker-c", 60)
                .await
                .expect("exhausted claim"),
            "attempt cap must stop an automatic retry"
        );
    }

    #[tokio::test]
    async fn changed_files_become_artifacts_exactly_once() {
        let pool = db().await;
        let office = Uuid::new_v4();
        let id = Uuid::new_v4();
        WorkItemQueries::seed(&pool, &item(id, office, "pending", 1))
            .await
            .expect("seed");
        let key = id.to_string();
        let run_id = Uuid::new_v4();
        let bot_id = Uuid::new_v4();
        FileChangeQueries::insert(
            &pool,
            &ravenbot_core::FileChange::new(
                bot_id,
                Some(run_id),
                Some(Uuid::new_v4()),
                "src/dashboard.rs",
                "file_write",
                12,
                0,
            ),
        )
        .await
        .expect("change");
        // Two writes to the same file are one artifact path.
        FileChangeQueries::insert(
            &pool,
            &ravenbot_core::FileChange::new(
                bot_id,
                Some(run_id),
                Some(Uuid::new_v4()),
                "src/dashboard.rs",
                "code_edit",
                2,
                1,
            ),
        )
        .await
        .expect("second change");

        let paths = FileChangeQueries::paths_for_run(&pool, run_id)
            .await
            .expect("paths");
        assert_eq!(paths, vec!["src/dashboard.rs".to_string()]);
        assert!(WorkItemQueries::add_artifact_once(&pool, &key, &paths[0])
            .await
            .expect("attach"));
        assert!(
            !WorkItemQueries::add_artifact_once(&pool, &key, &paths[0])
                .await
                .expect("second attach"),
            "the same path must not be attached twice"
        );
        let artifacts = WorkItemQueries::list_artifacts(&pool, &key)
            .await
            .expect("artifacts");
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].path, "src/dashboard.rs");
    }

    #[tokio::test]
    async fn review_block_and_rework_states_are_reversible_with_history() {
        let pool = db().await;
        let office = Uuid::new_v4();
        let id = Uuid::new_v4();
        WorkItemQueries::seed(&pool, &item(id, office, "pending", 1))
            .await
            .expect("seed");
        let key = id.to_string();
        WorkItemQueries::mark_done(&pool, &key, None, "dashboard shipped")
            .await
            .expect("done");

        assert!(WorkItemQueries::mark_review(&pool, &key, "reviewer")
            .await
            .expect("review"));
        let stored = WorkItemQueries::get(&pool, &key)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(stored.status, "review");

        assert!(
            WorkItemQueries::approve_review(&pool, &key, "reviewer", Some("Looks correct."))
                .await
                .expect("approve")
        );
        let detail = WorkItemQueries::detail(&pool, &key)
            .await
            .expect("detail")
            .expect("row");
        assert_eq!(detail.item.status, "done");
        assert_eq!(detail.comments.len(), 1);
        assert_eq!(detail.comments[0].body, "Looks correct.");
        assert!(detail
            .events
            .iter()
            .any(|event| event.kind == "review_approved"));

        assert!(WorkItemQueries::mark_review(&pool, &key, "reviewer")
            .await
            .expect("review again"));
        assert!(
            WorkItemQueries::mark_blocked(&pool, &key, "Needs a product decision")
                .await
                .expect("block")
        );
        let stored = WorkItemQueries::get(&pool, &key)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(stored.status, "blocked");

        // Unblocking and rework both return the item to the scheduler.
        assert!(WorkItemQueries::mark_pending(&pool, &key, "unblocked")
            .await
            .expect("unblock"));
        let stored = WorkItemQueries::get(&pool, &key)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(stored.status, "pending");
        assert!(stored.completed_at.is_none());

        // A deliberate rerun resets the attempt counter; a rework deserves a
        // fresh retry budget instead of inheriting the previous failure.
        let run = Uuid::new_v4();
        assert!(
            WorkItemQueries::claim_for_run(&pool, &key, run, "worker", 60)
                .await
                .expect("claim")
        );
        WorkItemQueries::mark_failed(&pool, &key, Some(run), "first attempt died")
            .await
            .expect("fail");
        WorkItemQueries::reset_for_rerun(&pool, &key, "changes requested")
            .await
            .expect("rerun");
        let stored = WorkItemQueries::get(&pool, &key)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(stored.status, "pending");
        assert_eq!(stored.attempts, 0);
    }

    #[tokio::test]
    async fn failed_attempts_outside_the_current_plan_are_retired() {
        let pool = db().await;
        let office = Uuid::new_v4();
        let stale = Uuid::new_v4();
        let current = Uuid::new_v4();
        for id in [stale, current] {
            WorkItemQueries::seed(&pool, &item(id, office, "pending", 1))
                .await
                .expect("seed");
            WorkItemQueries::mark_failed(&pool, &id.to_string(), None, "old failure")
                .await
                .expect("failed");
        }

        let retired = WorkItemQueries::retire_stale_failures(&pool, office, &[current])
            .await
            .expect("retire");
        assert_eq!(retired, 1);
        let stored = WorkItemQueries::get(&pool, &stale.to_string())
            .await
            .expect("get")
            .expect("row");
        assert_eq!(stored.status, "cancelled");
        assert!(stored
            .error
            .as_deref()
            .is_some_and(|error| error.contains("Superseded")));
        let kept = WorkItemQueries::get(&pool, &current.to_string())
            .await
            .expect("get")
            .expect("row");
        assert_eq!(kept.status, "failed");
    }

    #[tokio::test]
    async fn expired_running_work_is_retired() {
        let pool = db().await;
        let office = Uuid::new_v4();
        let id = Uuid::new_v4();
        WorkItemQueries::seed(&pool, &item(id, office, "pending", 1))
            .await
            .expect("seed");
        assert!(
            WorkItemQueries::claim_for_run(&pool, &id.to_string(), Uuid::new_v4(), "graph", 60)
                .await
                .expect("claim"),
            "the item should be claimed"
        );
        sqlx::query("UPDATE work_items SET lease_expires_at = ? WHERE id = ?")
            .bind("2020-01-01T00:00:00+00:00")
            .bind(id.to_string())
            .execute(&pool)
            .await
            .expect("expire lease");

        let retired =
            WorkItemQueries::retire_expired_running(&pool, office, "2026-01-01T00:00:00+00:00")
                .await
                .expect("retire");
        assert_eq!(retired, 1);
        let stored = WorkItemQueries::get(&pool, &id.to_string())
            .await
            .expect("get")
            .expect("row");
        assert_eq!(stored.status, "cancelled");
        assert!(stored
            .error
            .as_deref()
            .is_some_and(|error| error.contains("ended before it finished")));
    }

    #[tokio::test]
    async fn open_and_done_queries_keep_the_project_backlog_visible() {
        let pool = db().await;
        let office = Uuid::new_v4();
        let done_id = Uuid::new_v4();
        let failed_id = Uuid::new_v4();
        let pending_id = Uuid::new_v4();
        for (id, status) in [
            (done_id, "pending"),
            (failed_id, "pending"),
            (pending_id, "pending"),
        ] {
            WorkItemQueries::seed(&pool, &item(id, office, status, 1))
                .await
                .expect("seed");
        }
        WorkItemQueries::mark_done(&pool, &done_id.to_string(), None, "shipped")
            .await
            .expect("done");
        WorkItemQueries::mark_failed(&pool, &failed_id.to_string(), None, "tests red")
            .await
            .expect("failed");

        let open = WorkItemQueries::list_open_by_office(&pool, office, 20)
            .await
            .expect("open");
        assert_eq!(open.len(), 2);
        assert_eq!(open[0].id, failed_id.to_string(), "failures come first");
        assert!(open.iter().any(|item| item.id == pending_id.to_string()));

        let done = WorkItemQueries::list_done_by_office(&pool, office, 20)
            .await
            .expect("done");
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].id, done_id.to_string());
    }

    #[tokio::test]
    async fn an_expired_lease_is_reclaimable_by_a_new_run() {
        let pool = db().await;
        let office = Uuid::new_v4();
        let id = Uuid::new_v4();
        WorkItemQueries::seed(&pool, &item(id, office, "pending", 2))
            .await
            .expect("seed");
        let key = id.to_string();
        assert!(
            WorkItemQueries::claim_for_run(&pool, &key, Uuid::new_v4(), "crashed", 0)
                .await
                .expect("short claim")
        );
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;

        let new_run = Uuid::new_v4();
        assert!(
            WorkItemQueries::claim_for_run(&pool, &key, new_run, "replacement", 60)
                .await
                .expect("reclaim")
        );
        let stored = WorkItemQueries::get(&pool, &key)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(stored.status, "running");
        assert_eq!(stored.run_id, Some(new_run.to_string()));
        assert_eq!(stored.attempts, 2, "reclaiming is a new attempt");
    }

    #[tokio::test]
    async fn transitions_comments_artifacts_and_events_form_one_readable_record() {
        let pool = db().await;
        let office = Uuid::new_v4();
        let id = Uuid::new_v4();
        WorkItemQueries::seed(&pool, &item(id, office, "pending", 1))
            .await
            .expect("seed");
        let key = id.to_string();
        let run_id = Uuid::new_v4();

        assert!(
            WorkItemQueries::claim_for_run(&pool, &key, run_id, "graph", 60)
                .await
                .expect("claim")
        );
        assert!(WorkItemQueries::heartbeat(&pool, &key, 60)
            .await
            .expect("heartbeat"));
        assert!(
            WorkItemQueries::mark_done(&pool, &key, Some(run_id), "dashboard shipped")
                .await
                .expect("done")
        );
        assert!(
            !WorkItemQueries::mark_done(&pool, &key, Some(run_id), "duplicate")
                .await
                .expect("second done"),
            "terminal transitions are idempotent"
        );
        assert!(
            !WorkItemQueries::claim_for_run(&pool, &key, Uuid::new_v4(), "thief", 60)
                .await
                .expect("done claim"),
            "a completed item is never reclaimed"
        );

        WorkItemQueries::add_comment(&pool, &key, "reviewer", "Verified the build.")
            .await
            .expect("comment");
        WorkItemQueries::add_artifact(
            &pool,
            &key,
            "dist/dashboard.html",
            Some("abc123"),
            Some("text/html"),
            Some(42),
        )
        .await
        .expect("artifact");

        let detail = WorkItemQueries::detail(&pool, &key)
            .await
            .expect("detail")
            .expect("record");
        assert_eq!(detail.item.status, "done");
        assert_eq!(detail.comments.len(), 1);
        assert_eq!(detail.comments[0].author, "reviewer");
        assert_eq!(detail.artifacts.len(), 1);
        assert_eq!(detail.artifacts[0].path, "dist/dashboard.html");
        let kinds: Vec<&str> = detail
            .events
            .iter()
            .map(|event| event.kind.as_str())
            .collect();
        assert!(kinds.contains(&"claimed"), "{kinds:?}");
        assert!(kinds.contains(&"completed"), "{kinds:?}");
        assert!(kinds.contains(&"comment"), "{kinds:?}");
        assert!(kinds.contains(&"artifact"), "{kinds:?}");
    }

    #[tokio::test]
    async fn two_concurrent_workers_cannot_win_the_same_claim() {
        let path = std::env::temp_dir().join(format!("ravenbot-work-claim-{}.db", Uuid::new_v4()));
        let database = crate::Database::new(&path).await.expect("temp db");
        let pool = database.pool().clone();
        let office = Uuid::new_v4();
        let id = Uuid::new_v4();
        WorkItemQueries::seed(&pool, &item(id, office, "pending", 1))
            .await
            .expect("seed");
        let key = id.to_string();
        let (a, b) = tokio::join!(
            WorkItemQueries::claim_for_run(&pool, &key, Uuid::new_v4(), "worker-a", 60),
            WorkItemQueries::claim_for_run(&pool, &key, Uuid::new_v4(), "worker-b", 60),
        );
        let wins = usize::from(a.expect("claim a")) + usize::from(b.expect("claim b"));
        assert_eq!(wins, 1, "exactly one concurrent claim may win");
        drop(database);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn the_first_human_claim_wins_and_the_loser_is_told_who_has_it() {
        let pool = db().await;
        let office = Uuid::new_v4();
        let id = Uuid::new_v4();
        WorkItemQueries::seed(&pool, &item(id, office, "pending", 1))
            .await
            .expect("seed");
        let key = id.to_string();

        let winner = WorkItemQueries::claim(&pool, &key, "alice")
            .await
            .expect("first claim");
        assert_eq!(winner.owner_account.as_deref(), Some("alice"));
        assert!(winner.claimed_at.is_some());

        let error = WorkItemQueries::claim(&pool, &key, "bob")
            .await
            .expect_err("the second claim must lose");
        match error {
            ClaimError::Conflict { owner } => {
                assert_eq!(owner.as_deref(), Some("alice"));
            }
            other => panic!("expected a conflict carrying the owner, got {other:?}"),
        }

        let stored = WorkItemQueries::get(&pool, &key)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(stored.owner_account.as_deref(), Some("alice"));
        assert_eq!(stored.claimed_at, winner.claimed_at);
        assert_eq!(stored.updated_at, winner.updated_at);
    }

    #[tokio::test]
    async fn reclaiming_your_own_task_is_idempotent_and_preserves_the_stamp() {
        let pool = db().await;
        let office = Uuid::new_v4();
        let id = Uuid::new_v4();
        let planned = item(id, office, "pending", 1);
        WorkItemQueries::seed(&pool, &planned).await.expect("seed");
        let key = id.to_string();

        let first = WorkItemQueries::claim(&pool, &key, "alice")
            .await
            .expect("first claim");
        let claimed_at = first.claimed_at.clone().expect("claim stamp");
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;

        let again = WorkItemQueries::claim(&pool, &key, "  alice  ")
            .await
            .expect("re-claim");
        assert_eq!(again.owner_account.as_deref(), Some("alice"));
        assert_eq!(again.claimed_at.as_deref(), Some(claimed_at.as_str()));

        // A resumed planner reseeds the same node; that must not clear the
        // person's claim or reset the stamp.
        WorkItemQueries::seed(&pool, &planned)
            .await
            .expect("reseed");
        let stored = WorkItemQueries::get(&pool, &key)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(stored.owner_account.as_deref(), Some("alice"));
        assert_eq!(stored.claimed_at, Some(claimed_at));

        let detail = WorkItemQueries::detail(&pool, &key)
            .await
            .expect("detail")
            .expect("record");
        let claims = detail
            .events
            .iter()
            .filter(|event| event.kind == "claimed")
            .count();
        assert_eq!(claims, 1, "an idempotent re-claim is not a new transition");
    }

    #[tokio::test]
    async fn only_the_owner_may_release_and_release_frees_the_next_claim() {
        let pool = db().await;
        let office = Uuid::new_v4();
        let id = Uuid::new_v4();
        WorkItemQueries::seed(&pool, &item(id, office, "pending", 1))
            .await
            .expect("seed");
        let key = id.to_string();

        WorkItemQueries::claim(&pool, &key, "alice")
            .await
            .expect("claim");

        let error = WorkItemQueries::release(&pool, &key, "bob")
            .await
            .expect_err("a non-owner cannot release");
        match error {
            ClaimError::Conflict { owner } => {
                assert_eq!(owner.as_deref(), Some("alice"));
            }
            other => panic!("expected a conflict, got {other:?}"),
        }
        let still_owned = WorkItemQueries::get(&pool, &key)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(still_owned.owner_account.as_deref(), Some("alice"));

        let released = WorkItemQueries::release(&pool, &key, "alice")
            .await
            .expect("owner release");
        assert!(released.owner_account.is_none());
        assert!(released.claimed_at.is_none());

        let error = WorkItemQueries::release(&pool, &key, "alice")
            .await
            .expect_err("a released task has nothing to release");
        match error {
            ClaimError::Conflict { owner } => assert!(owner.is_none()),
            other => panic!("expected an unclaimed conflict, got {other:?}"),
        }

        let next = WorkItemQueries::claim(&pool, &key, "bob")
            .await
            .expect("after release the next person can claim");
        assert_eq!(next.owner_account.as_deref(), Some("bob"));
        assert!(next.claimed_at.is_some());
    }

    #[tokio::test]
    async fn claim_and_release_transitions_are_durable_timeline_events() {
        let pool = db().await;
        let office = Uuid::new_v4();
        let id = Uuid::new_v4();
        WorkItemQueries::seed(&pool, &item(id, office, "pending", 1))
            .await
            .expect("seed");
        let key = id.to_string();

        WorkItemQueries::claim(&pool, &key, "alice")
            .await
            .expect("claim");
        WorkItemQueries::release(&pool, &key, "alice")
            .await
            .expect("release");
        WorkItemQueries::claim(&pool, &key, "bob")
            .await
            .expect("second claim");

        let detail = WorkItemQueries::detail(&pool, &key)
            .await
            .expect("detail")
            .expect("record");
        let claimed: Vec<_> = detail
            .events
            .iter()
            .filter(|event| event.kind == "claimed")
            .collect();
        let released: Vec<_> = detail
            .events
            .iter()
            .filter(|event| event.kind == "released")
            .collect();
        assert_eq!(claimed.len(), 2);
        assert_eq!(released.len(), 1);
        assert_eq!(claimed[0].payload["account"], "alice");
        assert!(claimed[0].payload["previous_owner"].is_null());
        assert_eq!(claimed[1].payload["account"], "bob");
        assert!(claimed[1].payload["previous_owner"].is_null());
        assert_eq!(released[0].payload["account"], "alice");
        assert_eq!(released[0].payload["previous_owner"], "alice");
    }

    #[tokio::test]
    async fn an_archived_task_cannot_be_claimed() {
        let pool = db().await;
        let office = Uuid::new_v4();
        let id = Uuid::new_v4();
        WorkItemQueries::seed(&pool, &item(id, office, "archived", 1))
            .await
            .expect("seed");
        let key = id.to_string();

        match WorkItemQueries::claim(&pool, &key, "alice")
            .await
            .expect_err("archived work is not claimable")
        {
            ClaimError::InvalidState(status) => assert_eq!(status, "archived"),
            other => panic!("expected an invalid state, got {other:?}"),
        }
    }
}

#[cfg(test)]
mod gate_tests {
    use super::*;
    use crate::migrations;

    async fn db() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        migrations::run(&pool).await.expect("migrations");
        pool
    }

    #[tokio::test]
    async fn gate_results_are_persisted_with_the_failure_tail() {
        let pool = db().await;
        let office = Uuid::new_v4();
        let thread = Uuid::new_v4();
        QualityGateQueries::record(
            &pool,
            office,
            Some(thread),
            2,
            "npm test",
            "/tmp/office",
            Some(1),
            false,
            false,
            812,
            "FAIL src/app.test.ts",
            None,
        )
        .await
        .expect("record failure");
        QualityGateQueries::record(
            &pool,
            office,
            Some(thread),
            2,
            "npm run build",
            "/tmp/office",
            Some(0),
            true,
            false,
            1200,
            "built",
            None,
        )
        .await
        .expect("record pass");

        let runs = QualityGateQueries::list_for_office(&pool, office, 10)
            .await
            .expect("list");
        assert_eq!(runs.len(), 2);
        let failed = runs.iter().find(|run| !run.passed).expect("failure");
        assert_eq!(failed.cycle, 2);
        assert_eq!(failed.exit_code, Some(1));
        assert_eq!(failed.command, "npm test");
        assert!(failed.output_tail.contains("FAIL"));
    }
}

#[cfg(test)]
mod thread_worktree_tests {
    use super::*;
    use crate::migrations;

    async fn db() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        migrations::run(&pool).await.expect("migrations");
        pool
    }

    #[tokio::test]
    async fn a_thread_has_one_active_checkout_and_a_finished_history() {
        let pool = db().await;
        let thread = Uuid::new_v4();
        let run = Uuid::new_v4();
        let first = ThreadWorktreeQueries::activate(
            &pool,
            thread,
            "/project",
            "/worktrees/first",
            "/worktrees/first",
            "abc123",
            run,
        )
        .await
        .expect("activate");
        assert_eq!(first.status, "active");
        assert!(ThreadWorktreeQueries::active_for_thread(&pool, thread)
            .await
            .expect("active")
            .is_some());

        assert!(ThreadWorktreeQueries::finish(&pool, thread, "applied", "")
            .await
            .expect("finish"));
        assert!(ThreadWorktreeQueries::active_for_thread(&pool, thread)
            .await
            .expect("active")
            .is_none());
        let latest = ThreadWorktreeQueries::latest_for_thread(&pool, thread)
            .await
            .expect("latest")
            .expect("row");
        assert_eq!(latest.status, "applied");
        assert!(latest.finished_at.is_some());

        let second_run = Uuid::new_v4();
        let second = ThreadWorktreeQueries::activate(
            &pool,
            thread,
            "/project",
            "/worktrees/second",
            "/worktrees/second",
            "def456",
            second_run,
        )
        .await
        .expect("reactivate");
        assert_eq!(second.worktree_path, "/worktrees/second");
        assert_eq!(second.status, "active");
    }
}

#[cfg(test)]
mod goal_tests {
    use super::*;
    use crate::migrations;

    async fn db() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        migrations::run(&pool).await.expect("migrations");
        pool
    }

    #[tokio::test]
    async fn an_office_goal_is_created_once_and_its_contract_is_editable() {
        let pool = db().await;
        let office = Uuid::new_v4();
        let goal = GoalQueries::ensure_office_goal(&pool, office, "Ship the dashboard", 8)
            .await
            .expect("ensure");
        assert_eq!(goal.objective, "Ship the dashboard");
        assert_eq!(goal.status, "active");
        assert_eq!(goal.contract.max_turns, 8);
        let again = GoalQueries::ensure_office_goal(&pool, office, "Different objective", 4)
            .await
            .expect("ensure again");
        assert_eq!(again.id, goal.id, "the existing contract is not replaced");

        let mut contract = goal.contract.clone();
        contract.outcome = "The dashboard builds and all charts render.".to_string();
        contract.gates = vec![
            ravenbot_core::GoalGate::new("npm test"),
            ravenbot_core::GoalGate::new("npm run build"),
        ];
        let updated =
            GoalQueries::update_contract(&pool, Uuid::parse_str(&goal.id).unwrap(), &contract)
                .await
                .expect("update")
                .expect("goal");
        assert_eq!(updated.contract.outcome, contract.outcome);
        let gates = GoalQueries::get_gates(&pool, Uuid::parse_str(&goal.id).unwrap())
            .await
            .expect("gates");
        assert_eq!(gates.len(), 2);
        assert_eq!(gates[0].command, "npm test");
        assert_eq!(gates[1].position, 1);
    }

    #[tokio::test]
    async fn a_thread_goal_is_created_once_with_its_round_budget() {
        let pool = db().await;
        let thread = Uuid::new_v4();
        let goal = GoalQueries::ensure_thread_goal(&pool, thread, "Ship the feature", 6)
            .await
            .expect("ensure");
        assert_eq!(goal.office_id, None);
        assert_eq!(goal.thread_id, Some(thread.to_string()));
        assert_eq!(goal.status, "active");
        assert_eq!(goal.max_rounds, 6);
        assert_eq!(goal.contract.max_turns, 6);

        let again = GoalQueries::ensure_thread_goal(&pool, thread, "Different", 2)
            .await
            .expect("ensure again");
        assert_eq!(
            again.id, goal.id,
            "the existing thread goal is not replaced"
        );
        assert_eq!(again.max_rounds, 6);

        let found = GoalQueries::get_by_thread(&pool, thread)
            .await
            .expect("get")
            .expect("goal");
        assert_eq!(found.id, goal.id);
        let detail = GoalQueries::thread_detail(&pool, thread)
            .await
            .expect("detail")
            .expect("detail");
        assert!(detail.gates.is_empty());

        // An office goal must never be picked up by the thread-goal loop.
        let office_goal =
            GoalQueries::ensure_office_goal(&pool, Uuid::new_v4(), "Office mission", 2)
                .await
                .expect("office goal");
        let due = GoalQueries::due_thread_for_autopilot(&pool, "2999-01-01T00:00:00Z", 10)
            .await
            .expect("due");
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id, goal.id);
        assert_ne!(due[0].id, office_goal.id);
        assert!(
            GoalQueries::due_thread_for_autopilot(&pool, "1970-01-01T00:00:00Z", 10)
                .await
                .expect("not due")
                .is_empty(),
            "a goal armed in the future is not due"
        );
    }

    #[tokio::test]
    async fn a_thread_goal_yields_to_a_live_run() {
        let pool = db().await;
        let bot = ravenbot_core::Bot::new("Writer", "writing");
        BotQueries::insert(&pool, &bot).await.expect("bot");
        let thread = ravenbot_core::Thread::new(bot.id, "Goal thread");
        ThreadQueries::create(&pool, &thread).await.expect("thread");

        let run = ravenbot_core::Run::new(bot.id, thread.id);
        RunQueries::insert(&pool, &run).await.expect("run");
        sqlx::query("UPDATE runs SET state = 'acting' WHERE id = ?")
            .bind(run.id.to_string())
            .execute(&pool)
            .await
            .expect("mark acting");
        assert!(
            RunProgressQueries::has_active_for_thread(&pool, thread.id)
                .await
                .expect("active"),
            "a live run in the thread must block another goal turn"
        );
        assert_eq!(
            RunProgressQueries::active_ids_for_thread(&pool, thread.id)
                .await
                .expect("active ids"),
            vec![run.id],
            "stopping a goal can address the run in flight"
        );

        sqlx::query("UPDATE runs SET state = 'paused' WHERE id = ?")
            .bind(run.id.to_string())
            .execute(&pool)
            .await
            .expect("mark paused");
        assert!(
            RunProgressQueries::has_active_for_thread(&pool, thread.id)
                .await
                .expect("paused"),
            "a user-paused run must not be interleaved with a goal round"
        );

        sqlx::query("UPDATE runs SET state = 'completed' WHERE id = ?")
            .bind(run.id.to_string())
            .execute(&pool)
            .await
            .expect("mark completed");
        assert!(
            !RunProgressQueries::has_active_for_thread(&pool, thread.id)
                .await
                .expect("inactive"),
            "a finished run must not block the next goal turn"
        );
        assert!(
            RunProgressQueries::active_ids_for_thread(&pool, thread.id)
                .await
                .expect("finished ids")
                .is_empty(),
            "a finished run is not cancellable work"
        );
    }

    #[tokio::test]
    async fn raising_a_goal_budget_keeps_its_history() {
        let pool = db().await;
        let thread = Uuid::new_v4();
        let goal = GoalQueries::ensure_thread_goal(&pool, thread, "Summarize the notes.", 4)
            .await
            .expect("ensure");
        let id = Uuid::parse_str(&goal.id).unwrap();
        let recorded = GoalQueries::record_verdict(
            &pool,
            id,
            ravenbot_core::GoalVerdict::Continue,
            "one more pass",
            900,
            false,
        )
        .await
        .expect("verdict")
        .expect("goal");
        assert_eq!(recorded.rounds_used, 1);
        assert_eq!(recorded.tokens_used, 900);

        let raised = GoalQueries::set_budget(&pool, id, 50_000, Some(12))
            .await
            .expect("budget")
            .expect("goal");
        assert_eq!(raised.contract.max_tokens, 50_000);
        assert_eq!(raised.contract.max_turns, 12);
        assert_eq!(raised.max_rounds, 12);
        assert_eq!(raised.rounds_used, 1, "history is not reset");
        assert_eq!(raised.tokens_used, 900, "spend is not reset");

        let unlimited = GoalQueries::set_budget(&pool, id, 0, None)
            .await
            .expect("unlimited")
            .expect("goal");
        assert_eq!(unlimited.contract.max_tokens, 0);
        assert_eq!(unlimited.max_rounds, 12, "rounds stay when unspecified");
    }

    #[tokio::test]
    async fn verdicts_update_history_and_done_is_terminal() {
        let pool = db().await;
        let office = Uuid::new_v4();
        let goal = GoalQueries::ensure_office_goal(&pool, office, "Ship it", 4)
            .await
            .expect("ensure");
        let id = Uuid::parse_str(&goal.id).unwrap();

        let continued = GoalQueries::record_verdict(
            &pool,
            id,
            ravenbot_core::GoalVerdict::Continue,
            "one milestone left",
            120,
            false,
        )
        .await
        .expect("continue")
        .expect("goal");
        assert_eq!(continued.turns_used, 1);
        assert_eq!(continued.rounds_used, 1);
        assert_eq!(continued.tokens_used, 120);
        assert_eq!(continued.status, "active");
        assert_eq!(continued.last_verdict.as_deref(), Some("continue"));
        assert!(
            continued.next_run_at.is_some(),
            "a continue verdict must arm the next automatic cycle"
        );
        assert_eq!(continued.consecutive_failures, 0);

        let done = GoalQueries::record_verdict(
            &pool,
            id,
            ravenbot_core::GoalVerdict::Done,
            "verified",
            30,
            false,
        )
        .await
        .expect("done")
        .expect("goal");
        assert_eq!(done.status, "done");
        assert!(done.completed_at.is_some());
        assert!(
            done.next_run_at.is_none(),
            "a done goal must not wake up again"
        );

        let late = GoalQueries::record_verdict(
            &pool,
            id,
            ravenbot_core::GoalVerdict::Continue,
            "late review",
            1,
            false,
        )
        .await
        .expect("late")
        .expect("goal");
        assert_eq!(late.status, "done", "a done goal is not reopened");

        assert!(GoalQueries::reopen(&pool, id).await.expect("reopen"));
        let reopened = GoalQueries::get(&pool, id)
            .await
            .expect("get")
            .expect("goal");
        assert_eq!(reopened.status, "active");
        assert_eq!(reopened.rounds_used, 0);
        assert!(reopened.next_run_at.is_some());
        assert!(reopened.completed_at.is_none());
        assert!(reopened.last_verdict.is_none());
    }

    #[tokio::test]
    async fn a_round_that_finishes_after_cancel_cannot_revive_the_goal() {
        let pool = db().await;
        let office = Uuid::new_v4();
        let goal = GoalQueries::ensure_office_goal(&pool, office, "Ship it", 4)
            .await
            .expect("ensure");
        let id = Uuid::parse_str(&goal.id).unwrap();
        GoalQueries::set_status(&pool, id, ravenbot_core::GoalStatus::Cancelled)
            .await
            .expect("cancel");
        let late = GoalQueries::record_verdict(
            &pool,
            id,
            ravenbot_core::GoalVerdict::Done,
            "the round finished after the user stopped it",
            0,
            false,
        )
        .await
        .expect("late")
        .expect("goal");
        assert_eq!(late.status, "cancelled");
        assert!(late.completed_at.is_none());
        assert!(late.next_run_at.is_none());
    }

    #[tokio::test]
    async fn a_late_continue_cannot_override_a_user_pause() {
        let pool = db().await;
        let office = Uuid::new_v4();
        let goal = GoalQueries::ensure_office_goal(&pool, office, "Ship it", 4)
            .await
            .expect("ensure");
        let id = Uuid::parse_str(&goal.id).unwrap();
        GoalQueries::set_status(&pool, id, ravenbot_core::GoalStatus::Paused)
            .await
            .expect("pause");
        let late = GoalQueries::record_verdict(
            &pool,
            id,
            ravenbot_core::GoalVerdict::Continue,
            "late cycle",
            0,
            false,
        )
        .await
        .expect("late")
        .expect("goal");
        assert_eq!(late.status, "paused");
        assert!(late.next_run_at.is_none());
    }

    #[tokio::test]
    async fn a_blocker_must_persist_for_three_rounds_before_the_goal_is_parked() {
        let pool = db().await;
        let office = Uuid::new_v4();
        let goal = GoalQueries::ensure_office_goal(&pool, office, "Ship it", 4)
            .await
            .expect("ensure");
        let id = Uuid::parse_str(&goal.id).unwrap();

        for round in 1..=2 {
            let blocked = GoalQueries::record_verdict(
                &pool,
                id,
                ravenbot_core::GoalVerdict::Blocked,
                "missing API credential",
                0,
                false,
            )
            .await
            .expect("blocked")
            .expect("goal");
            assert_eq!(blocked.status, "active", "round {round}");
            assert_eq!(blocked.blocked_rounds, round);
            assert!(blocked.next_run_at.is_some(), "round {round}");
        }

        let parked = GoalQueries::record_verdict(
            &pool,
            id,
            ravenbot_core::GoalVerdict::Blocked,
            "missing API credential",
            0,
            false,
        )
        .await
        .expect("parked")
        .expect("goal");
        assert_eq!(parked.status, "blocked");
        assert_eq!(parked.blocked_rounds, 3);
        assert!(parked.next_run_at.is_none());
        assert!(parked.last_blocker.contains("credential"));
    }

    #[tokio::test]
    async fn exhausted_autopilot_rounds_pause_the_goal_with_a_reason() {
        let pool = db().await;
        let office = Uuid::new_v4();
        let goal = GoalQueries::ensure_office_goal(&pool, office, "Ship it", 4)
            .await
            .expect("ensure");
        let id = Uuid::parse_str(&goal.id).unwrap();
        sqlx::query("UPDATE goals SET max_rounds = 2 WHERE id = ?")
            .bind(id.to_string())
            .execute(&pool)
            .await
            .unwrap();

        GoalQueries::record_verdict(
            &pool,
            id,
            ravenbot_core::GoalVerdict::Continue,
            "first cycle",
            0,
            false,
        )
        .await
        .expect("first")
        .expect("goal");
        let paused = GoalQueries::record_verdict(
            &pool,
            id,
            ravenbot_core::GoalVerdict::Continue,
            "second cycle",
            0,
            false,
        )
        .await
        .expect("second")
        .expect("goal");
        assert_eq!(paused.status, "paused");
        assert!(paused.next_run_at.is_none());
        assert!(paused.last_reason.contains("Auto-continuation budget"));
    }

    #[tokio::test]
    async fn a_due_goal_is_returned_once_and_a_future_goal_is_not() {
        let pool = db().await;
        let office = Uuid::new_v4();
        let goal = GoalQueries::ensure_office_goal(&pool, office, "Ship it", 4)
            .await
            .expect("ensure");
        let id = Uuid::parse_str(&goal.id).unwrap();
        let past = (Utc::now() - chrono::Duration::seconds(5)).to_rfc3339();
        sqlx::query("UPDATE goals SET next_run_at = ? WHERE id = ?")
            .bind(&past)
            .bind(id.to_string())
            .execute(&pool)
            .await
            .unwrap();
        let due = GoalQueries::due_for_autopilot(&pool, &Utc::now().to_rfc3339(), 10)
            .await
            .expect("due");
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id, goal.id);

        let future = (Utc::now() + chrono::Duration::minutes(5)).to_rfc3339();
        sqlx::query("UPDATE goals SET next_run_at = ? WHERE id = ?")
            .bind(&future)
            .bind(id.to_string())
            .execute(&pool)
            .await
            .unwrap();
        let due = GoalQueries::due_for_autopilot(&pool, &Utc::now().to_rfc3339(), 10)
            .await
            .expect("due");
        assert!(due.is_empty());
    }

    #[tokio::test]
    async fn office_token_usage_counts_office_threads_and_never_decreases() {
        let pool = db().await;
        let room = ravenbot_core::ChatRoom::new("Billing", "invoices", "custom");
        ChatRoomQueries::create(&pool, &room).await.expect("room");
        let bot = ravenbot_core::Bot::new("Clerk", "billing");
        BotQueries::insert(&pool, &bot).await.expect("bot");
        let office_thread = ravenbot_core::Thread::new(bot.id, "Office");
        ThreadQueries::create(&pool, &office_thread)
            .await
            .expect("thread");
        sqlx::query(
            "INSERT INTO chatroom_office_threads (chatroom_id, thread_id, created_at)
             VALUES (?, ?, ?)",
        )
        .bind(room.id.to_string())
        .bind(office_thread.id.to_string())
        .bind(Utc::now().to_rfc3339())
        .execute(&pool)
        .await
        .expect("link");

        let mut office_run = ravenbot_core::Run::new(bot.id, office_thread.id);
        office_run.tokens_consumed = 40;
        RunQueries::insert(&pool, &office_run).await.expect("run");
        let outside_thread = ravenbot_core::Thread::new(bot.id, "Elsewhere");
        ThreadQueries::create(&pool, &outside_thread)
            .await
            .expect("outside thread");
        let mut outside_run = ravenbot_core::Run::new(bot.id, outside_thread.id);
        outside_run.tokens_consumed = 100;
        RunQueries::insert(&pool, &outside_run)
            .await
            .expect("outside run");

        let since = (Utc::now() - chrono::Duration::minutes(1)).to_rfc3339();
        assert_eq!(
            GoalQueries::tokens_for_office_since(&pool, room.id, &since)
                .await
                .expect("usage"),
            40
        );

        let goal = GoalQueries::ensure_office_goal(&pool, room.id, "Ship it", 4)
            .await
            .expect("goal");
        let goal_id = Uuid::parse_str(&goal.id).unwrap();
        GoalQueries::set_tokens_used(&pool, goal_id, 40)
            .await
            .expect("set usage");
        GoalQueries::set_tokens_used(&pool, goal_id, 10)
            .await
            .expect("lower usage");
        let stored = GoalQueries::get(&pool, goal_id)
            .await
            .expect("get")
            .expect("goal");
        assert_eq!(stored.tokens_used, 40, "usage is monotonic");
    }

    #[tokio::test]
    async fn office_detail_contains_gates_and_their_results() {
        let pool = db().await;
        let office = Uuid::new_v4();
        let goal = GoalQueries::ensure_office_goal(&pool, office, "Ship it", 4)
            .await
            .expect("ensure");
        let goal_id = Uuid::parse_str(&goal.id).unwrap();
        let mut contract = goal.contract.clone();
        contract.gates = vec![ravenbot_core::GoalGate::new("npm test")];
        GoalQueries::update_contract(&pool, goal_id, &contract)
            .await
            .expect("update");

        QualityGateQueries::record(
            &pool,
            office,
            None,
            0,
            "npm test",
            "/tmp/office",
            Some(1),
            false,
            false,
            500,
            "FAIL",
            Some(goal_id),
        )
        .await
        .expect("gate run");

        let detail = GoalQueries::office_detail(&pool, office)
            .await
            .expect("detail")
            .expect("goal detail");
        assert_eq!(detail.gates.len(), 1);
        assert_eq!(detail.gate_runs.len(), 1);
        assert_eq!(
            detail.gate_runs[0].goal_id.as_deref(),
            Some(goal.id.as_str())
        );
        assert!(!detail.gate_runs[0].passed);
    }

    #[test]
    fn contract_normalization_bounds_gates_and_timeouts() {
        let mut contract = ravenbot_core::GoalContract::for_objective("  Ship it  ");
        contract.gates = (0..12)
            .map(|index| {
                let mut gate = ravenbot_core::GoalGate::new(format!("cmd {index}"));
                gate.timeout_secs = 0;
                gate
            })
            .collect();
        contract.max_turns = 999;
        contract.normalize();
        assert_eq!(contract.objective, "Ship it");
        assert_eq!(contract.gates.len(), 4);
        assert_eq!(contract.gates[0].timeout_secs, 1);
        assert_eq!(contract.max_turns, 64);
        assert!(contract.gates.iter().all(|gate| gate.position < 4));
    }
}

#[cfg(test)]
mod context_summary_tests {
    use super::*;
    use crate::migrations;

    async fn db() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        migrations::run(&pool).await.expect("migrations");
        pool
    }

    #[tokio::test]
    async fn a_summary_has_a_floor_and_later_messages_survive_it() {
        let pool = db().await;
        let bot = ravenbot_core::Bot::new("Summarizer", "context");
        BotQueries::insert(&pool, &bot).await.expect("bot");
        let thread = ravenbot_core::Thread::new(bot.id, "Long project");
        ThreadQueries::create(&pool, &thread).await.expect("thread");
        let first = ravenbot_core::Message::user(thread.id, "first request");
        let second = ravenbot_core::Message::user(thread.id, "second request");
        let third = ravenbot_core::Message::assistant(thread.id, "latest answer");
        for message in [&first, &second, &third] {
            MessageQueries::insert(&pool, message)
                .await
                .expect("message");
        }

        let summary = ContextSummaryQueries::save(
            &pool,
            thread.id,
            second.id,
            "[Compacted context handoff]\n## Task\nBuild it",
            42,
            "test-model",
        )
        .await
        .expect("save summary");
        let latest = ContextSummaryQueries::latest(&pool, thread.id)
            .await
            .expect("latest")
            .expect("summary");
        assert_eq!(latest.id, summary.to_string());
        assert_eq!(latest.through_message_id, Some(second.id.to_string()));
        assert_eq!(latest.token_estimate, 42);

        let after = MessageQueries::list_after(&pool, thread.id, second.id)
            .await
            .expect("after");
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].id, third.id);
        assert!(MessageQueries::exists(&pool, second.id)
            .await
            .expect("exists"));

        let pointers: (Option<String>, Option<String>) = sqlx::query_as(
            "SELECT context_summary_id, context_floor_message_id FROM threads WHERE id = ?",
        )
        .bind(thread.id.to_string())
        .fetch_one(&pool)
        .await
        .expect("pointers");
        assert_eq!(pointers.0, Some(summary.to_string()));
        assert_eq!(pointers.1, Some(second.id.to_string()));

        ContextSummaryQueries::clear(&pool, thread.id)
            .await
            .expect("clear");
        assert!(ContextSummaryQueries::latest(&pool, thread.id)
            .await
            .expect("latest after clear")
            .is_none());
    }
}

#[cfg(test)]
mod remote_access_tests {
    use super::*;
    use crate::migrations;

    async fn db() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        migrations::run(&pool).await.expect("migrations");
        pool
    }

    #[tokio::test]
    async fn a_pairing_code_is_single_use_and_expires() {
        let pool = db().await;
        RemoteAccessQueries::create_pairing(&pool, "hash-1", "operator", 120)
            .await
            .expect("pairing");
        let first = RemoteAccessQueries::consume_pairing(&pool, "hash-1")
            .await
            .expect("consume");
        assert_eq!(first.expect("first use").1, "operator");
        let second = RemoteAccessQueries::consume_pairing(&pool, "hash-1")
            .await
            .expect("consume again");
        assert!(second.is_none(), "a pairing code must not be reusable");

        sqlx::query(
            "INSERT INTO remote_pairings (id, code_hash, role, expires_at)
             VALUES ('expired', 'hash-2', 'viewer', datetime('now', '-1 minute'))",
        )
        .execute(&pool)
        .await
        .expect("expired row");
        assert!(RemoteAccessQueries::consume_pairing(&pool, "hash-2")
            .await
            .expect("consume expired")
            .is_none());
    }

    #[tokio::test]
    async fn a_revoked_device_cannot_authenticate() {
        let pool = db().await;
        let id = RemoteAccessQueries::create_device(&pool, "Phone", "viewer", "token-hash")
            .await
            .expect("device");
        let device = RemoteAccessQueries::device_for_token(&pool, "token-hash")
            .await
            .expect("lookup")
            .expect("device");
        assert_eq!(device.name, "Phone");
        assert_eq!(device.role, "viewer");
        assert!(device.account_id.is_none());

        assert!(RemoteAccessQueries::revoke_device(&pool, id)
            .await
            .expect("revoke"));
        assert!(RemoteAccessQueries::device_for_token(&pool, "token-hash")
            .await
            .expect("lookup after revoke")
            .is_none());
    }

    #[tokio::test]
    async fn an_account_binding_is_persisted_and_can_be_cleared() {
        let pool = db().await;
        let id = RemoteAccessQueries::create_device(&pool, "Phone", "operator", "token-hash")
            .await
            .expect("device");
        assert!(RemoteAccessQueries::bind_account(
            &pool,
            &id.to_string(),
            "account-1",
            "alice",
            "viewer"
        )
        .await
        .expect("bind"));
        let device = RemoteAccessQueries::device_for_token(&pool, "token-hash")
            .await
            .expect("lookup")
            .expect("device");
        assert_eq!(device.account_id.as_deref(), Some("account-1"));
        assert_eq!(device.account_name.as_deref(), Some("alice"));
        assert_eq!(device.account_role.as_deref(), Some("viewer"));
        let listed = RemoteAccessQueries::list_devices(&pool)
            .await
            .expect("list");
        assert_eq!(listed[0].account_name.as_deref(), Some("alice"));

        RemoteAccessQueries::clear_account_binding(&pool, &id.to_string())
            .await
            .expect("clear");
        let device = RemoteAccessQueries::device_for_token(&pool, "token-hash")
            .await
            .expect("lookup")
            .expect("device");
        assert!(device.account_id.is_none());
        assert!(device.account_name.is_none());
        assert!(device.account_role.is_none());
    }

    #[tokio::test]
    async fn remote_requests_leave_an_attributable_audit_row() {
        let pool = db().await;
        RemoteAccessQueries::audit(
            &pool,
            Some("device-1"),
            Some("Phone"),
            Some("operator"),
            Some("alice"),
            "POST",
            "/api/threads/abc/messages",
            200,
        )
        .await
        .expect("audit");
        let rows = RemoteAccessQueries::recent_audit(&pool, 10)
            .await
            .expect("recent");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].device_name.as_deref(), Some("Phone"));
        assert_eq!(rows[0].account_name.as_deref(), Some("alice"));
        assert_eq!(rows[0].method, "POST");
        assert_eq!(rows[0].status, 200);
    }
}

#[cfg(test)]
mod telegram_tests {
    use super::*;
    use crate::migrations;

    async fn db() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        migrations::run(&pool).await.expect("migrations");
        pool
    }

    #[tokio::test]
    async fn a_telegram_pairing_code_is_single_use() {
        let pool = db().await;
        let bot_id = Uuid::new_v4();
        TelegramQueries::create_pairing(&pool, "hash", bot_id, 120)
            .await
            .expect("pairing");
        assert_eq!(
            TelegramQueries::consume_pairing(&pool, "hash")
                .await
                .expect("consume"),
            Some(bot_id)
        );
        assert!(TelegramQueries::consume_pairing(&pool, "hash")
            .await
            .expect("replay")
            .is_none());
    }

    #[tokio::test]
    async fn a_chat_link_can_be_relinked_and_removed() {
        let pool = db().await;
        let bot_id = Uuid::new_v4();
        let thread_id = Uuid::new_v4();
        TelegramQueries::upsert_link(&pool, "42", Some("Phone"), bot_id, thread_id)
            .await
            .expect("link");
        let link = TelegramQueries::link_for_chat(&pool, "42")
            .await
            .expect("lookup")
            .expect("link");
        assert_eq!(link.thread_id, thread_id.to_string());
        assert_eq!(link.chat_title.as_deref(), Some("Phone"));

        let new_thread = Uuid::new_v4();
        TelegramQueries::upsert_link(&pool, "42", Some("Phone 2"), bot_id, new_thread)
            .await
            .expect("relink");
        assert_eq!(
            TelegramQueries::link_for_chat(&pool, "42")
                .await
                .expect("lookup")
                .expect("link")
                .thread_id,
            new_thread.to_string()
        );
        assert!(TelegramQueries::unlink(&pool, "42").await.expect("unlink"));
        assert!(TelegramQueries::link_for_chat(&pool, "42")
            .await
            .expect("lookup")
            .is_none());
    }
}

#[cfg(test)]
mod slack_tests {
    use super::*;
    use crate::migrations;

    async fn db() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        migrations::run(&pool).await.expect("migrations");
        pool
    }

    #[tokio::test]
    async fn a_slack_pairing_code_is_single_use() {
        let pool = db().await;
        let bot_id = Uuid::new_v4();
        SlackQueries::create_pairing(&pool, "hash", bot_id, 120)
            .await
            .expect("pairing");
        assert_eq!(
            SlackQueries::consume_pairing(&pool, "hash")
                .await
                .expect("consume"),
            Some(bot_id)
        );
        assert!(SlackQueries::consume_pairing(&pool, "hash")
            .await
            .expect("replay")
            .is_none());
    }

    #[tokio::test]
    async fn a_redelivered_slack_event_is_handled_once() {
        let pool = db().await;
        assert!(SlackQueries::claim_event(&pool, "C1", "1700000000.000100")
            .await
            .expect("first"));
        assert!(!SlackQueries::claim_event(&pool, "C1", "1700000000.000100")
            .await
            .expect("duplicate"));
        assert!(SlackQueries::claim_event(&pool, "C1", "1700000000.000200")
            .await
            .expect("new timestamp"));
        assert!(SlackQueries::claim_event(&pool, "C2", "1700000000.000100")
            .await
            .expect("other channel"));
    }

    #[tokio::test]
    async fn a_slack_channel_can_be_relinked_and_removed() {
        let pool = db().await;
        let bot_id = Uuid::new_v4();
        let first = Uuid::new_v4();
        SlackQueries::upsert_link(&pool, "C1", Some("general"), bot_id, first)
            .await
            .expect("link");
        assert_eq!(
            SlackQueries::link_for_channel(&pool, "C1")
                .await
                .expect("lookup")
                .expect("link")
                .thread_id,
            first.to_string()
        );
        let second = Uuid::new_v4();
        SlackQueries::upsert_link(&pool, "C1", Some("general"), bot_id, second)
            .await
            .expect("relink");
        assert_eq!(
            SlackQueries::link_for_channel(&pool, "C1")
                .await
                .expect("lookup")
                .expect("link")
                .thread_id,
            second.to_string()
        );
        assert!(SlackQueries::unlink(&pool, "C1").await.expect("unlink"));
        assert!(SlackQueries::link_for_channel(&pool, "C1")
            .await
            .expect("lookup")
            .is_none());
    }
}

#[cfg(test)]
mod discord_tests {
    use super::*;
    use crate::migrations;

    async fn db() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        migrations::run(&pool).await.expect("migrations");
        pool
    }

    #[tokio::test]
    async fn a_discord_pairing_code_is_single_use() {
        let pool = db().await;
        let bot_id = Uuid::new_v4();
        DiscordQueries::create_pairing(&pool, "hash", bot_id, 120)
            .await
            .expect("pairing");
        assert_eq!(
            DiscordQueries::consume_pairing(&pool, "hash")
                .await
                .expect("consume"),
            Some(bot_id)
        );
        assert!(DiscordQueries::consume_pairing(&pool, "hash")
            .await
            .expect("replay")
            .is_none());
    }

    #[tokio::test]
    async fn a_discord_channel_can_be_relinked_and_removed() {
        let pool = db().await;
        let bot_id = Uuid::new_v4();
        let first = Uuid::new_v4();
        DiscordQueries::upsert_link(&pool, "D1", Some("G1"), Some("general"), bot_id, first)
            .await
            .expect("link");
        let link = DiscordQueries::link_for_channel(&pool, "D1")
            .await
            .expect("lookup")
            .expect("link");
        assert_eq!(link.thread_id, first.to_string());
        assert_eq!(link.guild_id.as_deref(), Some("G1"));
        let second = Uuid::new_v4();
        DiscordQueries::upsert_link(&pool, "D1", Some("G1"), Some("general"), bot_id, second)
            .await
            .expect("relink");
        assert_eq!(
            DiscordQueries::link_for_channel(&pool, "D1")
                .await
                .expect("lookup")
                .expect("link")
                .thread_id,
            second.to_string()
        );
        assert!(DiscordQueries::unlink(&pool, "D1").await.expect("unlink"));
        assert!(DiscordQueries::link_for_channel(&pool, "D1")
            .await
            .expect("lookup")
            .is_none());
    }

    #[tokio::test]
    async fn a_redelivered_discord_message_is_handled_once() {
        let pool = db().await;
        assert!(DiscordQueries::claim_event(&pool, "D1", "m1")
            .await
            .expect("first"));
        assert!(!DiscordQueries::claim_event(&pool, "D1", "m1")
            .await
            .expect("duplicate"));
        assert!(DiscordQueries::claim_event(&pool, "D1", "m2")
            .await
            .expect("new message"));
        assert!(DiscordQueries::claim_event(&pool, "D2", "m1")
            .await
            .expect("other channel"));
    }
}

#[cfg(test)]
mod whatsapp_tests {
    use super::*;
    use crate::migrations;

    async fn db() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        migrations::run(&pool).await.expect("migrations");
        pool
    }

    #[tokio::test]
    async fn a_whatsapp_pairing_code_is_single_use() {
        let pool = db().await;
        let bot_id = Uuid::new_v4();
        WhatsAppQueries::create_pairing(&pool, "hash", bot_id, 120)
            .await
            .expect("pairing");
        assert_eq!(
            WhatsAppQueries::consume_pairing(&pool, "hash")
                .await
                .expect("consume"),
            Some(bot_id)
        );
        assert!(WhatsAppQueries::consume_pairing(&pool, "hash")
            .await
            .expect("replay")
            .is_none());
    }

    #[tokio::test]
    async fn a_whatsapp_number_can_be_relinked_and_removed() {
        let pool = db().await;
        let bot_id = Uuid::new_v4();
        let first = Uuid::new_v4();
        WhatsAppQueries::upsert_link(&pool, "15551234567", Some("Ada"), bot_id, first)
            .await
            .expect("link");
        let link = WhatsAppQueries::link_for_number(&pool, "15551234567")
            .await
            .expect("lookup")
            .expect("link");
        assert_eq!(link.thread_id, first.to_string());
        assert_eq!(link.display_name.as_deref(), Some("Ada"));
        let second = Uuid::new_v4();
        WhatsAppQueries::upsert_link(&pool, "15551234567", Some("Ada"), bot_id, second)
            .await
            .expect("relink");
        assert_eq!(
            WhatsAppQueries::link_for_number(&pool, "15551234567")
                .await
                .expect("lookup")
                .expect("link")
                .thread_id,
            second.to_string()
        );
        assert!(WhatsAppQueries::unlink(&pool, "15551234567")
            .await
            .expect("unlink"));
        assert!(WhatsAppQueries::link_for_number(&pool, "15551234567")
            .await
            .expect("lookup")
            .is_none());
    }

    #[tokio::test]
    async fn a_redelivered_whatsapp_message_is_handled_once() {
        let pool = db().await;
        assert!(
            WhatsAppQueries::claim_event(&pool, "15551234567", "wamid.1")
                .await
                .expect("first")
        );
        assert!(
            !WhatsAppQueries::claim_event(&pool, "15551234567", "wamid.1")
                .await
                .expect("duplicate")
        );
        assert!(
            WhatsAppQueries::claim_event(&pool, "15551234567", "wamid.2")
                .await
                .expect("new message")
        );
        assert!(
            WhatsAppQueries::claim_event(&pool, "15557654321", "wamid.1")
                .await
                .expect("other number")
        );
    }
}

#[cfg(test)]
mod teams_tests {
    use super::*;
    use crate::migrations;

    async fn db() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        migrations::run(&pool).await.expect("migrations");
        pool
    }

    #[tokio::test]
    async fn a_teams_pairing_code_is_single_use() {
        let pool = db().await;
        let bot_id = Uuid::new_v4();
        TeamsQueries::create_pairing(&pool, "hash", bot_id, 120)
            .await
            .expect("pairing");
        assert_eq!(
            TeamsQueries::consume_pairing(&pool, "hash")
                .await
                .expect("consume"),
            Some(bot_id)
        );
        assert!(TeamsQueries::consume_pairing(&pool, "hash")
            .await
            .expect("replay")
            .is_none());
    }

    #[tokio::test]
    async fn a_teams_conversation_can_be_relinked_and_removed() {
        let pool = db().await;
        let bot_id = Uuid::new_v4();
        let first = Uuid::new_v4();
        TeamsQueries::upsert_link(
            &pool,
            "conv-1",
            "tenant-1",
            Some("personal"),
            "https://smba.example",
            Some("Ada"),
            bot_id,
            first,
        )
        .await
        .expect("link");
        let link = TeamsQueries::link_for_conversation(&pool, "conv-1")
            .await
            .expect("lookup")
            .expect("link");
        assert_eq!(link.thread_id, first.to_string());
        assert_eq!(link.tenant_id, "tenant-1");
        assert_eq!(link.service_url, "https://smba.example");
        let second = Uuid::new_v4();
        TeamsQueries::upsert_link(
            &pool,
            "conv-1",
            "tenant-1",
            Some("personal"),
            "https://smba.example",
            Some("Ada"),
            bot_id,
            second,
        )
        .await
        .expect("relink");
        assert_eq!(
            TeamsQueries::link_for_conversation(&pool, "conv-1")
                .await
                .expect("lookup")
                .expect("link")
                .thread_id,
            second.to_string()
        );
        assert!(TeamsQueries::unlink(&pool, "conv-1").await.expect("unlink"));
        assert!(TeamsQueries::link_for_conversation(&pool, "conv-1")
            .await
            .expect("lookup")
            .is_none());
    }

    #[tokio::test]
    async fn a_redelivered_teams_activity_is_handled_once() {
        let pool = db().await;
        assert!(TeamsQueries::claim_event(&pool, "conv-1", "activity-1")
            .await
            .expect("first"));
        assert!(!TeamsQueries::claim_event(&pool, "conv-1", "activity-1")
            .await
            .expect("duplicate"));
        assert!(TeamsQueries::claim_event(&pool, "conv-1", "activity-2")
            .await
            .expect("new activity"));
        assert!(TeamsQueries::claim_event(&pool, "conv-2", "activity-1")
            .await
            .expect("other conversation"));
    }
}

#[cfg(test)]
mod run_actor_tests {
    use super::*;
    use crate::migrations;

    #[tokio::test]
    async fn a_run_records_the_acting_account_and_workspace() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        migrations::run(&pool).await.expect("migrations");
        let run_id = Uuid::new_v4();
        RunQueries::set_actor(&pool, run_id, "actor-1", "alice", "operator", "workspace-1")
            .await
            .expect("actor");
        let actor = RunQueries::actor_for(&pool, run_id)
            .await
            .expect("lookup")
            .expect("actor");
        assert_eq!(actor.actor_name, "alice");
        assert_eq!(actor.role, "operator");
        assert_eq!(actor.workspace_id, "workspace-1");
    }
}

#[cfg(test)]
mod usage_tests {
    use super::*;
    use crate::migrations;

    async fn db() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        migrations::run(&pool).await.expect("migrations");
        pool
    }

    #[tokio::test]
    async fn usage_aggregates_runs_by_agent() {
        let pool = db().await;
        let first = ravenbot_core::Bot::new("Billing", "ledger");
        let second = ravenbot_core::Bot::new("Research", "sources");
        BotQueries::insert(&pool, &first).await.expect("bot 1");
        BotQueries::insert(&pool, &second).await.expect("bot 2");

        let thread = ravenbot_core::Thread::new(first.id, "Month close");
        ThreadQueries::create(&pool, &thread).await.expect("thread");

        for (bot_id, tokens, cost, state) in [
            (first.id, 100_i64, 0.25_f64, "completed"),
            (first.id, 50, 0.5, "failed"),
            (second.id, 20, 0.1, "completed"),
        ] {
            let run = ravenbot_core::Run::new(bot_id, thread.id);
            RunQueries::insert(&pool, &run).await.expect("run");
            sqlx::query(
                "UPDATE runs SET tokens_consumed = ?, cost_estimate = ?, state = ? WHERE id = ?",
            )
            .bind(tokens)
            .bind(cost)
            .bind(state)
            .bind(run.id.to_string())
            .execute(&pool)
            .await
            .expect("set usage");
        }

        let rows = UsageQueries::summary(&pool, 0).await.expect("summary");
        assert_eq!(rows.len(), 2);
        let billing = rows
            .iter()
            .find(|(id, ..)| id == &first.id.to_string())
            .expect("billing");
        assert_eq!(
            (billing.1, billing.2, billing.3, billing.4, billing.5),
            (2, 150, 0.75, 1, 1)
        );
        let research = rows
            .iter()
            .find(|(id, ..)| id == &second.id.to_string())
            .expect("research");
        assert_eq!(
            (research.1, research.2, research.3, research.4, research.5),
            (1, 20, 0.1, 0, 1)
        );

        // The 7-day window contains runs created now.
        let recent = UsageQueries::summary(&pool, 7).await.expect("recent");
        assert_eq!(recent.len(), 2);
    }

    #[tokio::test]
    async fn run_spend_round_trips_model_and_tool_buckets() {
        let pool = db().await;
        let bot = ravenbot_core::Bot::new("Metered", "ledger");
        BotQueries::insert(&pool, &bot).await.expect("bot");
        let thread = ravenbot_core::Thread::new(bot.id, "Metered thread");
        ThreadQueries::create(&pool, &thread).await.expect("thread");

        let mut run = ravenbot_core::Run::new(bot.id, thread.id);
        run.add_model_usage(100, 0.30);
        run.add_tool_usage(0.05);
        RunQueries::insert(&pool, &run).await.expect("run");

        let mut loaded = RunQueries::get(&pool, run.id)
            .await
            .expect("read")
            .expect("run exists");
        assert!((loaded.model_cost - 0.30).abs() < 1e-12);
        assert!((loaded.tool_cost - 0.05).abs() < 1e-12);
        assert!((loaded.cost_estimate - 0.35).abs() < 1e-12);

        loaded.add_tool_usage(0.02);
        RunQueries::update(&pool, &loaded).await.expect("update");
        let reloaded = RunQueries::get(&pool, run.id)
            .await
            .expect("read")
            .expect("run exists");
        assert!((reloaded.tool_cost - 0.07).abs() < 1e-12);
        assert!((reloaded.cost_estimate - 0.37).abs() < 1e-12);
    }

    #[tokio::test]
    async fn migration_50_backfills_old_run_totals_as_model_spend() {
        let pool = db().await;
        let bot = ravenbot_core::Bot::new("Legacy", "old ledger");
        BotQueries::insert(&pool, &bot).await.expect("bot");
        let thread = ravenbot_core::Thread::new(bot.id, "Legacy thread");
        ThreadQueries::create(&pool, &thread).await.expect("thread");
        let run = ravenbot_core::Run::new(bot.id, thread.id);
        RunQueries::insert(&pool, &run).await.expect("run");
        // The shape a pre-50 run has: a total, no split.
        sqlx::query(
            "UPDATE runs SET cost_estimate = 0.9, model_cost = 0, tool_cost = 0 WHERE id = ?",
        )
        .bind(run.id.to_string())
        .execute(&pool)
        .await
        .expect("legacy row");

        // Re-apply migration 50. The ALTERs report duplicate columns, which the
        // runner tolerates; the backfill must still run.
        sqlx::query("DELETE FROM _migrations WHERE version >= 50")
            .execute(&pool)
            .await
            .expect("forget versions");
        migrations::run(&pool).await.expect("reapply migration");

        let (model, tool) = UsageQueries::spend_breakdown(&pool, 0)
            .await
            .expect("breakdown");
        assert!(
            (model - 0.9).abs() < 1e-12,
            "backfilled model spend: {model}"
        );
        assert_eq!(tool, 0.0);
    }

    #[tokio::test]
    async fn spend_breakdown_separates_model_tool_and_utility() {
        let pool = db().await;
        let bot = ravenbot_core::Bot::new("Split", "ledger");
        BotQueries::insert(&pool, &bot).await.expect("bot");
        let thread = ravenbot_core::Thread::new(bot.id, "Split thread");
        ThreadQueries::create(&pool, &thread).await.expect("thread");

        let mut run = ravenbot_core::Run::new(bot.id, thread.id);
        run.add_model_usage(80, 0.20);
        run.add_tool_usage(0.03);
        RunQueries::insert(&pool, &run).await.expect("run");

        UtilityUsageQueries::record(&pool, bot.id, 40, 0.10)
            .await
            .expect("utility row");
        UtilityUsageQueries::record(&pool, bot.id, 20, 0.05)
            .await
            .expect("utility row");

        let (model, tool) = UsageQueries::spend_breakdown(&pool, 0)
            .await
            .expect("breakdown");
        assert!((model - 0.20).abs() < 1e-12, "model spend: {model}");
        assert!((tool - 0.03).abs() < 1e-12, "tool spend: {tool}");
        let (utility_tokens, utility_cost) =
            UtilityUsageQueries::total(&pool, 0).await.expect("utility");
        assert_eq!(utility_tokens, 60);
        assert!((utility_cost - 0.15).abs() < 1e-12);

        // The window is inclusive of rows written now.
        let (recent_model, _) = UsageQueries::spend_breakdown(&pool, 7)
            .await
            .expect("recent breakdown");
        assert!((recent_model - 0.20).abs() < 1e-12);
    }
}

/// Inbound workflow webhook routing. Only the SHA-256 of the URL token is
/// stored; the token is shown once when the trigger is enabled or rotated.
pub struct WorkflowWebhookQueries;

impl WorkflowWebhookQueries {
    pub async fn enable(
        pool: &SqlitePool,
        workflow_id: Uuid,
        secret: &str,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO workflow_webhooks
             (workflow_id, secret_hash, enabled, created_at, updated_at)
             VALUES (?, ?, 1, ?, ?)
             ON CONFLICT(workflow_id) DO UPDATE SET
               secret_hash = excluded.secret_hash,
               enabled = 1,
               updated_at = excluded.updated_at",
        )
        .bind(workflow_id.to_string())
        .bind(hash_webhook_secret(secret))
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn disable(pool: &SqlitePool, workflow_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM workflow_webhooks WHERE workflow_id = ?")
            .bind(workflow_id.to_string())
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn is_enabled(pool: &SqlitePool, workflow_id: Uuid) -> Result<bool, sqlx::Error> {
        let row: Option<(i64,)> =
            sqlx::query_as("SELECT enabled FROM workflow_webhooks WHERE workflow_id = ?")
                .bind(workflow_id.to_string())
                .fetch_optional(pool)
                .await?;
        Ok(row.map(|(enabled,)| enabled != 0).unwrap_or(false))
    }

    pub async fn workflow_for_secret(
        pool: &SqlitePool,
        secret: &str,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT workflow_id FROM workflow_webhooks
             WHERE secret_hash = ? AND enabled = 1",
        )
        .bind(hash_webhook_secret(secret))
        .fetch_optional(pool)
        .await?;
        Ok(row.and_then(|(id,)| Uuid::parse_str(&id).ok()))
    }
}

/// Local credential vault metadata. Secrets are resolved separately and never
/// included in a list response.
#[derive(Debug, Clone, Serialize)]
pub struct VaultItemMeta {
    pub id: String,
    pub kind: String,
    pub name: String,
    /// The machine-safe name of an API key; `None` for other vault kinds.
    pub key_name: Option<String>,
    pub username: String,
    pub website: String,
    pub last4: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub struct VaultQueries;

impl VaultQueries {
    pub async fn list(pool: &SqlitePool) -> Result<Vec<VaultItemMeta>, sqlx::Error> {
        type Row = (
            String,
            String,
            String,
            Option<String>,
            String,
            String,
            Option<String>,
            String,
            String,
        );
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT id, kind, name, key_name, username, website, last4, created_at, updated_at
             FROM vault_items ORDER BY updated_at DESC",
        )
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(id, kind, name, key_name, username, website, last4, created_at, updated_at)| {
                    VaultItemMeta {
                        id,
                        kind,
                        name,
                        key_name,
                        username,
                        website,
                        last4,
                        created_at,
                        updated_at,
                    }
                },
            )
            .collect())
    }

    pub async fn get_secret(pool: &SqlitePool, id: &str) -> Result<Option<String>, sqlx::Error> {
        let stored: Option<(String,)> =
            sqlx::query_as("SELECT secret FROM vault_items WHERE id = ?")
                .bind(id)
                .fetch_optional(pool)
                .await?;
        let Some((stored,)) = stored else {
            return Ok(None);
        };
        if stored == crate::secrets::KEYCHAIN_MARKER {
            return Ok(crate::secrets::get_vault_secret(id).await);
        }
        if stored.trim().is_empty() {
            return Ok(None);
        }
        if crate::secrets::set_vault_secret(id, &stored).await {
            let _ = sqlx::query("UPDATE vault_items SET secret = ?, updated_at = ? WHERE id = ?")
                .bind(crate::secrets::KEYCHAIN_MARKER)
                .bind(Utc::now().to_rfc3339())
                .bind(id)
                .execute(pool)
                .await;
        }
        Ok(Some(stored))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn upsert(
        pool: &SqlitePool,
        id: &str,
        kind: &str,
        name: &str,
        username: &str,
        website: &str,
        last4: Option<&str>,
        secret: &str,
    ) -> Result<(), sqlx::Error> {
        let stored = if crate::secrets::set_vault_secret(id, secret).await {
            crate::secrets::KEYCHAIN_MARKER
        } else {
            secret
        };
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO vault_items (id, kind, name, username, website, last4, secret, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
               kind = excluded.kind,
               name = excluded.name,
               username = excluded.username,
               website = excluded.website,
               last4 = excluded.last4,
               secret = excluded.secret,
               updated_at = excluded.updated_at",
        )
        .bind(id)
        .bind(kind)
        .bind(name)
        .bind(username)
        .bind(website)
        .bind(last4)
        .bind(stored)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn delete(pool: &SqlitePool, id: &str) -> Result<(), sqlx::Error> {
        crate::secrets::delete_vault_secret(id).await;
        sqlx::query("DELETE FROM vault_items WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }
}

/// One durable office graph snapshot.
///
/// `labels` is a JSON object mapping node ids to the teammate label the board
/// and synthesis use; `graph` is the serialized `TaskGraph`. Columns are kept
/// as strings so the database crate does not depend on the runtime's graph
/// types.
#[derive(Debug, Clone, Serialize)]
pub struct OfficeGraphRecord {
    pub id: String,
    pub chatroom_id: String,
    pub thread_id: String,
    pub cycle: i64,
    pub request: String,
    pub visible: String,
    pub goal: String,
    pub labels: String,
    pub graph: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

pub struct OfficeGraphQueries;

impl OfficeGraphQueries {
    /// Insert or update one graph snapshot.
    ///
    /// The writer runs about once a second while a graph executes, so this is
    /// deliberately a single upsert. `created_at` belongs to the first write.
    #[allow(clippy::too_many_arguments)]
    pub async fn save(
        pool: &SqlitePool,
        id: Uuid,
        chatroom_id: Uuid,
        thread_id: Uuid,
        cycle: u32,
        request: &str,
        visible: &str,
        goal: &str,
        labels: &str,
        graph: &str,
        status: &str,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO office_graphs
             (id, chatroom_id, thread_id, cycle, request, visible, goal, labels, graph,
              status, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
               chatroom_id = excluded.chatroom_id,
               thread_id = excluded.thread_id,
               cycle = excluded.cycle,
               request = excluded.request,
               visible = excluded.visible,
               goal = excluded.goal,
               labels = excluded.labels,
               graph = excluded.graph,
               status = excluded.status,
               updated_at = excluded.updated_at",
        )
        .bind(id.to_string())
        .bind(chatroom_id.to_string())
        .bind(thread_id.to_string())
        .bind(i64::from(cycle))
        .bind(request)
        .bind(visible)
        .bind(goal)
        .bind(labels)
        .bind(graph)
        .bind(status)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// The newest graph that was still running when the process stopped.
    pub async fn latest_running(
        pool: &SqlitePool,
        chatroom_id: Uuid,
    ) -> Result<Option<OfficeGraphRecord>, sqlx::Error> {
        let row: Option<(
            String,
            String,
            String,
            i64,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
        )> = sqlx::query_as(
            "SELECT id, chatroom_id, thread_id, cycle, request, visible, goal, labels,
                    graph, status, created_at, updated_at
             FROM office_graphs
             WHERE chatroom_id = ? AND status = 'running'
             ORDER BY updated_at DESC
             LIMIT 1",
        )
        .bind(chatroom_id.to_string())
        .fetch_optional(pool)
        .await?;
        Ok(row.map(
            |(
                id,
                chatroom_id,
                thread_id,
                cycle,
                request,
                visible,
                goal,
                labels,
                graph,
                status,
                created_at,
                updated_at,
            )| OfficeGraphRecord {
                id,
                chatroom_id,
                thread_id,
                cycle,
                request,
                visible,
                goal,
                labels,
                graph,
                status,
                created_at,
                updated_at,
            },
        ))
    }

    /// Load one saved graph by id.
    pub async fn get(
        pool: &SqlitePool,
        id: Uuid,
    ) -> Result<Option<OfficeGraphRecord>, sqlx::Error> {
        let row: Option<(
            String,
            String,
            String,
            i64,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
        )> = sqlx::query_as(
            "SELECT id, chatroom_id, thread_id, cycle, request, visible, goal, labels,
                    graph, status, created_at, updated_at
             FROM office_graphs WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(pool)
        .await?;
        Ok(row.map(
            |(
                id,
                chatroom_id,
                thread_id,
                cycle,
                request,
                visible,
                goal,
                labels,
                graph,
                status,
                created_at,
                updated_at,
            )| OfficeGraphRecord {
                id,
                chatroom_id,
                thread_id,
                cycle,
                request,
                visible,
                goal,
                labels,
                graph,
                status,
                created_at,
                updated_at,
            },
        ))
    }

    /// Mark one graph terminal (or abandoned) after a successful resume/replan.
    pub async fn mark_status(pool: &SqlitePool, id: &str, status: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE office_graphs SET status = ?, updated_at = ? WHERE id = ?")
            .bind(status)
            .bind(Utc::now().to_rfc3339())
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// A fresh turn supersedes any graph that was left running by a failed
    /// resume or a crash after the office was already restarted.
    pub async fn abandon_running(pool: &SqlitePool, chatroom_id: Uuid) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE office_graphs SET status = 'abandoned', updated_at = ?
             WHERE chatroom_id = ? AND status = 'running'",
        )
        .bind(Utc::now().to_rfc3339())
        .bind(chatroom_id.to_string())
        .execute(pool)
        .await?;
        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod office_graph_tests {
    use super::*;
    use crate::migrations;

    async fn db() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        migrations::run(&pool).await.expect("migrations");
        pool
    }

    #[tokio::test]
    async fn a_running_graph_is_durable_and_stops_being_resumable_when_terminal() {
        let pool = db().await;
        let room = ravenbot_core::ChatRoom::new("ARGUS", "dashboard", "custom");
        ChatRoomQueries::create(&pool, &room).await.expect("room");
        let bot = ravenbot_core::Bot::new("CEO", "lead");
        BotQueries::insert(&pool, &bot).await.expect("bot");
        let thread = ravenbot_core::Thread::new(bot.id, "Office");
        ThreadQueries::create(&pool, &thread).await.expect("thread");

        let id = Uuid::new_v4();
        OfficeGraphQueries::save(
            &pool,
            id,
            room.id,
            thread.id,
            0,
            "do next",
            "do next",
            "Build the dashboard",
            r#"{"node":"CEO"}"#,
            r#"{"id":"graph"}"#,
            "running",
        )
        .await
        .expect("save graph");

        let loaded = OfficeGraphQueries::latest_running(&pool, room.id)
            .await
            .expect("latest")
            .expect("running graph");
        assert_eq!(loaded.id, id.to_string());
        assert_eq!(loaded.cycle, 0);
        assert_eq!(loaded.request, "do next");
        assert_eq!(loaded.goal, "Build the dashboard");
        assert_eq!(loaded.labels, r#"{"node":"CEO"}"#);
        let by_id = OfficeGraphQueries::get(&pool, id)
            .await
            .expect("get graph")
            .expect("saved graph");
        assert_eq!(by_id.graph, r#"{"id":"graph"}"#);
        assert_eq!(by_id.status, "running");

        OfficeGraphQueries::mark_status(&pool, &id.to_string(), "completed")
            .await
            .expect("complete");
        assert!(
            OfficeGraphQueries::latest_running(&pool, room.id)
                .await
                .expect("latest after completion")
                .is_none(),
            "a completed graph must not be resumed"
        );

        // A fresh turn abandons anything left running by a failed resume.
        OfficeGraphQueries::save(
            &pool,
            Uuid::new_v4(),
            room.id,
            thread.id,
            1,
            "continue",
            "continue",
            "Build the dashboard",
            "{}",
            "{}",
            "running",
        )
        .await
        .expect("second graph");
        assert_eq!(
            OfficeGraphQueries::abandon_running(&pool, room.id)
                .await
                .expect("abandon"),
            1
        );
        assert!(OfficeGraphQueries::latest_running(&pool, room.id)
            .await
            .expect("latest after abandon")
            .is_none());
    }
}

#[cfg(test)]
mod mcp_event_trigger_tests {
    use super::*;
    use crate::migrations;
    use chrono::Duration;

    async fn db() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        migrations::run(&pool).await.expect("migrations");
        pool
    }

    async fn seeded() -> (SqlitePool, Uuid, Uuid) {
        let pool = db().await;
        let bot = ravenbot_core::Bot::new("Listener", "MCP event test");
        BotQueries::insert(&pool, &bot).await.expect("bot");
        let routine_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO routines (id, bot_id, name, schedule, instruction, is_enabled)
             VALUES (?, ?, 'Nightly', '0 9 * * *', 'do the thing', 1)",
        )
        .bind(routine_id.to_string())
        .bind(bot.id.to_string())
        .execute(&pool)
        .await
        .expect("routine");
        (pool, bot.id, routine_id)
    }

    #[tokio::test]
    async fn a_trigger_round_trips_and_is_scoped_to_its_bot() {
        let (pool, bot_id, routine_id) = seeded().await;
        let trigger = McpEventTriggerQueries::create(
            &pool,
            routine_id,
            " github ",
            "notifications/message",
            "",
            "deploy",
            120,
        )
        .await
        .expect("create");
        assert_eq!(trigger.server_id, "github");
        assert_eq!(trigger.cooldown_secs, 120);
        assert!(trigger.enabled);

        let for_bot = McpEventTriggerQueries::list_for_bot(&pool, bot_id)
            .await
            .expect("list");
        assert_eq!(for_bot.len(), 1);
        assert_eq!(for_bot[0].id, trigger.id);
        assert!(McpEventTriggerQueries::list_for_bot(&pool, Uuid::new_v4())
            .await
            .expect("other bot")
            .is_empty());
    }

    #[tokio::test]
    async fn a_firing_slot_is_claimed_once_per_cooldown() {
        let (pool, _, routine_id) = seeded().await;
        let trigger = McpEventTriggerQueries::create(&pool, routine_id, "github", "*", "", "", 60)
            .await
            .expect("create");
        let now = Utc::now();
        let now_text = now.to_rfc3339();
        let cutoff = (now - Duration::seconds(60)).to_rfc3339();
        assert!(
            McpEventTriggerQueries::claim(&pool, trigger.id, &now_text, &cutoff)
                .await
                .expect("first claim")
        );
        assert!(
            !McpEventTriggerQueries::claim(&pool, trigger.id, &now_text, &cutoff)
                .await
                .expect("second claim")
        );

        let later = (now + Duration::seconds(61)).to_rfc3339();
        let later_cutoff = (now + Duration::seconds(1)).to_rfc3339();
        assert!(
            McpEventTriggerQueries::claim(&pool, trigger.id, &later, &later_cutoff)
                .await
                .expect("after cooldown")
        );
    }

    #[tokio::test]
    async fn disabling_the_routine_or_trigger_stops_delivery() {
        let (pool, _, routine_id) = seeded().await;
        let trigger = McpEventTriggerQueries::create(&pool, routine_id, "github", "*", "", "", 0)
            .await
            .expect("create");
        assert_eq!(
            McpEventTriggerQueries::list_enabled(&pool)
                .await
                .expect("enabled")
                .len(),
            1
        );

        McpEventTriggerQueries::set_enabled(&pool, trigger.id, false)
            .await
            .expect("disable trigger");
        assert!(McpEventTriggerQueries::list_enabled(&pool)
            .await
            .expect("disabled trigger")
            .is_empty());

        McpEventTriggerQueries::set_enabled(&pool, trigger.id, true)
            .await
            .expect("enable trigger");
        sqlx::query("UPDATE routines SET is_enabled = 0 WHERE id = ?")
            .bind(routine_id.to_string())
            .execute(&pool)
            .await
            .expect("disable routine");
        assert!(McpEventTriggerQueries::list_enabled(&pool)
            .await
            .expect("disabled routine")
            .is_empty());

        McpEventTriggerQueries::delete_for_routine(&pool, routine_id)
            .await
            .expect("cleanup");
        assert!(McpEventTriggerQueries::get(&pool, trigger.id)
            .await
            .expect("get")
            .is_none());
    }

    #[tokio::test]
    async fn a_duplicate_trigger_is_rejected_and_errors_are_bounded() {
        let (pool, _, routine_id) = seeded().await;
        let trigger = McpEventTriggerQueries::create(
            &pool,
            routine_id,
            "github",
            "notifications/message",
            "file:///repo",
            "push",
            60,
        )
        .await
        .expect("create");
        assert!(
            McpEventTriggerQueries::create(
                &pool,
                routine_id,
                "github",
                "notifications/message",
                "file:///repo",
                "push",
                60,
            )
            .await
            .is_err(),
            "the same server/event/resource/filter must not create a second row"
        );

        McpEventTriggerQueries::record_error(&pool, trigger.id, &"x".repeat(2_000))
            .await
            .expect("record error");
        let stored = McpEventTriggerQueries::get(&pool, trigger.id)
            .await
            .expect("get")
            .expect("trigger");
        assert_eq!(stored.last_error.len(), 1_000);
    }
}

#[cfg(test)]
mod thread_share_tests {
    use super::*;
    use crate::migrations;
    use chrono::Duration;

    async fn db() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        migrations::run(&pool).await.expect("migrations");
        pool
    }

    #[tokio::test]
    async fn a_share_resolves_only_while_active_and_records_views() {
        let pool = db().await;
        let thread_id = Uuid::new_v4();
        let expiry = (Utc::now() + Duration::hours(1)).to_rfc3339();
        let share = ThreadShareQueries::create(&pool, thread_id, "hash-one", &expiry)
            .await
            .expect("create");
        assert_eq!(
            ThreadShareQueries::for_token(&pool, "hash-one")
                .await
                .expect("resolve")
                .expect("active")
                .id,
            share.id
        );
        assert!(ThreadShareQueries::for_token(&pool, "unknown")
            .await
            .expect("unknown")
            .is_none());

        ThreadShareQueries::record_view(&pool, share.id)
            .await
            .expect("view");
        let listed = ThreadShareQueries::list_for_thread(&pool, thread_id)
            .await
            .expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].view_count, 1);
        assert!(listed[0].last_viewed_at.is_some());

        ThreadShareQueries::revoke(&pool, share.id)
            .await
            .expect("revoke");
        assert!(ThreadShareQueries::for_token(&pool, "hash-one")
            .await
            .expect("revoked")
            .is_none());
        let listed = ThreadShareQueries::list_for_thread(&pool, thread_id)
            .await
            .expect("list after revoke");
        assert!(listed[0].revoked_at.is_some());
    }

    #[tokio::test]
    async fn an_expired_share_is_indistinguishable_from_an_unknown_token() {
        let pool = db().await;
        let thread_id = Uuid::new_v4();
        let expiry = (Utc::now() - Duration::minutes(1)).to_rfc3339();
        ThreadShareQueries::create(&pool, thread_id, "expired", &expiry)
            .await
            .expect("create");
        assert!(ThreadShareQueries::for_token(&pool, "expired")
            .await
            .expect("expired")
            .is_none());
    }

    #[tokio::test]
    async fn one_token_hash_creates_one_share_and_threads_are_scoped() {
        let pool = db().await;
        let first_thread = Uuid::new_v4();
        let second_thread = Uuid::new_v4();
        let expiry = (Utc::now() + Duration::hours(1)).to_rfc3339();
        ThreadShareQueries::create(&pool, first_thread, "same-token", &expiry)
            .await
            .expect("first");
        assert!(
            ThreadShareQueries::create(&pool, second_thread, "same-token", &expiry)
                .await
                .is_err(),
            "a token must resolve to exactly one share"
        );
        assert_eq!(
            ThreadShareQueries::list_for_thread(&pool, first_thread)
                .await
                .expect("first list")
                .len(),
            1
        );
        assert!(ThreadShareQueries::list_for_thread(&pool, second_thread)
            .await
            .expect("second list")
            .is_empty());

        ThreadShareQueries::delete_for_thread(&pool, first_thread)
            .await
            .expect("delete");
        assert!(ThreadShareQueries::list_for_thread(&pool, first_thread)
            .await
            .expect("after delete")
            .is_empty());
    }
}

#[cfg(test)]
mod email_draft_tests {
    use super::*;
    use crate::migrations;

    async fn db() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        migrations::run(&pool).await.expect("migrations");
        pool
    }

    async fn create(pool: &SqlitePool, subject: &str) -> EmailDraft {
        EmailDraftQueries::create(
            pool,
            &NewEmailDraft {
                bot_id: "bot-1",
                thread_id: Some("thread-1"),
                run_id: Some("run-1"),
                to_addresses: &["person@example.com".to_string()],
                cc_addresses: &["cc@example.com".to_string()],
                subject,
                body: "Full body that lists must not return.",
            },
        )
        .await
        .expect("create")
    }

    #[tokio::test]
    async fn create_round_trips_addresses_and_context() {
        let pool = db().await;
        let draft = create(&pool, "Hello").await;
        assert_eq!(draft.status, "draft");
        assert_eq!(draft.bot_id, "bot-1");
        assert_eq!(draft.thread_id.as_deref(), Some("thread-1"));
        assert_eq!(draft.run_id.as_deref(), Some("run-1"));
        assert_eq!(draft.to_addresses, vec!["person@example.com"]);
        assert_eq!(draft.cc_addresses, vec!["cc@example.com"]);
        assert_eq!(draft.body, "Full body that lists must not return.");
        assert!(draft.sent_at.is_none());
        assert!(draft.error.is_none());

        let loaded = EmailDraftQueries::get(&pool, &draft.id)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(loaded.subject, "Hello");
        assert_eq!(loaded.body, draft.body);
    }

    #[tokio::test]
    async fn summaries_are_newest_first_and_never_include_the_body() {
        let pool = db().await;
        create(&pool, "older").await;
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        create(&pool, "newer").await;

        let summaries = EmailDraftQueries::list(&pool, 100).await.expect("list");
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].subject, "newer");
        assert_eq!(summaries[1].subject, "older");
        let encoded = serde_json::to_string(&summaries).expect("serialize");
        assert!(
            !encoded.contains("Full body that lists must not return."),
            "a list response leaked a body: {encoded}"
        );
    }

    #[tokio::test]
    async fn only_a_draft_can_be_edited_or_transitioned() {
        let pool = db().await;
        let sent = create(&pool, "send me").await;
        assert!(
            !EmailDraftQueries::mark_sent(&pool, &sent.id)
                .await
                .expect("mark sent before claim"),
            "mark_sent must require a sending row"
        );
        assert!(EmailDraftQueries::begin_send(&pool, &sent.id)
            .await
            .expect("begin"));
        assert!(EmailDraftQueries::mark_sent(&pool, &sent.id)
            .await
            .expect("sent"));
        assert!(!EmailDraftQueries::mark_sent(&pool, &sent.id)
            .await
            .expect("second send"));
        assert!(!EmailDraftQueries::begin_send(&pool, &sent.id)
            .await
            .expect("claim sent"));
        assert!(!EmailDraftQueries::mark_failed(&pool, &sent.id, "boom")
            .await
            .expect("fail sent"));
        assert!(!EmailDraftQueries::mark_discarded(&pool, &sent.id)
            .await
            .expect("discard sent"));
        assert!(!EmailDraftQueries::update_content(
            &pool,
            &sent.id,
            &["other@example.com".to_string()],
            &[],
            "changed",
            "changed body",
        )
        .await
        .expect("edit sent"));

        let loaded = EmailDraftQueries::get(&pool, &sent.id)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(loaded.status, "sent");
        assert_eq!(loaded.subject, "send me");
        assert!(loaded.sent_at.is_some());
        assert!(loaded.error.is_none());

        let discarded = create(&pool, "throw away").await;
        assert!(EmailDraftQueries::mark_discarded(&pool, &discarded.id)
            .await
            .expect("discard"));
        assert!(!EmailDraftQueries::begin_send(&pool, &discarded.id)
            .await
            .expect("claim discarded"));
        assert!(!EmailDraftQueries::mark_sent(&pool, &discarded.id)
            .await
            .expect("send discarded"));
        assert!(
            !EmailDraftQueries::mark_failed(&pool, &discarded.id, "boom")
                .await
                .expect("fail discarded")
        );
        assert!(!EmailDraftQueries::mark_discarded(&pool, &discarded.id)
            .await
            .expect("discard twice"));
        assert!(!EmailDraftQueries::update_content(
            &pool,
            &discarded.id,
            &["other@example.com".to_string()],
            &[],
            "changed",
            "changed body",
        )
        .await
        .expect("edit discarded"));
        let loaded = EmailDraftQueries::get(&pool, &discarded.id)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(loaded.status, "discarded");
        assert!(loaded.sent_at.is_none());
    }

    #[tokio::test]
    async fn a_sending_draft_is_not_editable_discardable_or_re_claimable() {
        let pool = db().await;
        let draft = create(&pool, "in flight").await;
        assert!(EmailDraftQueries::begin_send(&pool, &draft.id)
            .await
            .expect("begin"));

        let loaded = EmailDraftQueries::get(&pool, &draft.id)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(loaded.status, "sending");
        assert!(!EmailDraftQueries::begin_send(&pool, &draft.id)
            .await
            .expect("second claim"));
        assert!(!EmailDraftQueries::mark_discarded(&pool, &draft.id)
            .await
            .expect("discard sending"));
        assert!(!EmailDraftQueries::update_content(
            &pool,
            &draft.id,
            &["other@example.com".to_string()],
            &[],
            "changed",
            "changed body",
        )
        .await
        .expect("edit sending"));
    }

    #[tokio::test]
    async fn two_concurrent_claims_only_one_wins() {
        let pool = db().await;
        let draft = create(&pool, "only once").await;

        let (first, second) = tokio::join!(
            EmailDraftQueries::begin_send(&pool, &draft.id),
            EmailDraftQueries::begin_send(&pool, &draft.id),
        );
        let wins = u8::from(first.expect("first claim")) + u8::from(second.expect("second claim"));
        assert_eq!(wins, 1, "exactly one sender may win the draft");
        let loaded = EmailDraftQueries::get(&pool, &draft.id)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(loaded.status, "sending");
    }

    #[tokio::test]
    async fn a_draft_can_be_edited_and_a_failed_send_stays_retryable() {
        let pool = db().await;
        let draft = create(&pool, "before").await;
        assert!(EmailDraftQueries::update_content(
            &pool,
            &draft.id,
            &["new@example.com".to_string()],
            &[],
            "after",
            "new body",
        )
        .await
        .expect("update"));

        let loaded = EmailDraftQueries::get(&pool, &draft.id)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(loaded.status, "draft");
        assert_eq!(loaded.subject, "after");
        assert_eq!(loaded.to_addresses, vec!["new@example.com"]);
        assert!(loaded.cc_addresses.is_empty());

        assert!(EmailDraftQueries::begin_send(&pool, &draft.id)
            .await
            .expect("begin"));
        assert!(
            EmailDraftQueries::mark_failed(&pool, &draft.id, "SMTP rejected the message")
                .await
                .expect("fail")
        );
        let loaded = EmailDraftQueries::get(&pool, &draft.id)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(loaded.status, "draft", "a failed send must stay a draft");
        assert_eq!(loaded.error.as_deref(), Some("SMTP rejected the message"));
        assert!(EmailDraftQueries::begin_send(&pool, &draft.id)
            .await
            .expect("retry claim"));

        assert!(EmailDraftQueries::mark_sent(&pool, &draft.id)
            .await
            .expect("sent"));
        let loaded = EmailDraftQueries::get(&pool, &draft.id)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(loaded.status, "sent");
        assert!(loaded.error.is_none());
    }

    async fn set_updated_at(pool: &SqlitePool, id: &str, seconds_ago: i64) {
        let timestamp = (Utc::now() - chrono::Duration::seconds(seconds_ago)).to_rfc3339();
        sqlx::query("UPDATE email_drafts SET updated_at = ? WHERE id = ?")
            .bind(timestamp)
            .bind(id)
            .execute(pool)
            .await
            .expect("set updated_at");
    }

    #[tokio::test]
    async fn interrupted_sends_are_recovered_only_after_the_threshold() {
        let pool = db().await;
        let old = create(&pool, "old claim").await;
        let recent = create(&pool, "recent claim").await;
        let finished = create(&pool, "finished").await;
        assert!(EmailDraftQueries::begin_send(&pool, &old.id)
            .await
            .expect("old begin"));
        assert!(EmailDraftQueries::begin_send(&pool, &recent.id)
            .await
            .expect("recent begin"));
        assert!(EmailDraftQueries::begin_send(&pool, &finished.id)
            .await
            .expect("finished begin"));
        assert!(EmailDraftQueries::mark_sent(&pool, &finished.id)
            .await
            .expect("finished sent"));
        set_updated_at(&pool, &old.id, 3600).await;
        set_updated_at(&pool, &finished.id, 3600).await;

        assert_eq!(
            EmailDraftQueries::recover_interrupted_sends(&pool, 600)
                .await
                .expect("recover"),
            1
        );
        let recovered = EmailDraftQueries::get(&pool, &old.id)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(recovered.status, "draft");
        assert_eq!(recovered.error.as_deref(), Some(SEND_INTERRUPTED_ERROR));
        assert!(recovered.sent_at.is_none());

        let still_sending = EmailDraftQueries::get(&pool, &recent.id)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(still_sending.status, "sending");
        assert!(still_sending.error.is_none());

        let untouched_sent = EmailDraftQueries::get(&pool, &finished.id)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(untouched_sent.status, "sent", "sent is terminal");

        // A second pass at the same threshold finds nothing.
        assert_eq!(
            EmailDraftQueries::recover_interrupted_sends(&pool, 600)
                .await
                .expect("second recover"),
            0
        );

        // Once the recent claim is old enough it is recovered too.
        set_updated_at(&pool, &recent.id, 3600).await;
        assert_eq!(
            EmailDraftQueries::recover_interrupted_sends(&pool, 600)
                .await
                .expect("late recover"),
            1
        );
        let recovered = EmailDraftQueries::get(&pool, &recent.id)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(recovered.status, "draft");
        assert_eq!(recovered.error.as_deref(), Some(SEND_INTERRUPTED_ERROR));
    }
}

#[cfg(test)]
mod commerce_query_tests {
    use super::*;

    async fn db() -> sqlx::SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        crate::migrations::run(&pool).await.expect("migrations");
        pool
    }

    fn request(title: &str, amount: i64) -> NewSpendRequest<'_> {
        NewSpendRequest {
            bot_id: "bot-1",
            thread_id: Some("thread-1"),
            run_id: Some("run-1"),
            title,
            merchant: "Example Store",
            merchant_url: "https://shop.example.com/checkout",
            amount_minor: amount,
            currency: "USD",
            reason: "A test purchase.",
        }
    }

    #[tokio::test]
    async fn creates_lists_and_reads_requests() {
        let pool = db().await;
        let first = SpendRequestQueries::create(&pool, &request("Drive", 1_200))
            .await
            .expect("create");
        SpendRequestQueries::create(&pool, &request("Cable", 300))
            .await
            .expect("create");
        assert_eq!(first.status, "pending");
        assert_eq!(first.amount_minor, 1_200);
        assert!(first.decided_at.is_none());

        let loaded = SpendRequestQueries::get(&pool, &first.id)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(loaded.title, "Drive");
        assert_eq!(loaded.merchant_url, "https://shop.example.com/checkout");

        let all = SpendRequestQueries::list(&pool, 100).await.expect("list");
        assert_eq!(all.len(), 2);
        assert!(all.iter().any(|row| row.id == first.id));
    }

    #[tokio::test]
    async fn approval_is_pending_only_and_records_the_actor() {
        let pool = db().await;
        let row = SpendRequestQueries::create(&pool, &request("Drive", 1_200))
            .await
            .expect("create");

        let outcome = SpendRequestQueries::approve(&pool, &row.id, "owner", 10_000)
            .await
            .expect("approve");
        let ApproveOutcome::Approved(approved) = outcome else {
            panic!("expected approval");
        };
        assert_eq!(approved.status, "approved");
        assert_eq!(approved.decided_by.as_deref(), Some("owner"));
        assert!(approved.decided_at.is_some());

        assert!(matches!(
            SpendRequestQueries::approve(&pool, &row.id, "owner", 10_000)
                .await
                .expect("second approve"),
            ApproveOutcome::NotPending
        ));
        assert!(matches!(
            SpendRequestQueries::approve(&pool, "missing", "owner", 10_000)
                .await
                .expect("missing"),
            ApproveOutcome::NotFound
        ));
    }

    #[tokio::test]
    async fn daily_limit_is_enforced_at_approval_and_rolls_back() {
        let pool = db().await;
        let first = SpendRequestQueries::create(&pool, &request("Drive", 3_000))
            .await
            .expect("create");
        let second = SpendRequestQueries::create(&pool, &request("Cable", 3_000))
            .await
            .expect("create");

        assert!(matches!(
            SpendRequestQueries::approve(&pool, &first.id, "owner", 5_000)
                .await
                .expect("first"),
            ApproveOutcome::Approved(_)
        ));
        match SpendRequestQueries::approve(&pool, &second.id, "owner", 5_000)
            .await
            .expect("second")
        {
            ApproveOutcome::DailyLimitExceeded {
                spent_today_minor,
                limit_minor,
            } => {
                assert_eq!(spent_today_minor, 3_000);
                assert_eq!(limit_minor, 5_000);
            }
            other => panic!("expected daily limit, got {other:?}"),
        }
        let still_pending = SpendRequestQueries::get(&pool, &second.id)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(still_pending.status, "pending");
        assert!(still_pending.decided_at.is_none());
    }

    #[tokio::test]
    async fn decisions_are_terminal_and_do_not_skip_states() {
        let pool = db().await;
        let denied = SpendRequestQueries::create(&pool, &request("Denied", 100))
            .await
            .expect("create");
        assert!(
            SpendRequestQueries::deny(&pool, &denied.id, "owner", Some("not needed"))
                .await
                .expect("deny")
        );
        assert!(!SpendRequestQueries::deny(&pool, &denied.id, "owner", None)
            .await
            .expect("second deny"));
        assert!(!SpendRequestQueries::mark_purchased(&pool, &denied.id)
            .await
            .expect("purchase denied"));

        let approved = SpendRequestQueries::create(&pool, &request("Approved", 200))
            .await
            .expect("create");
        SpendRequestQueries::approve(&pool, &approved.id, "owner", 1_000)
            .await
            .expect("approve");
        assert!(SpendRequestQueries::mark_purchased(&pool, &approved.id)
            .await
            .expect("purchase"));
        assert!(!SpendRequestQueries::mark_purchased(&pool, &approved.id)
            .await
            .expect("second purchase"));
        assert!(
            !SpendRequestQueries::cancel(&pool, &approved.id, "owner", None)
                .await
                .expect("cancel purchased")
        );

        let pending = SpendRequestQueries::create(&pool, &request("Pending", 300))
            .await
            .expect("create");
        assert!(
            SpendRequestQueries::cancel(&pool, &pending.id, "owner", Some("changed mind"))
                .await
                .expect("cancel")
        );
        assert!(matches!(
            SpendRequestQueries::approve(&pool, &pending.id, "owner", 1_000)
                .await
                .expect("approve cancelled"),
            ApproveOutcome::NotPending
        ));
    }

    #[tokio::test]
    async fn spent_today_counts_only_approved_and_purchased() {
        let pool = db().await;
        let approved = SpendRequestQueries::create(&pool, &request("Approved", 1_000))
            .await
            .expect("create");
        SpendRequestQueries::approve(&pool, &approved.id, "owner", 10_000)
            .await
            .expect("approve");

        let purchased = SpendRequestQueries::create(&pool, &request("Purchased", 500))
            .await
            .expect("create");
        SpendRequestQueries::approve(&pool, &purchased.id, "owner", 10_000)
            .await
            .expect("approve");
        SpendRequestQueries::mark_purchased(&pool, &purchased.id)
            .await
            .expect("purchase");

        SpendRequestQueries::create(&pool, &request("Pending", 9_999))
            .await
            .expect("create");
        let denied = SpendRequestQueries::create(&pool, &request("Denied", 9_999))
            .await
            .expect("create");
        SpendRequestQueries::deny(&pool, &denied.id, "owner", None)
            .await
            .expect("deny");

        let today = Utc::now().date_naive().to_string();
        assert_eq!(
            SpendRequestQueries::spent_on(&pool, &today)
                .await
                .expect("sum"),
            1_500
        );
        assert_eq!(
            SpendRequestQueries::spent_on(&pool, "2000-01-01")
                .await
                .expect("other day"),
            0
        );
    }
}
