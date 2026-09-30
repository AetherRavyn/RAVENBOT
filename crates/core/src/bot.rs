use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use uuid::Uuid;

/// Bot status in the system
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BotStatus {
    /// Bot is idle, waiting for work
    Idle,
    /// Bot is currently thinking (processing a message)
    Thinking,
    /// Bot is executing a tool
    RunningTool,
    /// Bot is waiting for user input
    WaitingOnUser,
    /// Bot has been paused by the user
    Paused,
}

/// Sandbox tier for bot execution isolation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SandboxTier {
    /// OS-level sandbox (default, safest)
    OsLevel,
    /// Docker container isolation
    Docker,
    /// Host execution (explicitly opt-in, off by default)
    Host,
}

/// Permission that a bot skill can request
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Permission {
    /// Access to the file system
    FileSystem { paths: Vec<String> },
    /// Network access to specific domains
    Network { domains: Vec<String> },
    /// Shell command execution
    Shell,
    /// Screenshot/screen capture for vision
    Screenshot,
    /// Mouse/keyboard control
    InputControl,
    /// Audio capture
    AudioCapture,
    /// Audio playback (TTS)
    AudioPlayback,
    /// Clipboard access
    Clipboard,
    /// Inter-bot delegation
    Delegation,
}

/// Configuration for a bot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotConfig {
    /// The model provider to use
    pub model_provider: String,
    /// The specific model identifier
    pub model_id: String,
    /// Fallback provider (optional)
    pub fallback_provider: Option<String>,
    /// Fallback model (optional)
    pub fallback_model: Option<String>,
    /// Sandbox tier
    pub sandbox_tier: SandboxTier,
    /// Maximum tokens per response
    pub max_tokens: Option<u32>,
    /// Temperature setting
    pub temperature: Option<f32>,
    /// Custom system prompt (if any, overrides default)
    pub custom_prompt: Option<String>,
}

impl Default for BotConfig {
    fn default() -> Self {
        Self {
            model_provider: "openrouter".to_string(),
            model_id: "anthropic/claude-3-5-sonnet".to_string(),
            fallback_provider: None,
            fallback_model: None,
            sandbox_tier: SandboxTier::OsLevel,
            max_tokens: Some(4096),
            temperature: Some(0.7),
            custom_prompt: None,
        }
    }
}

/// A bot - a persistent AI agent with identity, skills, and memory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bot {
    /// Unique identifier
    pub id: Uuid,
    /// Human-readable name
    pub name: String,
    /// Bot description/purpose
    pub description: String,
    /// Avatar color (hex)
    pub avatar_color: String,
    /// Avatar URL (dicebear or custom) — animated via CSS if dicebear
    pub avatar_url: Option<String>,
    /// DiceBear style (avataaars, bottts, personas, lorelei, adventurer, etc.)
    pub avatar_style: Option<String>,
    /// Rank in office (e.g., CTO, Tech Lead)
    pub rank: Option<String>,
    /// Specialty / role lane
    pub specialty: Option<String>,
    /// Current status
    pub status: BotStatus,
    /// Bot configuration
    pub config: BotConfig,
    /// Permissions granted to this bot
    pub permissions: Vec<Permission>,
    /// Whether this bot can orchestrate other bots
    pub is_orchestrator: bool,
    /// IDs of bots this bot can delegate to
    pub delegate_to: Vec<Uuid>,
    /// Enabled skill IDs (empty = all built-ins enabled)
    #[serde(default)]
    pub skills: Vec<String>,
    /// Created timestamp
    pub created_at: DateTime<Utc>,
    /// Last updated timestamp
    pub updated_at: DateTime<Utc>,
    /// Last activity timestamp
    pub last_active_at: Option<DateTime<Utc>>,
}

impl Bot {
    /// Create a new bot with the given name
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        let now = Utc::now();
        let name_str: String = name.into();
        let style = Self::style_for_name(&name_str);
        let avatar_url = Self::dicebear_url(&name_str, &style);
        Self {
            id: Uuid::new_v4(),
            name: name_str.clone(),
            description: description.into(),
            avatar_color: "#6366f1".to_string(), // Indigo by default
            avatar_url: Some(avatar_url),
            avatar_style: Some(style),
            rank: None,
            specialty: None,
            status: BotStatus::Idle,
            config: BotConfig::default(),
            permissions: Vec::new(),
            is_orchestrator: false,
            delegate_to: Vec::new(),
            skills: Vec::new(),
            created_at: now,
            updated_at: now,
            last_active_at: None,
        }
    }

    /// Create an orchestrator bot (Chief)
    pub fn new_orchestrator(name: impl Into<String>, description: impl Into<String>) -> Self {
        let mut bot = Self::new(name, description);
        bot.is_orchestrator = true;
        bot.avatar_color = "#8b5cf6".to_string(); // Purple for orchestrators
        bot
    }

    /// Pick a dicebear style deterministically from name hash — ensures animated variety if user doesn't choose
    pub fn style_for_name(name: &str) -> String {
        let styles = ["avataaars", "bottts", "personas", "lorelei", "adventurer", "micah", "notionists", "fun-emoji"];
        let hash = name.bytes().fold(0u32, |a, b| a.wrapping_add(b as u32 * 31));
        styles[(hash as usize) % styles.len()].to_string()
    }

    /// Generate dicebear URL for a seed + style with animated-ready params
    pub fn dicebear_url(seed: &str, style: &str) -> String {
        format!(
            "https://api.dicebear.com/9.x/{}/svg?seed={}&backgroundColor=6366f1,8b5cf6,06b6d4,ec4899,f59e0b,10b981&radius=50",
            style,
            urlencoding::encode(seed)
        )
    }

}

mod urlencoding {
    pub fn encode(s: &str) -> String {
        let mut out = String::new();
        for b in s.bytes() {
            if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' { out.push(b as char); }
            else { out.push_str(&format!("%{:02X}", b)); }
        }
        out
    }
}
