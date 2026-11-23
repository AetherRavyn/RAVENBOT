-- Workflow version history and per-dispatch run history.
ALTER TABLE workflows ADD COLUMN version INTEGER NOT NULL DEFAULT 1;

CREATE TABLE IF NOT EXISTS workflow_versions (
    id TEXT PRIMARY KEY NOT NULL,
    workflow_id TEXT NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
    version INTEGER NOT NULL,
    name TEXT NOT NULL,
    goal TEXT NOT NULL,
    tasks TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE(workflow_id, version)
);

CREATE INDEX IF NOT EXISTS idx_workflow_versions_workflow
    ON workflow_versions(workflow_id, version DESC);

-- Existing definitions become version 1.
INSERT OR IGNORE INTO workflow_versions (id, workflow_id, version, name, goal, tasks, created_at)
SELECT id || '-v' || version, id, version, name, goal, tasks, created_at FROM workflows;

CREATE TABLE IF NOT EXISTS workflow_runs (
    id TEXT PRIMARY KEY NOT NULL,
    workflow_id TEXT NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
    workflow_version INTEGER NOT NULL,
    thread_id TEXT,
    goal TEXT NOT NULL,
    state TEXT NOT NULL,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    detail TEXT
);

CREATE INDEX IF NOT EXISTS idx_workflow_runs_workflow
    ON workflow_runs(workflow_id, started_at DESC);
