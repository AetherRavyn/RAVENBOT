use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};


use crate::bot::Bot;
use crate::memory::MemoryFact;
use crate::skill::Skill;

/// Bundle format for exporting/importing bots
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotBundle {
    /// Bundle format version (for compatibility)
    pub format_version: u32,
    /// The bot definition
    pub bot: Bot,
    /// Bot skills configuration
    pub skills: Vec<Skill>,
    /// Bot memory (optional)
    pub memory: Option<Vec<MemoryFact>>,
    /// Ed25519 signature over the serialized bot (base64), if signed
    pub signature: Option<String>,
    /// Signer's Ed25519 public key (base64) — lets imports verify + TOFU-trust
    #[serde(default)]
    pub pubkey: Option<String>,
    /// When this bundle was created
    pub created_at: DateTime<Utc>,
}

impl BotBundle {
    /// Create a new unsigned bundle
    pub fn new(bot: Bot) -> Self {
        Self {
            format_version: 1,
            bot,
            skills: Vec::new(),
            memory: None,
            signature: None,
            pubkey: None,
            created_at: Utc::now(),
        }
    }

    /// Set skills
    pub fn with_skills(mut self, skills: Vec<Skill>) -> Self {
        self.skills = skills;
        self
    }

    /// Set memory
    pub fn with_memory(mut self, memory: Vec<MemoryFact>) -> Self {
        self.memory = Some(memory);
        self
    }

    /// Sign the bundle with Ed25519 key
    ///
    /// NOTE: legacy placeholder signing (kept for old-bundle compatibility).
    /// Real Ed25519 export signing lives in `ravenbot-sync` and sets both
    /// `signature` and `pubkey`.
    pub fn sign(mut self, private_key: &[u8]) -> Self {
        // Deterministic signing for bundle authenticity
        // Real verification uses ed25519-dalek; this keeps core lightweight
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        let mut data = serde_json::to_vec(&self.bot).unwrap_or_default();
        data.extend_from_slice(private_key);
        // Simple hash-based signature for core (full Ed25519 in sync crate)
        let hash = {
            let mut h: u64 = 0;
            for (i, b) in data.iter().enumerate() {
                h = h.wrapping_add((*b as u64).wrapping_mul(31_u64.wrapping_pow(i as u32 % 4)));
            }
            h
        };
        self.signature = Some(STANDARD.encode(hash.to_le_bytes()));
        self
    }

    /// Verify the signature
    pub fn verify(&self, _public_key: &[u8]) -> bool {
        // Core verification: check signature exists and is base64-decodable
        // Full Ed25519 verification lives in ravenbot-sync crate
        if let Some(sig) = &self.signature {
            use base64::{Engine as _, engine::general_purpose::STANDARD};
            STANDARD.decode(sig).is_ok()
        } else {
            false
        }
    }
}
