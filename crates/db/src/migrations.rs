//! Database migrations
//!
//! All schema changes are versioned and applied sequentially.

use sqlx::SqlitePool;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MigrationError {
    #[error("SQL error: {0}")]
    Sql(#[from] sqlx::Error),
    #[error("Migration version mismatch: expected {expected}, found {found}")]
    VersionMismatch { expected: i32, found: i32 },
}

/// Migration version
pub const CURRENT_VERSION: i32 = 6;

/// SQL for each migration version
const MIGRATIONS: &[(i32, &str)] = &[
    (1, include_str!("migrations/001_initial.sql")),
    (2, include_str!("migrations/002_chatrooms.sql")),
    (3, include_str!("migrations/003_plugins.sql")),
    (4, include_str!("migrations/004_office_production.sql")),
    (5, include_str!("migrations/005_office_memory.sql")),
    (6, include_str!("migrations/006_mcp.sql")),
    (7, include_str!("migrations/007_ephemeral.sql")),
    (8, include_str!("migrations/008_budget_usage.sql")),
];

/// Run all pending migrations
pub async fn run(pool: &SqlitePool) -> Result<(), super::DbError> {
    // Create migrations table if it doesn't exist
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS _migrations (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        )"
    )
    .execute(pool)
    .await?;

    // Get current version
    let current_version: i32 = sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) FROM _migrations")
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    // Apply pending migrations
    for &(version, sql) in MIGRATIONS {
        if version > current_version {
            tracing::info!("Applying migration v{}", version);
            
            // Split by semicolons and execute each statement
            for statement in sql.split(';') {
                let statement = statement.trim();
                if !statement.is_empty() {
                    sqlx::query(statement)
                        .execute(pool)
                        .await?;
                }
            }

            // Record migration
            sqlx::query("INSERT INTO _migrations (version) VALUES (?)")
                .bind(version)
                .execute(pool)
                .await?;
        }
    }

    Ok(())
}
