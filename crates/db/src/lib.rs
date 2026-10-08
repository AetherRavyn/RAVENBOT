//! RAVENBOT database layer
//!
//! This crate handles all SQLite database operations including migrations,
//! queries, and the typed database interface.

pub mod migrations;
pub mod models;
pub mod queries;

use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions,
};
use std::path::Path;
use std::time::Duration;
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

        // SQLite keeps `foreign_keys` and `busy_timeout` *per connection*, so
        // issuing them as one-off PRAGMAs against the pool touched exactly one
        // of the five connections and left the other four at their defaults —
        // which is how foreign-key enforcement went missing for most queries
        // while still looking configured. `connect_with` applies these to every
        // connection as it is opened.
        //
        // `busy_timeout` is the other half of the same story: WAL admits only
        // one writer at a time, and without a timeout SQLite answers a
        // conflicting write with SQLITE_BUSY immediately instead of waiting for
        // the lock. That surfaces as "database is locked" on any write that
        // races another — in the app exactly as in the tests.
        let options: SqliteConnectOptions = url.parse()?;
        let options = options
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5));

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Per-connection settings have to reach *every* connection in the pool,
    /// not whichever one happened to execute the PRAGMA.
    ///
    /// `journal_mode`, `foreign_keys` and `busy_timeout` used to be issued as
    /// one-off statements against the pool. That touches exactly one of the
    /// five connections: foreign keys held for a fifth of queries and were
    /// silently off elsewhere, and with no `busy_timeout` a write that raced
    /// another got SQLITE_BUSY immediately instead of waiting for the WAL
    /// writer lock — the "database is locked" failure that took CI down.
    #[tokio::test]
    async fn every_pooled_connection_carries_the_per_connection_settings() {
        let path = std::env::temp_dir().join(format!(
            "ravenbot-db-settings-{}-{}.db",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let db = Database::new(&path).await.expect("temp db");

        // Hold all five at once. The pool is capped at five, so these five
        // acquisitions can only all return if they are five distinct live
        // connections — otherwise the last one would block waiting for a
        // release. Reading each one therefore samples the pool, not one
        // connection five times.
        let mut conns = Vec::new();
        for _ in 0..5 {
            conns.push(db.pool().acquire().await.expect("acquire connection"));
        }

        for conn in conns.iter_mut() {
            // Deref to the connection itself: `&mut PoolConnection` is not an
            // `Executor` here, `&mut SqliteConnection` is.
            let conn = &mut **conn;
            let fk: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
                .fetch_one(&mut *conn)
                .await
                .expect("read foreign_keys");
            let busy: i64 = sqlx::query_scalar("PRAGMA busy_timeout")
                .fetch_one(&mut *conn)
                .await
                .expect("read busy_timeout");
            let journal: String = sqlx::query_scalar("PRAGMA journal_mode")
                .fetch_one(&mut *conn)
                .await
                .expect("read journal_mode");

            assert_eq!(fk, 1, "a pooled connection is not enforcing foreign keys");
            assert_eq!(
                busy, 5000,
                "a pooled connection would answer SQLITE_BUSY instead of waiting for the lock"
            );
            assert_eq!(journal.to_lowercase(), "wal");
        }

        drop(conns);
        drop(db);
        let wal = format!("{}-wal", path.display());
        let shm = format!("{}-shm", path.display());
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(wal);
        let _ = std::fs::remove_file(shm);
    }
}
