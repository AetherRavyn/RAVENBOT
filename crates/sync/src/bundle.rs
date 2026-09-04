//! Bot bundle export/import

use ravenbot_core::{BotBundle, MemoryFact};
use base64::{Engine as _, engine::general_purpose};
use sqlx::SqlitePool;
use uuid::Uuid;

/// Bundle manager for export/import
pub struct BundleManager {
    pool: SqlitePool,
    signer: crate::signing::BundleSigner,
}

impl BundleManager {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            signer: crate::signing::BundleSigner::new(),
        }
    }

    /// Open a bundle manager with a *persisted* Ed25519 signing key.
    ///
    /// The seed lives in `bundle_signing_key` (created on first use), so
    /// exported bundle signatures are stable across app restarts — required
    /// for other machines to TOFU-verify our bundles.
    pub async fn open(pool: SqlitePool) -> Result<Self, String> {
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS bundle_signing_key (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                private_key TEXT NOT NULL,
                created_at TEXT NOT NULL
            )"#,
        )
        .execute(&pool)
        .await
        .map_err(|e| e.to_string())?;

        let row: Option<(String,)> =
            sqlx::query_as("SELECT private_key FROM bundle_signing_key WHERE id = 1")
                .fetch_optional(&pool)
                .await
                .map_err(|e| e.to_string())?;

        let signer = match row {
            Some((seed_b64,)) => {
                let seed_bytes = general_purpose::STANDARD
                    .decode(seed_b64.trim())
                    .map_err(|e| format!("Corrupt bundle signing key: {}", e))?;
                let seed = <[u8; 32]>::try_from(seed_bytes.as_slice())
                    .map_err(|_| "Corrupt bundle signing key length".to_string())?;
                crate::signing::BundleSigner::from_bytes(seed)
            }
            None => {
                use rand::RngCore;
                let mut seed = [0u8; 32];
                rand::rngs::OsRng.fill_bytes(&mut seed);
                let signer = crate::signing::BundleSigner::from_bytes(seed);
                let seed_b64 = general_purpose::STANDARD.encode(signer.private_key_bytes());
                sqlx::query(
                    "INSERT OR REPLACE INTO bundle_signing_key (id, private_key, created_at) VALUES (1, ?, ?)",
                )
                .bind(&seed_b64)
                .bind(chrono::Utc::now().to_rfc3339())
                .execute(&pool)
                .await
                .map_err(|e| e.to_string())?;
                signer
            }
        };

        Ok(Self { pool, signer })
    }

    /// Pool accessor (tests need to insert prerequisite rows)
    pub fn pool_ref(&self) -> &SqlitePool {
        &self.pool
    }

    /// Signer accessor (tests forge/re-sign bundles)
    pub fn signer_ref(&self) -> &crate::signing::BundleSigner {
        &self.signer
    }

    /// Ensure the trusted-keys tables exist (TOFU registry for import verification)
    pub async fn ensure_trusted_keys(&self) -> Result<(), String> {
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS trusted_bundle_pubkeys (
                fingerprint TEXT PRIMARY KEY,
                pubkey TEXT NOT NULL,
                label TEXT,
                first_imported_at TEXT NOT NULL
            )"#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        // Per-bot key binding: a bot imported with one signing key rejects
        // later re-imports signed with a different key (blocks self-consistent
        // impostor re-signing of an existing bot)
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS bot_trusted_fingerprint (
                bot_id TEXT PRIMARY KEY,
                fingerprint TEXT NOT NULL,
                trusted_at TEXT NOT NULL
            )"#,
        )
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
    }

    async fn is_trusted(&self, fingerprint: &str) -> Result<bool, String> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT fingerprint FROM trusted_bundle_pubkeys WHERE fingerprint = ?",
        )
        .bind(fingerprint)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        Ok(row.is_some())
    }

    async fn trust_key(&self, pubkey_b64: &str, label: &str) -> Result<(), String> {
        let fingerprint = crate::signing::BundleSigner::fingerprint(pubkey_b64);
        sqlx::query(
            "INSERT OR IGNORE INTO trusted_bundle_pubkeys (fingerprint, pubkey, label, first_imported_at) VALUES (?, ?, ?, ?)",
        )
        .bind(&fingerprint)
        .bind(pubkey_b64)
        .bind(label)
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
    }

    /// Export a bot as a bundle
    pub async fn export_bot(
        &self,
        bot_id: Uuid,
        include_memory: bool,
    ) -> Result<BotBundle, String> {
        // Get bot
        let bot = ravenbot_db::queries::BotQueries::get(&self.pool, bot_id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Bot not found".to_string())?;

        let mut bundle = BotBundle::new(bot);

        // Get bot skills
        let skills = self.get_bot_skills(bot_id).await?;
        bundle = bundle.with_skills(skills);

        // Optionally include memory
        if include_memory {
            let memory = self.get_bot_memory(bot_id).await?;
            bundle = bundle.with_memory(memory);
        }

        // Real Ed25519 signing: signature over the serialized bot + embedded
        // pubkey so importing machines can verify + TOFU-trust us
        let bot_bytes = serde_json::to_vec(&bundle.bot).unwrap_or_default();
        bundle.signature = Some(self.signer.sign_bundle(&bot_bytes));
        bundle.pubkey = Some(self.signer.public_key_b64());

        tracing::info!(
            bot_id = %bot_id,
            include_memory = include_memory,
            "Bot exported (Ed25519 signed)"
        );

        Ok(bundle)
    }

    /// Import a bot from a bundle with TOFU signature verification.
    ///
    /// - Signed bundle + pubkey: verified against the trusted-keys registry;
    ///   on first import the key is recorded (Trust On First Use), later
    ///   imports must match — a key swap is **rejected**.
    /// - Tampered signatures are rejected outright.
    /// - Unsigned/legacy bundles import with a warning.
    pub async fn import_bot(&self, bundle: &BotBundle) -> Result<Uuid, String> {
        self.ensure_trusted_keys().await?;

        let bot_bytes = serde_json::to_vec(&bundle.bot).unwrap_or_default();
        let (sig, pubkey_b64) = (&bundle.signature, &bundle.pubkey);

        if let (Some(sig), Some(pubkey_b64)) = (sig, pubkey_b64) {
            let fingerprint = crate::signing::BundleSigner::fingerprint(pubkey_b64);
            let bot_id = bundle.bot.id;

            // Per-bot binding: if this bot was imported before with a known
            // signer, only that signer may re-import it
            let recorded: Option<(String,)> = sqlx::query_as(
                "SELECT fingerprint FROM bot_trusted_fingerprint WHERE bot_id = ?",
            )
            .bind(bot_id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| e.to_string())?;

            if let Some((recorded_fp,)) = recorded {
                if recorded_fp != fingerprint {
                    return Err(
                        "Bundle rejected: this bot was previously imported with a different signing key (possible bot hijacking)".to_string(),
                    );
                }
                if !crate::signing::BundleSigner::verify_with(&bot_bytes, sig, pubkey_b64) {
                    return Err(
                        "Bundle rejected: signature does not match the trusted key for this bot (possible tampering)".to_string(),
                    );
                }
                tracing::info!(bot_id = %bot_id, "Bundle verified against this bot's trusted signer");
            } else if self.is_trusted(&fingerprint).await? {
                // Known signer (global TOFU): signature must verify
                if !crate::signing::BundleSigner::verify_with(&bot_bytes, sig, pubkey_b64) {
                    return Err(
                        "Bundle rejected: signature does not match the trusted key for this signer (possible tampering or key swap)".to_string(),
                    );
                }
                tracing::info!(fingerprint = %fingerprint[..16], "Bundle verified against trusted key");
            } else {
                // First import from this signer: verify, then TOFU-trust
                if !crate::signing::BundleSigner::verify_with(&bot_bytes, sig, pubkey_b64) {
                    return Err(
                        "Bundle rejected: invalid Ed25519 signature (possible tampering)".to_string(),
                    );
                }
                let label = bundle.bot.name.clone();
                self.trust_key(pubkey_b64, &label).await?;
                tracing::info!(
                    fingerprint = %fingerprint[..16],
                    label = %label,
                    "New signer trusted (TOFU)"
                );
            }

            // Bind this bot to its signer for future re-imports
            sqlx::query(
                "INSERT OR REPLACE INTO bot_trusted_fingerprint (bot_id, fingerprint, trusted_at) VALUES (?, ?, ?)",
            )
            .bind(bot_id.to_string())
            .bind(&fingerprint)
            .bind(chrono::Utc::now().to_rfc3339())
            .execute(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
        } else if bundle.signature.is_some() {
            // Legacy placeholder signature (no pubkey): verify base64-decodability
            if !bundle.verify(self.signer.public_key().as_slice()) {
                tracing::warn!("Legacy bundle signature check failed; importing with warning");
            }
        } else {
            tracing::warn!("Unsigned bundle; importing without authenticity verification");
        }

        let bot_id = bundle.bot.id;

        // Check if bot already exists
        let existing = ravenbot_db::queries::BotQueries::get(&self.pool, bot_id)
            .await
            .map_err(|e| e.to_string())?;

        if existing.is_some() {
            // Bot exists, update it
            ravenbot_db::queries::BotQueries::update(&self.pool, &bundle.bot)
                .await
                .map_err(|e| e.to_string())?;
        } else {
            // Insert new bot
            ravenbot_db::queries::BotQueries::insert(&self.pool, &bundle.bot)
                .await
                .map_err(|e| e.to_string())?;
        }

        // Import memory if present
        if let Some(memory) = &bundle.memory {
            for fact in memory {
                self.import_memory_fact(bot_id, fact).await?;
            }
        }

        tracing::info!(
            bot_id = %bot_id,
            "Bot imported"
        );

        Ok(bot_id)
    }

    /// Export bot to JSON
    pub async fn export_to_json(&self, bot_id: Uuid, include_memory: bool) -> Result<String, String> {
        let bundle = self.export_bot(bot_id, include_memory).await?;
        serde_json::to_string_pretty(&bundle)
            .map_err(|e| e.to_string())
    }

    /// Export bot to file
    pub async fn export_to_file(&self, bot_id: Uuid, path: &str, include_memory: bool) -> Result<(), String> {
        let json = self.export_to_json(bot_id, include_memory).await?;
        tokio::fs::write(path, json)
            .await
            .map_err(|e| e.to_string())?;
        
        tracing::info!(path = path, "Bot exported to file");
        Ok(())
    }

    /// Import bot from JSON
    pub async fn import_from_json(&self, json: &str) -> Result<Uuid, String> {
        let bundle: BotBundle = serde_json::from_str(json)
            .map_err(|e| format!("Invalid bundle format: {}", e))?;
        
        self.import_bot(&bundle).await
    }

    /// Import bot from file
    pub async fn import_from_file(&self, path: &str) -> Result<Uuid, String> {
        let json = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| format!("Failed to read file: {}", e))?;
        
        self.import_from_json(&json).await
    }

    /// Get skills for a bot
    async fn get_bot_skills(&self, _bot_id: Uuid) -> Result<Vec<ravenbot_core::Skill>, String> {
        // In production, query bot_skills table
        Ok(vec![])
    }

    /// Get memory for a bot
    async fn get_bot_memory(&self, bot_id: Uuid) -> Result<Vec<MemoryFact>, String> {
        let rows: Vec<(String, String, String, Option<Vec<u8>>, f32, i32, String, String)> = sqlx::query_as(
            "SELECT id, bot_id, content, embedding, importance, access_count, last_accessed, created_at
             FROM memory_facts WHERE bot_id = ?"
        )
        .bind(bot_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        let facts = rows.iter()
            .filter_map(|row| {
                Some(MemoryFact {
                    id: Uuid::parse_str(&row.0).ok()?,
                    bot_id: Uuid::parse_str(&row.1).ok()?,
                    content: row.2.clone(),
                    embedding: None, // Don't export embeddings
                    importance: row.4,
                    access_count: row.5 as u32,
                    last_accessed: chrono::DateTime::parse_from_rfc3339(&row.6)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now()),
                    created_at: chrono::DateTime::parse_from_rfc3339(&row.7)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now()),
                })
            })
            .collect();

        Ok(facts)
    }

    /// Import a memory fact
    async fn import_memory_fact(&self, bot_id: Uuid, fact: &MemoryFact) -> Result<(), String> {
        // Check if fact already exists
        let existing: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM memory_facts WHERE bot_id = ? AND content = ?"
        )
        .bind(bot_id.to_string())
        .bind(&fact.content)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        if existing.is_some() {
            return Ok(()); // Already exists
        }

        sqlx::query(
            "INSERT INTO memory_facts (id, bot_id, content, importance, access_count, last_accessed, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(fact.id.to_string())
        .bind(bot_id.to_string())
        .bind(&fact.content)
        .bind(fact.importance)
        .bind(fact.access_count as i32)
        .bind(fact.last_accessed.to_rfc3339())
        .bind(fact.created_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        Ok(())
    }
}

