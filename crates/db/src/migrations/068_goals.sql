-- Persisted completion contracts and custom verification gates.
--
-- The office already has a mission (`chatrooms.goal`). This is the checkable
-- contract around it: what outcome counts as done, what must not be violated,
-- how many turns/tokens may be spent, and which commands must pass.
--
-- One office has at most one active contract. A thread-scoped goal is allowed
-- (office_id NULL, thread_id set) for future per-conversation goals.
CREATE TABLE IF NOT EXISTS goals (
    id TEXT PRIMARY KEY NOT NULL,
    office_id TEXT,
    thread_id TEXT,
    objective TEXT NOT NULL DEFAULT '',
    contract_json TEXT NOT NULL DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'active',
    turns_used INTEGER NOT NULL DEFAULT 0,
    tokens_used INTEGER NOT NULL DEFAULT 0,
    last_verdict TEXT,
    last_reason TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f+00:00', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f+00:00', 'now')),
    completed_at TEXT
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_goals_office
    ON goals(office_id) WHERE office_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_goals_thread
    ON goals(thread_id, created_at DESC);

CREATE TABLE IF NOT EXISTS goal_gates (
    id TEXT PRIMARY KEY NOT NULL,
    goal_id TEXT NOT NULL,
    command TEXT NOT NULL,
    cwd TEXT,
    timeout_secs INTEGER NOT NULL DEFAULT 600,
    required INTEGER NOT NULL DEFAULT 1,
    position INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f+00:00', 'now')),
    FOREIGN KEY (goal_id) REFERENCES goals(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_goal_gates_goal
    ON goal_gates(goal_id, position ASC);

-- Tie persisted gate runs to the contract that required them. Detected
-- manifest gates keep goal_id NULL when no contract exists.
ALTER TABLE office_gate_runs ADD COLUMN goal_id TEXT;
CREATE INDEX IF NOT EXISTS idx_office_gate_runs_goal
    ON office_gate_runs(goal_id, created_at DESC);
