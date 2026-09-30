use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Office template types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OfficeTemplate {
    ItOffice,
    Marketing,
    Sales,
    Design,
    Custom,
}

impl OfficeTemplate {
    pub fn as_str(&self) -> &str {
        match self {
            Self::ItOffice => "it-office",
            Self::Marketing => "marketing",
            Self::Sales => "sales",
            Self::Design => "design",
            Self::Custom => "custom",
        }
    }
    pub fn from_str(s: &str) -> Self {
        match s {
            "it-office" => Self::ItOffice,
            "marketing" => Self::Marketing,
            "sales" => Self::Sales,
            "design" => Self::Design,
            _ => Self::Custom,
        }
    }
}

/// A chatroom — a team of bots working like a real office (production: goal, policy, terms, budget)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRoom {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub office_template: String,
    pub avatar_url: Option<String>,
    pub avatar_style: Option<String>,
    /// Office goal / quarterly objective
    pub goal: Option<String>,
    /// Office policy & standards (markdown)
    pub policy: Option<String>,
    /// Terms and conditions for this office
    pub terms: Option<String>,
    /// Total budget for office
    pub budget: Option<f64>,
    /// Per-agent budget distribution JSON {bot_id: amount}
    pub budget_distribution: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ChatRoom {
    pub fn new(name: impl Into<String>, description: impl Into<String>, office_template: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            description: description.into(),
            office_template: office_template.into(),
            avatar_url: None,
            avatar_style: None,
            goal: None,
            policy: None,
            terms: None,
            budget: None,
            budget_distribution: None,
            created_at: now,
            updated_at: now,
        }
    }
}

/// Membership of a bot in a chatroom with rank/specialty
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRoomMember {
    pub chatroom_id: Uuid,
    pub bot_id: Uuid,
    pub rank: String,
    pub specialty: String,
    pub joined_at: DateTime<Utc>,
}

/// A chatroom with its members expanded
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRoomWithMembers {
    pub room: ChatRoom,
    pub members: Vec<ChatRoomMemberWithBot>,
    pub thread_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRoomMemberWithBot {
    pub member: ChatRoomMember,
    pub bot: crate::bot::Bot,
}

/// Message in a chatroom group thread
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRoomMessage {
    pub id: Uuid,
    pub chatroom_id: Uuid,
    pub thread_id: Uuid,
    pub sender_id: Option<Uuid>, // None = user, Some(bot_id)
    pub sender_name: String,
    pub sender_rank: Option<String>,
    pub content: String,
    pub created_at: DateTime<Utc>,
}