#[cfg(test)]
mod tofu_tests {
    use super::*;
    use ravenbot_core::Bot;
    use std::path::PathBuf;

    async fn temp_manager() -> BundleManager {
        let path = PathBuf::from(std::env::temp_dir())
            .join(format!("ravenbot-sync-test-{}.db", Uuid::new_v4()));
        let db = ravenbot_db::Database::new(&path).await.expect("temp db");
        BundleManager::open(db.pool().clone()).await.expect("manager")
    }

    #[tokio::test]
    async fn roundtrip_export_import_verifies() {
        let manager = temp_manager().await;
        let mut bot = Bot::new("Roundtrip", "sync test bot");
        bot.config.model_provider = "ollama".to_string();
        ravenbot_db::queries::BotQueries::insert(manager.pool_ref(), &bot)
            .await
            .map_err(|e| e.to_string())
            .unwrap();

        let bundle = manager.export_bot(bot.id, false).await.unwrap();
        assert!(bundle.signature.is_some());
        assert!(bundle.pubkey.is_some());

        let imported = manager.import_bot(&bundle).await.unwrap();
        assert_eq!(imported, bot.id);
    }

    #[tokio::test]
    async fn tampered_signature_is_rejected() {
        let manager = temp_manager().await;
        let mut bot = Bot::new("Tampered", "sync test bot");
        bot.config.model_provider = "ollama".to_string();
        ravenbot_db::queries::BotQueries::insert(manager.pool_ref(), &bot)
            .await
            .map_err(|e| e.to_string())
            .unwrap();

        let mut bundle = manager.export_bot(bot.id, false).await.unwrap();
        // Tamper with the bot payload after signing
        bundle.bot.description = "EVIL — tampered payload".to_string();

        let err = manager.import_bot(&bundle).await.unwrap_err();
        assert!(err.contains("rejected"), "got: {err}");
    }

