//! RAVENBOT database layer
//!
//! This crate handles all SQLite database operations including migrations,
//! queries, and the typed database interface.

pub mod migrations;
pub mod models;
pub mod queries;
pub mod secrets;
pub mod vault_import;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};
use std::path::Path;
use std::time::Duration;
use thiserror::Error;

// A crafted database + WAL file can make SQLite before 3.54.0 return
// uninitialized page-cache memory (RUSTSEC-2026-0459). The direct dependency
// both forces that version and keeps the requirement visible here rather than
// in a lockfile nobody reads.
const _: () = assert!(libsqlite3_sys::SQLITE_VERSION_NUMBER >= 3_054_000);

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
        let path = db_path.as_ref().to_path_buf();
        let db_path = path.to_string_lossy();
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

        // The database holds provider keys, MCP credentials, conversations and
        // the bundle signing seed. Keep it owner-only on Unix; the parent
        // directory is also tightened so the -wal/-shm siblings are covered.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(parent) = path.parent() {
                let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
            }
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }

        Ok(db)
    }

    /// Run all pending migrations
    async fn run_migrations(&self) -> Result<(), DbError> {
        migrations::run(&self.pool).await
    }

    /// Wrap an existing pool. The caller is responsible for running
    /// migrations; this is for tests and embedded callers that already did.
    pub fn from_pool(pool: SqlitePool) -> Self {
        Self { pool }
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

    /// A keychain-less host still gets private-at-rest storage: the database
    /// holds markers and the values live in the sealed vault.
    #[tokio::test]
    async fn secrets_use_the_encrypted_vault_when_the_keychain_is_disabled() {
        let path = std::env::temp_dir().join(format!(
            "ravenbot-secret-fallback-{}-{}.db",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let vault_dir = std::env::temp_dir().join(format!(
            "ravenbot-secret-vault-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let db = Database::new(&path).await.expect("temp db");
        std::env::set_var("RAVENBOT_DISABLE_KEYCHAIN", "1");
        std::env::set_var("RAVENBOT_SECRET_VAULT", &vault_dir);

        queries::ProviderKeyQueries::set(db.pool(), "test-provider", "sk-test-value")
            .await
            .unwrap();
        assert_eq!(
            queries::ProviderKeyQueries::get(db.pool(), "test-provider")
                .await
                .unwrap()
                .as_deref(),
            Some("sk-test-value")
        );
        let stored: String =
            sqlx::query_scalar("SELECT api_key FROM provider_keys WHERE provider = ?")
                .bind("test-provider")
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(
            stored,
            secrets::KEYCHAIN_MARKER,
            "the database must only hold a marker"
        );
        assert!(
            !stored.contains("sk-test-value"),
            "the provider key must not be persisted in plaintext"
        );

        queries::CustomProviderQueries::upsert(
            db.pool(),
            "test-provider",
            "Test",
            "openai",
            "https://example.com/v1",
            "test-model",
            true,
            true,
            Some(r#"{"X-Test":"header-secret"}"#),
        )
        .await
        .unwrap();
        let rows = queries::CustomProviderQueries::list(db.pool())
            .await
            .unwrap();
        assert_eq!(
            rows.iter()
                .find(|row| row.0 == "test-provider")
                .and_then(|row| row.7.as_deref()),
            Some(r#"{"X-Test":"header-secret"}"#)
        );
        let stored_headers: String =
            sqlx::query_scalar("SELECT headers_json FROM custom_providers WHERE id = ?")
                .bind("test-provider")
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(stored_headers, secrets::KEYCHAIN_MARKER);

        queries::VaultQueries::upsert(
            db.pool(),
            "vault-1",
            "login",
            "GitHub",
            "ada",
            "https://github.com",
            None,
            r#"{"password":"hunter2"}"#,
        )
        .await
        .unwrap();
        let vault = queries::VaultQueries::list(db.pool()).await.unwrap();
        assert_eq!(vault.len(), 1);
        assert_eq!(vault[0].name, "GitHub");
        assert_eq!(
            queries::VaultQueries::get_secret(db.pool(), "vault-1")
                .await
                .unwrap()
                .as_deref(),
            Some(r#"{"password":"hunter2"}"#)
        );
        let stored_secret: String =
            sqlx::query_scalar("SELECT secret FROM vault_items WHERE id = 'vault-1'")
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(stored_secret, secrets::KEYCHAIN_MARKER);

        std::env::remove_var("RAVENBOT_DISABLE_KEYCHAIN");
        std::env::remove_var("RAVENBOT_SECRET_VAULT");
        drop(db);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
        let _ = std::fs::remove_dir_all(&vault_dir);
    }

    /// The plugin and MCP tables are owned by migrations, not by the runtime.
    ///
    /// `Runtime::new` used to re-create them from a spawned task "just in case",
    /// which raced the caller's first query for the SQLite write lock. On a
    /// current-thread runtime — what every `#[tokio::test]` gives you — that
    /// race is a deadlock rather than a delay: the caller's sqlite busy-wait
    /// blocks the only thread, so the task holding the lock is never polled to
    /// completion and `busy_timeout` expires as "database is locked". It
    /// surfaced as an unrelated-looking flake in a runtime test.
    ///
    /// Asserting the ownership is what stops it coming back: if these tables
    /// ever stop being created here, the runtime would have to start doing DDL
    /// again in order to keep working.
    #[tokio::test]
    async fn migrations_create_the_plugin_and_mcp_tables() {
        let path = std::env::temp_dir().join(format!(
            "ravenbot-db-schema-{}-{}.db",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let db = Database::new(&path).await.expect("temp db");

        for table in ["plugins", "bot_plugins", "mcp_servers", "mcp_bot_servers"] {
            let found: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?",
            )
            .bind(table)
            .fetch_one(db.pool())
            .await
            .expect("query sqlite_master");
            assert_eq!(
                found, 1,
                "{table} must be created by a migration — the runtime doing DDL races the first query"
            );
        }

        drop(db);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }

    /// Migration 040 must make a database created before it searchable: the
    /// externally-tagged event JSON becomes an indexable kind, and legacy
    /// space-separated timestamps become canonical UTC.
    #[tokio::test]
    async fn audit_migration_backfills_kind_and_normalises_timestamps() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory audit db");
        sqlx::query(
            "CREATE TABLE audit_log (
                id TEXT PRIMARY KEY NOT NULL,
                bot_id TEXT NOT NULL,
                run_id TEXT,
                thread_id TEXT,
                event TEXT NOT NULL,
                timestamp TEXT NOT NULL DEFAULT (datetime('now'))
            )",
        )
        .execute(&pool)
        .await
        .expect("legacy audit table");

        let rows = [
            (
                "a",
                r#"{"ToolCall":{"tool_name":"read_file","arguments":{"path":"/tmp/x"}}}"#,
                "2026-01-02 03:04:05",
            ),
            ("b", r#""BotStarted""#, "2026-01-02T03:04:06.123+00:00"),
            (
                "c",
                r#"{"ModelCall":{"provider":"local","model":"m","tokens_in":1,"tokens_out":2,"cost":0.0}}"#,
                "2026-01-02 03:04:07",
            ),
            ("d", "{}", "2026-01-02 03:04:08"),
        ];
        for (id, event, timestamp) in rows {
            sqlx::query(
                "INSERT INTO audit_log (id, bot_id, event, timestamp) VALUES (?, 'bot', ?, ?)",
            )
            .bind(id)
            .bind(event)
            .bind(timestamp)
            .execute(&pool)
            .await
            .expect("legacy audit row");
        }

        for statement in include_str!("migrations/040_audit_viewer.sql").split(';') {
            let statement = statement.trim();
            if !statement.is_empty() {
                sqlx::query(statement)
                    .execute(&pool)
                    .await
                    .unwrap_or_else(|error| panic!("040 statement failed: {error}\n{statement}"));
            }
        }

        let kind: String = sqlx::query_scalar("SELECT event_kind FROM audit_log WHERE id = 'a'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(kind, "ToolCall");
        let kind: String = sqlx::query_scalar("SELECT event_kind FROM audit_log WHERE id = 'b'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(kind, "BotStarted");
        let kind: String = sqlx::query_scalar("SELECT event_kind FROM audit_log WHERE id = 'c'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(kind, "ModelCall");

        let timestamps: Vec<(String, String)> =
            sqlx::query_as("SELECT id, timestamp FROM audit_log ORDER BY timestamp")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(
            timestamps,
            vec![
                ("a".into(), "2026-01-02T03:04:05.000+00:00".into()),
                ("b".into(), "2026-01-02T03:04:06.123+00:00".into()),
                ("c".into(), "2026-01-02T03:04:07.000+00:00".into()),
                ("d".into(), "2026-01-02T03:04:08.000+00:00".into()),
            ]
        );

        let index: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'idx_audit_log_kind_timestamp'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(index, 1);
    }

    #[tokio::test]
    async fn browser_profiles_are_created_renamed_and_deleted() {
        let path = std::env::temp_dir().join(format!(
            "ravenbot-browser-profiles-{}-{}.db",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let db = Database::new(&path).await.expect("temp db");
        queries::BrowserProfileQueries::create(db.pool(), "named-a", "Work")
            .await
            .unwrap();
        queries::BrowserProfileQueries::create(db.pool(), "named-b", "Personal")
            .await
            .unwrap();
        assert!(
            queries::BrowserProfileQueries::create(db.pool(), "named-c", "work")
                .await
                .is_err()
        );
        let profiles = queries::BrowserProfileQueries::list(db.pool())
            .await
            .unwrap();
        assert_eq!(profiles.len(), 2);
        assert_eq!(profiles[0].name, "Personal");
        assert_eq!(profiles[1].name, "Work");
        assert_eq!(profiles[0].cookie_policy, "inherit");
        assert!(
            queries::BrowserProfileQueries::set_cookie_policy(db.pool(), "named-a", "always")
                .await
                .unwrap()
        );
        assert_eq!(
            queries::BrowserProfileQueries::get(db.pool(), "named-a")
                .await
                .unwrap()
                .unwrap()
                .cookie_policy,
            "always"
        );
        assert!(
            queries::BrowserProfileQueries::rename(db.pool(), "named-a", "Office")
                .await
                .unwrap()
        );
        assert_eq!(
            queries::BrowserProfileQueries::get(db.pool(), "named-a")
                .await
                .unwrap()
                .unwrap()
                .name,
            "Office"
        );
        assert!(queries::BrowserProfileQueries::delete(db.pool(), "named-b")
            .await
            .unwrap());
        assert_eq!(
            queries::BrowserProfileQueries::list(db.pool())
                .await
                .unwrap()
                .len(),
            1
        );

        drop(db);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }

    #[tokio::test]
    async fn workflow_webhooks_route_by_hashed_secret() {
        let path = std::env::temp_dir().join(format!(
            "ravenbot-workflow-webhooks-{}-{}.db",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let db = Database::new(&path).await.expect("temp db");
        let workflow_id = uuid::Uuid::new_v4();
        let tasks = serde_json::to_string(&vec![queries::WorkflowTaskRecord {
            bot_id: uuid::Uuid::new_v4().to_string(),
            node_type: "agent".to_string(),
            label: "Audit".to_string(),
            depends_on: vec![],
            condition: None,
            data_from: None,
            retries: 0,
            output_schema: None,
            loop_iterations: 1,
            loop_until: None,
            loop_group: None,
            loop_group_terminal: false,
            loop_group_iterations: 1,
            loop_group_stop: None,
        }])
        .unwrap();
        queries::WorkflowQueries::upsert(
            db.pool(),
            &workflow_id.to_string(),
            "Triggered",
            "Audit the release",
            &tasks,
            None,
        )
        .await
        .unwrap();
        queries::WorkflowWebhookQueries::enable(db.pool(), workflow_id, "workflow-secret")
            .await
            .unwrap();
        assert!(
            queries::WorkflowWebhookQueries::is_enabled(db.pool(), workflow_id)
                .await
                .unwrap()
        );
        assert_eq!(
            queries::WorkflowWebhookQueries::workflow_for_secret(db.pool(), "workflow-secret")
                .await
                .unwrap(),
            Some(workflow_id)
        );
        assert_eq!(
            queries::WorkflowWebhookQueries::workflow_for_secret(db.pool(), "wrong")
                .await
                .unwrap(),
            None
        );
        queries::WorkflowWebhookQueries::disable(db.pool(), workflow_id)
            .await
            .unwrap();
        assert!(
            !queries::WorkflowWebhookQueries::is_enabled(db.pool(), workflow_id)
                .await
                .unwrap()
        );
        assert_eq!(
            queries::WorkflowWebhookQueries::workflow_for_secret(db.pool(), "workflow-secret")
                .await
                .unwrap(),
            None
        );

        drop(db);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }

    #[tokio::test]
    async fn workflows_are_scoped_updated_and_deleted() {
        let path = std::env::temp_dir().join(format!(
            "ravenbot-workflows-{}-{}.db",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let db = Database::new(&path).await.expect("temp db");
        let office = ravenbot_core::ChatRoom::new("Workflow Office", "test", "custom");
        let other = ravenbot_core::ChatRoom::new("Other Office", "test", "custom");
        queries::ChatRoomQueries::create(db.pool(), &office)
            .await
            .unwrap();
        queries::ChatRoomQueries::create(db.pool(), &other)
            .await
            .unwrap();
        let tasks = serde_json::to_string(&vec![queries::WorkflowTaskRecord {
            bot_id: uuid::Uuid::new_v4().to_string(),
            node_type: "agent".to_string(),
            label: "Audit the release".to_string(),
            depends_on: vec![],
            condition: None,
            data_from: None,
            retries: 0,
            output_schema: None,
            loop_iterations: 1,
            loop_until: None,
            loop_group: None,
            loop_group_terminal: false,
            loop_group_iterations: 1,
            loop_group_stop: None,
        }])
        .unwrap();

        queries::WorkflowQueries::upsert(
            db.pool(),
            "wf-1",
            "Release audit",
            "Audit the release",
            &tasks,
            Some(office.id),
        )
        .await
        .unwrap();
        queries::WorkflowQueries::upsert(
            db.pool(),
            "wf-2",
            "Shared check",
            "Shared goal",
            &tasks,
            None,
        )
        .await
        .unwrap();
        queries::WorkflowQueries::upsert(
            db.pool(),
            "wf-3",
            "Other check",
            "Other goal",
            &tasks,
            Some(other.id),
        )
        .await
        .unwrap();

        let scoped = queries::WorkflowQueries::list(db.pool(), Some(office.id))
            .await
            .unwrap();
        let names: Vec<&str> = scoped
            .iter()
            .map(|workflow| workflow.name.as_str())
            .collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"Release audit"));
        assert!(names.contains(&"Shared check"));
        assert_eq!(
            queries::WorkflowQueries::list(db.pool(), None)
                .await
                .unwrap()
                .len(),
            3
        );

        let version = queries::WorkflowQueries::upsert(
            db.pool(),
            "wf-1",
            "Release audit v2",
            "Audit the release again",
            &tasks,
            Some(office.id),
        )
        .await
        .unwrap();
        assert_eq!(version, 2);
        let updated = queries::WorkflowQueries::list(db.pool(), Some(office.id))
            .await
            .unwrap()
            .into_iter()
            .find(|workflow| workflow.id == "wf-1")
            .expect("updated workflow");
        assert_eq!(updated.name, "Release audit v2");
        assert_eq!(updated.version, 2);
        assert_eq!(updated.tasks.len(), 1);

        let versions = queries::WorkflowQueries::list_versions(db.pool(), "wf-1", 20)
            .await
            .unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].version, 2);
        assert_eq!(versions[1].version, 1);
        assert_eq!(versions[1].name, "Release audit");
        assert_eq!(
            queries::WorkflowQueries::get_version(db.pool(), "wf-1", 1)
                .await
                .unwrap()
                .expect("version 1")
                .name,
            "Release audit"
        );

        let run_id = queries::WorkflowQueries::begin_run(
            db.pool(),
            "wf-1",
            2,
            None,
            "Audit the release again",
        )
        .await
        .unwrap();
        assert!(!run_id.is_empty());
        let running = queries::WorkflowQueries::list_runs(db.pool(), "wf-1", 20)
            .await
            .unwrap();
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].state, "running");
        assert_eq!(running[0].workflow_version, 2);
        assert!(queries::WorkflowQueries::finish_run(
            db.pool(),
            &run_id,
            "completed",
            None,
            Some("finished"),
        )
        .await
        .unwrap());
        assert!(
            !queries::WorkflowQueries::finish_run(db.pool(), &run_id, "completed", None, None,)
                .await
                .unwrap()
        );
        let runs = queries::WorkflowQueries::list_runs(db.pool(), "wf-1", 20)
            .await
            .unwrap();
        assert_eq!(runs[0].state, "completed");

        let due = (chrono::Utc::now() - chrono::Duration::seconds(1)).to_rfc3339();
        queries::WorkflowQueries::set_schedule(db.pool(), "wf-1", 24, true, Some(&due))
            .await
            .unwrap();
        let claimed =
            queries::WorkflowQueries::claim_due(db.pool(), &chrono::Utc::now().to_rfc3339())
                .await
                .unwrap()
                .expect("due workflow");
        assert_eq!(claimed.workflow_id, "wf-1");
        assert_eq!(claimed.tasks.len(), 1);
        assert_eq!(claimed.office_id, office.id.to_string());
        let running = queries::WorkflowQueries::list_runs(db.pool(), "wf-1", 20)
            .await
            .unwrap();
        assert_eq!(running[0].id, claimed.run_id);
        assert_eq!(running[0].state, "running");
        assert!(
            queries::WorkflowQueries::claim_due(db.pool(), &chrono::Utc::now().to_rfc3339(),)
                .await
                .unwrap()
                .is_none()
        );
        queries::WorkflowQueries::set_schedule(db.pool(), "wf-1", 24, false, None)
            .await
            .unwrap();

        queries::WorkflowQueries::delete(db.pool(), "wf-1")
            .await
            .unwrap();
        assert_eq!(
            queries::WorkflowQueries::list(db.pool(), Some(office.id))
                .await
                .unwrap()
                .len(),
            1
        );
        assert!(
            queries::WorkflowQueries::list_versions(db.pool(), "wf-1", 20)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(queries::WorkflowQueries::list_runs(db.pool(), "wf-1", 20)
            .await
            .unwrap()
            .is_empty());

        drop(db);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }

    #[tokio::test]
    async fn office_usage_attributes_own_and_delegated_threads() {
        let path = std::env::temp_dir().join(format!(
            "ravenbot-office-usage-{}-{}.db",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let db = Database::new(&path).await.expect("temp db");
        let bot = ravenbot_core::Bot::new("Aster", "office usage test");
        let other = ravenbot_core::Bot::new("Birch", "office usage test");
        queries::BotQueries::insert(db.pool(), &bot).await.unwrap();
        queries::BotQueries::insert(db.pool(), &other)
            .await
            .unwrap();

        let mut office = ravenbot_core::ChatRoom::new("Alpha", "usage office", "custom");
        office.budget = Some(2.0);
        queries::ChatRoomQueries::create(db.pool(), &office)
            .await
            .unwrap();
        let own_thread = ravenbot_core::Thread::new(bot.id, "Alpha conversation");
        let delegated_thread = ravenbot_core::Thread::new(other.id, "Alpha delegated");
        let direct_thread = ravenbot_core::Thread::new(bot.id, "Direct chat");
        for thread in [&own_thread, &delegated_thread, &direct_thread] {
            queries::ThreadQueries::create(db.pool(), thread)
                .await
                .unwrap();
        }
        sqlx::query(
            "INSERT INTO chatroom_threads (chatroom_id, thread_id, created_at) VALUES (?, ?, ?)",
        )
        .bind(office.id.to_string())
        .bind(own_thread.id.to_string())
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(db.pool())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO chatroom_office_threads (chatroom_id, thread_id, created_at) VALUES (?, ?, ?)",
        )
        .bind(office.id.to_string())
        .bind(delegated_thread.id.to_string())
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(db.pool())
        .await
        .unwrap();

        let now = chrono::Utc::now().to_rfc3339();
        let old = "2020-01-01T00:00:00+00:00";
        let runs = [
            (
                "r-own",
                bot.id,
                own_thread.id,
                "completed",
                100,
                1.0,
                now.as_str(),
            ),
            (
                "r-delegated",
                other.id,
                delegated_thread.id,
                "failed",
                50,
                0.5,
                now.as_str(),
            ),
            (
                "r-direct",
                bot.id,
                direct_thread.id,
                "completed",
                30,
                0.3,
                now.as_str(),
            ),
            ("r-old", bot.id, direct_thread.id, "completed", 10, 0.1, old),
        ];
        for (id, bot_id, thread_id, state, tokens, cost, created_at) in runs {
            sqlx::query(
                "INSERT INTO runs (id, bot_id, thread_id, state, tokens_consumed, cost_estimate, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(bot_id.to_string())
            .bind(thread_id.to_string())
            .bind(state)
            .bind(tokens)
            .bind(cost)
            .bind(created_at)
            .bind(created_at)
            .execute(db.pool())
            .await
            .unwrap();
        }

        let rows = queries::UsageQueries::office_summary(db.pool(), 30)
            .await
            .unwrap();
        assert_eq!(rows.len(), 2, "one office plus the direct group: {rows:?}");
        let office_row = rows
            .iter()
            .find(|row| row.1.as_deref() == Some("Alpha"))
            .expect("Alpha row");
        assert_eq!(office_row.2, Some(2.0));
        assert_eq!(office_row.3, 2);
        assert_eq!(office_row.4, 150);
        assert!((office_row.5 - 1.5).abs() < 1e-9);
        assert_eq!(office_row.6, 1);
        assert_eq!(office_row.7, 1);
        let direct_row = rows.iter().find(|row| row.0.is_none()).expect("direct row");
        assert_eq!(direct_row.3, 1);
        assert_eq!(direct_row.4, 30);
        assert!((direct_row.5 - 0.3).abs() < 1e-9);

        let all_rows = queries::UsageQueries::office_summary(db.pool(), 0)
            .await
            .unwrap();
        let direct_all = all_rows
            .iter()
            .find(|row| row.0.is_none())
            .expect("direct row");
        assert_eq!(direct_all.3, 2, "all-time includes the old run");
        assert_eq!(direct_all.4, 40);

        drop(db);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }
}
