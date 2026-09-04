//! RAVENBOT database layer
//!
//! This crate handles all SQLite database operations including migrations,
//! queries, and the typed database interface.

pub mod migrations;
pub mod models;
pub mod queries;

use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use std::path::Path;
use thiserror::Error;

/// Database errors
#[derive(Error, Debug)]
pub enum DbError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] sqlx::Error),
    #[error("Migration error: {0}")]
    Migration(String),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// The main database connection
#[derive(Clone)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    /// Create a new database connection and run migrations
    pub async fn new(db_path: impl AsRef<Path>) -> Result<Self, DbError> {
        let db_path = db_path.as_ref().to_string_lossy();
        let url = format!("sqlite:{}?mode=rwc", db_path);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await?;

        // Enable WAL mode for better concurrent access
        sqlx::query("PRAGMA journal_mode=WAL")
            .execute(&pool)
            .await?;

        // Enable foreign keys
        sqlx::query("PRAGMA foreign_keys=ON")
            .execute(&pool)
            .await?;

        let db = Self { pool };
        db.run_migrations().await?;

        Ok(db)
    }

    /// Run all pending migrations
    async fn run_migrations(&self) -> Result<(), DbError> {
        migrations::run(&self.pool).await
    }

    /// Get the underlying pool (for testing)
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}
