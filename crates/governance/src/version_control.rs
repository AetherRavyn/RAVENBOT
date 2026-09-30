//! Prompt version control with diff and rollback

use ravenbot_core::{BotVersion, VersionSource};
use sqlx::SqlitePool;
use uuid::Uuid;

/// Prompt version control system
pub struct PromptVersionControl {
    pool: SqlitePool,
}

impl PromptVersionControl {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Create a new version
    pub async fn create_version(
        &self,
        bot_id: Uuid,
        system_prompt: &str,
        config: serde_json::Value,
        source: VersionSource,
        _description: Option<&str>,
    ) -> Result<BotVersion, String> {
        // Get current version number
        let current_version: (Option<i64>,) = sqlx::query_as(
            "SELECT MAX(version_number) FROM bot_versions WHERE bot_id = ?"
        )
        .bind(bot_id.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        let new_version_number = current_version.0.unwrap_or(0) as u32 + 1;

        let version = BotVersion::new(
            bot_id,
            new_version_number,
            system_prompt,
            config,
            source,
        );

        sqlx::query(
            "INSERT INTO bot_versions (id, bot_id, version_number, system_prompt, config, source, description, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(version.id.to_string())
        .bind(version.bot_id.to_string())
        .bind(version.version_number as i64)
        .bind(&version.system_prompt)
        .bind(serde_json::to_string(&version.config).unwrap_or_default())
        .bind(match &version.source {
            VersionSource::User => "user",
            VersionSource::BotSelf => "bot_self",
        })
        .bind(&version.description)
        .bind(version.created_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        tracing::info!(
            bot_id = %bot_id,
            version = new_version_number,
            "New prompt version created"
        );

        Ok(version)
    }

    /// Get version history for a bot
    pub async fn get_history(&self, bot_id: Uuid, limit: u32) -> Result<Vec<BotVersion>, String> {
        let rows: Vec<(String, String, i64, String, String, String, Option<String>, String)> = sqlx::query_as(
            "SELECT id, bot_id, version_number, system_prompt, config, source, description, created_at
             FROM bot_versions
             WHERE bot_id = ?
             ORDER BY version_number DESC
             LIMIT ?"
        )
        .bind(bot_id.to_string())
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        let versions = rows.iter()
            .filter_map(|row| {
                let source = match row.5.as_str() {
                    "user" => VersionSource::User,
                    "bot_self" => VersionSource::BotSelf,
                    _ => VersionSource::User,
                };

                Some(BotVersion {
                    id: Uuid::parse_str(&row.0).ok()?,
                    bot_id: Uuid::parse_str(&row.1).ok()?,
                    version_number: row.2 as u32,
                    system_prompt: row.3.clone(),
                    config: serde_json::from_str(&row.4).ok()?,
                    source,
                    description: row.6.clone(),
                    created_at: chrono::DateTime::parse_from_rfc3339(&row.7)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now()),
                })
            })
            .collect();

        Ok(versions)
    }

    /// Get a specific version
    pub async fn get_version(&self, version_id: Uuid) -> Result<Option<BotVersion>, String> {
        let row: Option<(String, String, i64, String, String, String, Option<String>, String)> = sqlx::query_as(
            "SELECT id, bot_id, version_number, system_prompt, config, source, description, created_at
             FROM bot_versions WHERE id = ?"
        )
        .bind(version_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        match row {
            Some(row) => {
                let source = match row.5.as_str() {
                    "user" => VersionSource::User,
                    "bot_self" => VersionSource::BotSelf,
                    _ => VersionSource::User,
                };

                let id = Uuid::parse_str(&row.0)
                    .map_err(|e| e.to_string())?;
                let bot_id = Uuid::parse_str(&row.1)
                    .map_err(|e| e.to_string())?;
                let config: serde_json::Value = serde_json::from_str(&row.4)
                    .map_err(|e| e.to_string())?;
                
                Ok(Some(BotVersion {
                    id,
                    bot_id,
                    version_number: row.2 as u32,
                    system_prompt: row.3.clone(),
                    config,
                    source,
                    description: row.6.clone(),
                    created_at: chrono::DateTime::parse_from_rfc3339(&row.7)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now()),
                }))
            }
            None => Ok(None),
        }
    }

    /// Rollback to a specific version
    pub async fn rollback(&self, bot_id: Uuid, version_id: Uuid) -> Result<BotVersion, String> {
        let version = self.get_version(version_id).await?
            .ok_or_else(|| "Version not found".to_string())?;

        if version.bot_id != bot_id {
            return Err("Version does not belong to this bot".to_string());
        }

        // Create a new version based on the old one
        self.create_version(
            bot_id,
            &version.system_prompt,
            version.config.clone(),
            VersionSource::User,
            Some(&format!("Rollback to version {}", version.version_number)),
        ).await
    }

    /// Compare two versions
    pub async fn diff(&self, version_id_a: Uuid, version_id_b: Uuid) -> Result<VersionDiff, String> {
        let version_a = self.get_version(version_id_a).await?
            .ok_or_else(|| "Version A not found".to_string())?;
        let version_b = self.get_version(version_id_b).await?
            .ok_or_else(|| "Version B not found".to_string())?;

        let prompt_changed = version_a.system_prompt != version_b.system_prompt;
        let config_changed = version_a.config != version_b.config;

        // Simple line-by-line diff for prompts
        let prompt_diff = if prompt_changed {
            Some(LineDiff {
                removed: version_a.system_prompt.lines()
                    .filter(|line| !version_b.system_prompt.contains(line))
                    .map(|s| s.to_string())
                    .collect(),
                added: version_b.system_prompt.lines()
                    .filter(|line| !version_a.system_prompt.contains(line))
                    .map(|s| s.to_string())
                    .collect(),
            })
        } else {
            None
        };

        Ok(VersionDiff {
            version_a: VersionSummary {
                id: version_a.id,
                number: version_a.version_number,
                created_at: version_a.created_at,
            },
            version_b: VersionSummary {
                id: version_b.id,
                number: version_b.version_number,
                created_at: version_b.created_at,
            },
            prompt_changed,
            config_changed,
            prompt_diff,
        })
    }
}

/// Diff between two versions
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VersionDiff {
    pub version_a: VersionSummary,
    pub version_b: VersionSummary,
    pub prompt_changed: bool,
    pub config_changed: bool,
    pub prompt_diff: Option<LineDiff>,
}

/// Summary of a version
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VersionSummary {
    pub id: Uuid,
    pub number: u32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Line-level diff
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LineDiff {
    pub removed: Vec<String>,
    pub added: Vec<String>,
}
