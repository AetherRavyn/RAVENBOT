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
    pub created_at: String,
    pub updated_at: String,
    pub last_active_at: Option<String>,
}

impl BotRow {
    /// Convert to domain type
    pub fn to_domain(&self) -> Result<ravenbot_core::Bot, serde_json::Error> {
        let config: ravenbot_core::BotConfig = serde_json::from_str(&self.config)?;
        let permissions: Vec<ravenbot_core::Permission> = serde_json::from_str(&self.permissions)?;
        let delegate_to: Vec<Uuid> = serde_json::from_str(&self.delegate_to)?;
        let status = match self.status.as_str() {
            "idle" => ravenbot_core::BotStatus::Idle,
            "thinking" => ravenbot_core::BotStatus::Thinking,
            "running_tool" => ravenbot_core::BotStatus::RunningTool,
            "waiting_on_user" => ravenbot_core::BotStatus::WaitingOnUser,
            "paused" => ravenbot_core::BotStatus::Paused,
            _ => ravenbot_core::BotStatus::Idle,
        };

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
            skills: Vec::new(),
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
            status: match bot.status {
                ravenbot_core::BotStatus::Idle => "idle",
                ravenbot_core::BotStatus::Thinking => "thinking",
                ravenbot_core::BotStatus::RunningTool => "running_tool",
                ravenbot_core::BotStatus::WaitingOnUser => "waiting_on_user",
                ravenbot_core::BotStatus::Paused => "paused",
            }.to_string(),
            config: serde_json::to_string(&bot.config)?,
            permissions: serde_json::to_string(&bot.permissions)?,
            is_orchestrator: bot.is_orchestrator,
            delegate_to: serde_json::to_string(&bot.delegate_to)?,
            created_at: bot.created_at.to_rfc3339(),
            updated_at: bot.updated_at.to_rfc3339(),
            last_active_at: bot.last_active_at.map(|dt| dt.to_rfc3339()),
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
