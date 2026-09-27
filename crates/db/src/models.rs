//! Database model types that map to SQL tables

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Bot as stored in the database
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct BotRow {
    pub id: String,
    pub name: String,
    pub description: String,
    pub avatar_color: String,
    pub avatar_url: Option<String>,
    pub avatar_style: Option<String>,
    pub rank: Option<String>,
    pub specialty: Option<String>,
    pub status: String,
    pub config: String, // JSON
    pub permissions: String, // JSON array
    pub is_orchestrator: bool,
    pub delegate_to: String, // JSON array
    /// Per-bot enabled skill ids as JSON (migration 009; may be absent on old DBs)
    pub skills: Option<String>,
    /// Approval mode string (migration 010; may be absent on old DBs)
    pub approval_mode: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_active_at: Option<String>,
    /// Manual sidebar order (migration 020; 0 = never dragged)
    pub sort_order: Option<i64>,
}

impl BotRow {
    /// Convert to domain type
    pub fn to_domain(&self) -> Result<ravenbot_core::Bot, serde_json::Error> {
        let config: ravenbot_core::BotConfig = serde_json::from_str(&self.config)?;
        let permissions: Vec<ravenbot_core::Permission> = serde_json::from_str(&self.permissions)?;
        let delegate_to: Vec<Uuid> = serde_json::from_str(&self.delegate_to)?;
        let status = bot_status_from_db(Some(self.status.as_str()));

        Ok(ravenbot_core::Bot {
            id: Uuid::parse_str(&self.id).unwrap_or_default(),
            name: self.name.clone(),
            description: self.description.clone(),
            avatar_color: self.avatar_color.clone(),
            avatar_url: self.avatar_url.clone(),
            avatar_style: self.avatar_style.clone(),
            rank: self.rank.clone(),
            specialty: self.specialty.clone(),
            status,
            config,
            permissions,
            is_orchestrator: self.is_orchestrator,
            delegate_to,
            skills: self
                .skills
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default(),
            approval_mode: self
                .approval_mode
                .as_deref()
                .map(ravenbot_core::ApprovalMode::parse)
                .unwrap_or_default(),
            created_at: DateTime::parse_from_rfc3339(&self.created_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            updated_at: DateTime::parse_from_rfc3339(&self.updated_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            last_active_at: self.last_active_at.as_ref().and_then(|s| {
                DateTime::parse_from_rfc3339(s)
                    .map(|dt| dt.with_timezone(&Utc))
                    .ok()
            }),
        })
    }

    /// Convert from domain type
    pub fn from_domain(bot: &ravenbot_core::Bot) -> Result<Self, serde_json::Error> {
        Ok(Self {
            id: bot.id.to_string(),
            name: bot.name.clone(),
            description: bot.description.clone(),
            avatar_color: bot.avatar_color.clone(),
            avatar_url: bot.avatar_url.clone(),
            avatar_style: bot.avatar_style.clone(),
            rank: bot.rank.clone(),
            specialty: bot.specialty.clone(),
            status: bot_status_to_db(bot.status.clone()).to_string(),
            config: serde_json::to_string(&bot.config)?,
            permissions: serde_json::to_string(&bot.permissions)?,
            is_orchestrator: bot.is_orchestrator,
            delegate_to: serde_json::to_string(&bot.delegate_to)?,
            skills: Some(serde_json::to_string(&bot.skills)?),
            approval_mode: Some(bot.approval_mode.as_str().to_string()),
            created_at: bot.created_at.to_rfc3339(),
            updated_at: bot.updated_at.to_rfc3339(),
            last_active_at: bot.last_active_at.map(|dt| dt.to_rfc3339()),
            // Manual order is owned by BotQueries::reorder; create/update never touch it.
            sort_order: None,
        })
    }
}

/// Thread as stored in the database
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ThreadRow {
    pub id: String,
    pub bot_id: String,
    pub title: String,
    pub is_active: bool,
    pub ephemeral: bool,
    /// Added in migration 014 (nullable).
    #[sqlx(default)]
    pub channel_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Message as stored in the database
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MessageRow {
    pub id: String,
    pub thread_id: String,
    pub role: String,
    pub content: String, // JSON
    pub attachments: String, // JSON
    /// Which bot spoke (migration 011; None on old DBs / human messages)
    pub sender_bot_id: Option<String>,
    /// Denormalized sender label
    pub sender_name: Option<String>,
    /// Message this replies to (office discussion)
    pub reply_to_id: Option<String>,
    pub created_at: String,
}

/// Run as stored in the database
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RunRow {
    pub id: String,
    pub bot_id: String,
    pub thread_id: String,
    pub parent_run_id: Option<String>,
    pub state: String,
    pub checkpoint: Option<String>, // JSON
    pub outcome: Option<String>, // JSON
    pub tokens_consumed: i64,
    pub cost_estimate: f64,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

/// Audit entry as stored in the database
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AuditRow {
    pub id: String,
    pub bot_id: String,
    pub run_id: Option<String>,
    pub thread_id: Option<String>,
    pub event: String, // JSON
    pub timestamp: String,
}

/// ChatRoom as stored — production: goal, policy, terms, budget
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ChatRoomRow {
    pub id: String,
    pub name: String,
    pub description: String,
    pub office_template: String,
    pub avatar_url: Option<String>,
    pub avatar_style: Option<String>,
    pub goal: Option<String>,
    pub policy: Option<String>,
    pub terms: Option<String>,
    pub budget: Option<f64>,
    pub budget_distribution: Option<String>,
    /// JSON array of project folder paths (migration 016; None on old DBs)
    pub project_folders: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// ChatRoom member
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ChatRoomMemberRow {
    pub chatroom_id: String,
    pub bot_id: String,
    pub rank: String,
    pub specialty: String,
    pub joined_at: String,
}

/// The stored text for a bot status.
///
/// One place, because the mapping was inlined in `to_domain` and again in
/// `from_domain`, and a new variant would have been added to one of them.
pub fn bot_status_to_db(status: ravenbot_core::BotStatus) -> &'static str {
    match status {
        ravenbot_core::BotStatus::Idle => "idle",
        ravenbot_core::BotStatus::Thinking => "thinking",
        ravenbot_core::BotStatus::RunningTool => "running_tool",
        ravenbot_core::BotStatus::WaitingOnUser => "waiting_on_user",
        ravenbot_core::BotStatus::Paused => "paused",
    }
}

/// The bot status for a stored string.
///
/// An unrecognised value reads as `Idle` rather than failing: a status column
/// is a display hint, and a bot that cannot be identified should look
/// available rather than be unusable.
pub fn bot_status_from_db(raw: Option<&str>) -> ravenbot_core::BotStatus {
    use ravenbot_core::BotStatus;
    match raw.unwrap_or_default() {
        "thinking" => BotStatus::Thinking,
        "running_tool" => BotStatus::RunningTool,
        "waiting_on_user" => BotStatus::WaitingOnUser,
        "paused" => BotStatus::Paused,
        _ => BotStatus::Idle,
    }
}

#[cfg(test)]
mod status_tests {
    use super::*;
    use ravenbot_core::BotStatus;

    #[test]
    fn every_status_survives_a_round_trip() {
        for s in [
            BotStatus::Idle,
            BotStatus::Thinking,
            BotStatus::RunningTool,
            BotStatus::WaitingOnUser,
            BotStatus::Paused,
        ] {
            assert_eq!(
                bot_status_from_db(Some(bot_status_to_db(s.clone()))),
                s
            );
        }
    }

    #[test]
    fn an_unknown_or_missing_value_reads_as_idle() {
        // A status column is a display hint. A bot that cannot be identified
        // should look available rather than be unusable.
        assert_eq!(bot_status_from_db(None), BotStatus::Idle);
        assert_eq!(bot_status_from_db(Some("")), BotStatus::Idle);
        assert_eq!(bot_status_from_db(Some("EmulatingTools")), BotStatus::Idle);
    }

    #[test]
    fn the_stored_form_is_the_snake_case_the_frontend_expects() {
        // The frontend switches on these exact strings, and `BotStatus` itself
        // serialises as PascalCase because it has no serde rename.
        assert_eq!(bot_status_to_db(BotStatus::RunningTool), "running_tool");
        assert_eq!(bot_status_to_db(BotStatus::WaitingOnUser), "waiting_on_user");
    }
}
