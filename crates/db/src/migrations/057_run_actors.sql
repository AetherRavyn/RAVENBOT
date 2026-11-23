-- Attribution for shared workspaces. A run may be started by a different local
-- account than the workspace owner, so the actor is stored separately from the
-- run row and survives runtime changes.

CREATE TABLE IF NOT EXISTS run_actors (
    run_id TEXT PRIMARY KEY NOT NULL,
    actor_id TEXT NOT NULL,
    actor_name TEXT NOT NULL,
    role TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
