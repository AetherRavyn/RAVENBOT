-- Reusable office workflow templates: the goal, ordered tasks, and their DAG
-- dependencies. The existing runtime compiles a loaded template back into a
-- TaskGraph, so this table stores definitions, not a second executor.
CREATE TABLE IF NOT EXISTS workflows (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    goal TEXT NOT NULL,
    tasks TEXT NOT NULL,
    office_id TEXT REFERENCES chatrooms(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_workflows_office_updated
    ON workflows(office_id, updated_at DESC);