    #[tokio::test]
    async fn tofu_trusts_then_rejects_key_swaps() {
        let manager = temp_manager().await;
        let mut bot = Bot::new("Tofu", "sync test bot");
        bot.config.model_provider = "ollama".to_string();
        ravenbot_db::queries::BotQueries::insert(manager.pool_ref(), &bot)
            .await
            .map_err(|e| e.to_string())
            .unwrap();

        // First import: trusted on first use
        let bundle = manager.export_bot(bot.id, false).await.unwrap();
        manager.import_bot(&bundle).await.unwrap();

        // A different manager (different key) re-signs the bot: key swap
        let impostor = temp_manager().await;
        let mut forged = manager.export_bot(bot.id, false).await.unwrap();
        let bot_bytes = serde_json::to_vec(&forged.bot).unwrap();
        forged.signature = Some(impostor.signer_ref().sign_bundle(&bot_bytes));
        forged.pubkey = Some(impostor.signer_ref().public_key_b64());

        let err = manager.import_bot(&forged).await.unwrap_err();
        // Per-bot key binding: impostor re-signing an existing bot with a
        // different key is rejected
        assert!(err.contains("different signing key"), "got: {err}");
    }

    #[tokio::test]
    async fn unsigned_bundles_import_with_warning() {
        let manager = temp_manager().await;
        let bot = Bot::new("Unsigned", "sync test bot");
        let bundle = BotBundle::new(bot);
        let imported = manager.import_bot(&bundle).await.unwrap();
        assert_ne!(imported, Uuid::nil());
    }
}
