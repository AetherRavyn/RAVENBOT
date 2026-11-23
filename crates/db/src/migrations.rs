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
///
/// Kept in step with the last entry in [`MIGRATIONS`] rather than with anything
/// else, because that is the only thing it can honestly describe: a database
/// whose `_migrations` table tops out at 23 is at 23, whatever an earlier
/// constant still says.
pub const CURRENT_VERSION: i32 = 70;

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
    (
        9,
        include_str!("migrations/009_bot_skills_and_settings.sql"),
    ),
    (10, include_str!("migrations/010_approvals.sql")),
    (11, include_str!("migrations/011_message_senders.sql")),
    (
        12,
        include_str!("migrations/012_todos_skills_openapi_threads.sql"),
    ),
    (13, include_str!("migrations/013_questions.sql")),
    (14, include_str!("migrations/014_webhooks_channels.sql")),
    (
        15,
        include_str!("migrations/015_contacts_and_channel_rules.sql"),
    ),
    (
        16,
        include_str!("migrations/016_office_project_folders.sql"),
    ),
    (
        17,
        include_str!("migrations/017_thread_project_folders.sql"),
    ),
    (18, include_str!("migrations/018_custom_providers.sql")),
    (19, include_str!("migrations/019_mcp_remote_plugins.sql")),
    (20, include_str!("migrations/020_bot_sort_order.sql")),
    (
        21,
        include_str!("migrations/021_chatroom_office_threads.sql"),
    ),
    (22, include_str!("migrations/022_file_changes.sql")),
    (23, include_str!("migrations/023_file_changes_diff.sql")),
    (
        24,
        include_str!("migrations/024_custom_provider_headers.sql"),
    ),
    (25, include_str!("migrations/025_promote_opencode_go.sql")),
    (26, include_str!("migrations/026_office_autonomy.sql")),
    (27, include_str!("migrations/027_office_queue.sql")),
    (28, include_str!("migrations/028_team_channels.sql")),
    (29, include_str!("migrations/029_message_reactions.sql")),
    (30, include_str!("migrations/030_office_queue_reply.sql")),
    (
        31,
        include_str!("migrations/031_retire_placeholder_plugins.sql"),
    ),
    (32, include_str!("migrations/032_plugin_base_url.sql")),
    (
        33,
        include_str!("migrations/033_plugin_credential_hint.sql"),
    ),
    (34, include_str!("migrations/034_operation_body_format.sql")),
    (
        35,
        include_str!("migrations/035_operation_query_params.sql"),
    ),
    (36, include_str!("migrations/036_interaction_blocks.sql")),
    (37, include_str!("migrations/037_office_queue_position.sql")),
    (38, include_str!("migrations/038_webhook_secret_hash.sql")),
    (39, include_str!("migrations/039_vault.sql")),
    (40, include_str!("migrations/040_audit_viewer.sql")),
    (41, include_str!("migrations/041_workflows.sql")),
    (42, include_str!("migrations/042_workflow_history.sql")),
    (43, include_str!("migrations/043_browser_profiles.sql")),
    (44, include_str!("migrations/044_workflow_schedules.sql")),
    (45, include_str!("migrations/045_memory_provenance.sql")),
    (46, include_str!("migrations/046_workflow_webhooks.sql")),
    (
        47,
        include_str!("migrations/047_browser_profile_cookie_policy.sql"),
    ),
    (48, include_str!("migrations/048_budget_period_windows.sql")),
    (49, include_str!("migrations/049_single_budget_per_bot.sql")),
    (50, include_str!("migrations/050_usage_breakdown.sql")),
    (51, include_str!("migrations/051_remote_access.sql")),
    (52, include_str!("migrations/052_telegram.sql")),
    (53, include_str!("migrations/053_slack.sql")),
    (54, include_str!("migrations/054_discord.sql")),
    (55, include_str!("migrations/055_whatsapp.sql")),
    (56, include_str!("migrations/056_teams.sql")),
    (57, include_str!("migrations/057_run_actors.sql")),
    (58, include_str!("migrations/058_message_context.sql")),
    (
        59,
        include_str!("migrations/059_office_queue_recipient.sql"),
    ),
    (
        60,
        include_str!("migrations/060_message_context_breakdown.sql"),
    ),
    (61, include_str!("migrations/061_run_resume_on_start.sql")),
    (62, include_str!("migrations/062_default_yolo_approval.sql")),
    (63, include_str!("migrations/063_office_graphs.sql")),
    (64, include_str!("migrations/064_messages_fts.sql")),
    (65, include_str!("migrations/065_run_progress.sql")),
    (66, include_str!("migrations/066_work_items.sql")),
    (67, include_str!("migrations/067_quality_gates.sql")),
    (68, include_str!("migrations/068_goals.sql")),
    (69, include_str!("migrations/069_context_summaries.sql")),
    (70, include_str!("migrations/070_goal_autopilot.sql")),
];

