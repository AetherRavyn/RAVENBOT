-- Recurring workflow runs. A schedule follows the latest saved version and is
-- claimed by the desktop process. It is not a separate executor.
CREATE TABLE IF NOT EXISTS workflow_schedules (
    workflow_id TEXT PRIMARY KEY NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
    interval_hours INTEGER NOT NULL DEFAULT 24,
    enabled INTEGER NOT NULL DEFAULT 0,
    next_run_at TEXT,
    last_run_at TEXT,
    last_error TEXT,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_workflow_schedules_due
    ON workflow_schedules(enabled, next_run_at);
