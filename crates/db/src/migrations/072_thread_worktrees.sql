-- One opt-in, conversation-scoped Git worktree.
--
-- A row exists only after isolation actually created a checkout. `status`
-- moves active -> applied or discarded; the row is kept so a review surface
-- can explain what happened to a run after the checkout is gone. The original
-- repository path and base commit are recorded so apply can refuse a project
-- that moved or was replaced underneath the worktree.
CREATE TABLE IF NOT EXISTS thread_worktrees (
    thread_id TEXT PRIMARY KEY NOT NULL,
    original_root TEXT NOT NULL,
    worktree_path TEXT NOT NULL,
    agent_path TEXT NOT NULL DEFAULT '',
    base_commit TEXT NOT NULL DEFAULT '',
    last_run_id TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    note TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    finished_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_thread_worktrees_status
    ON thread_worktrees(status, updated_at);