/// Split a migration into executable statements without breaking trigger
/// bodies. The simple `split(';')` approach cuts a `BEGIN ... END;` trigger in
/// half; this tracks the BEGIN/END depth and only splits at depth zero.
fn split_sql_statements(sql: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut trigger_depth = 0usize;
    for line in sql.lines() {
        let trimmed = line.trim();
        current.push_str(line);
        current.push('\n');
        if trimmed.is_empty() || trimmed.starts_with("--") {
            continue;
        }
        let upper = trimmed.to_ascii_uppercase();
        if upper == "BEGIN" || upper.ends_with(" BEGIN") {
            trigger_depth += 1;
        } else if upper == "END;" || upper == "END" {
            trigger_depth = trigger_depth.saturating_sub(1);
        }
        if trigger_depth == 0 && trimmed.ends_with(';') {
            let statement = current.trim().to_string();
            if !statement.is_empty() {
                statements.push(statement);
            }
            current.clear();
        }
    }
    let rest = current.trim().to_string();
    if !rest.is_empty() {
        statements.push(rest);
    }
    statements
}

/// Run all pending migrations
pub async fn run(pool: &SqlitePool) -> Result<(), super::DbError> {
    // Create migrations table if it doesn't exist
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS _migrations (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )
    .execute(pool)
    .await?;

    // Get current version
    let current_version: i32 =
        sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) FROM _migrations")
            .fetch_one(pool)
            .await
            .unwrap_or(0);

    // Apply pending migrations
    for &(version, sql) in MIGRATIONS {
        if version > current_version {
            tracing::info!("Applying migration v{}", version);

            // Split into statements, keeping trigger bodies together.
            for statement in split_sql_statements(sql) {
                let statement = statement.trim();
                if !statement.is_empty() {
                    // `ALTER TABLE ... ADD COLUMN` is not idempotent in SQLite,
                    // so a migration that adds a column fails on databases that
                    // already have it (e.g. a dev DB where it was added by hand).
                    // Treat "duplicate column name" as success in that case.
                    match sqlx::query(statement).execute(pool).await {
                        Ok(_) => {}
                        Err(e) if e.to_string().contains("duplicate column name") => {}
                        Err(e) => return Err(e.into()),
                    }
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

#[cfg(test)]
mod tests {
    use super::split_sql_statements;

    #[test]
    fn trigger_bodies_are_not_split_into_broken_statements() {
        let sql = r#"
CREATE TABLE t (id TEXT);
CREATE TRIGGER t_ai AFTER INSERT ON t
BEGIN
    INSERT INTO other VALUES (new.id);
    DELETE FROM stale WHERE id = new.id;
END;
INSERT INTO t VALUES ('x');
"#;
        let statements = split_sql_statements(sql);
        assert_eq!(statements.len(), 3, "{statements:#?}");
        assert!(
            statements[1].starts_with("CREATE TRIGGER"),
            "{statements:#?}"
        );
        assert!(
            statements[1].contains("INSERT INTO other"),
            "{statements:#?}"
        );
        assert!(statements[1].ends_with("END;"), "{statements:#?}");
        assert!(statements[2].contains("INSERT INTO t"), "{statements:#?}");
    }

    #[test]
    fn a_case_end_does_not_close_a_trigger_early() {
        let sql = r#"
UPDATE t SET kind = COALESCE(CASE
    WHEN x = 0 THEN ''
    ELSE x
END, '');
CREATE INDEX i ON t(kind);
"#;
        let statements = split_sql_statements(sql);
        assert_eq!(statements.len(), 2, "{statements:#?}");
        assert!(statements[0].contains("END, ''"), "{statements:#?}");
    }
}
