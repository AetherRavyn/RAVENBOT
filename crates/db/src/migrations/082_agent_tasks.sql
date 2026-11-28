-- Durable background agent tasks and one-shot event waits.
--
-- A task is rowed before it runs so a restart can report what was interrupted.
-- The host task loop claims queued rows under dependency and resource rules,
-- creates the task's run on the already-created thread, and writes the result
-- back here. `agent_waits` is the blocked run's side of the bargain: an event
-- fires a waiting row exactly once, the blocked tool consumes it exactly once,
-- and a deadline expires it instead of hanging.

CREATE TABLE IF NOT EXISTS agent_tasks (
    id TEXT PRIMARY KEY NOT NULL,
    owner_bot_id TEXT NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    target_bot_id TEXT NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    owner_thread_id TEXT,
    owner_run_id TEXT,
    thread_id TEXT,
    run_id TEXT,
    office_id TEXT,
    title TEXT NOT NULL,
    instruction TEXT NOT NULL,
    expected_result TEXT NOT NULL DEFAULT '',
    state TEXT NOT NULL DEFAULT 'queued',
    resources TEXT NOT NULL DEFAULT '[]',
    depends_on TEXT NOT NULL DEFAULT '[]',
    workspace TEXT,
    depth INTEGER NOT NULL DEFAULT 1,
    auto_allow INTEGER NOT NULL DEFAULT 0,
    approval_policy TEXT,
    result TEXT,
    error TEXT,
    created_at TEXT NOT NULL,
    started_at TEXT,
    completed_at TEXT,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_agent_tasks_state
    ON agent_tasks(state, created_at);
CREATE INDEX IF NOT EXISTS idx_agent_tasks_owner
    ON agent_tasks(owner_bot_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_agent_tasks_target
    ON agent_tasks(target_bot_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_agent_tasks_thread
    ON agent_tasks(thread_id);

CREATE TABLE IF NOT EXISTS agent_waits (
    id TEXT PRIMARY KEY NOT NULL,
    bot_id TEXT NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    run_id TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    event_kind TEXT NOT NULL,
    filter TEXT NOT NULL DEFAULT '{}',
    deadline TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'waiting',
    payload TEXT,
    created_at TEXT NOT NULL,
    fired_at TEXT,
    consumed_at TEXT,
    closed_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_agent_waits_state
    ON agent_waits(state, deadline);
CREATE INDEX IF NOT EXISTS idx_agent_waits_run
    ON agent_waits(run_id, created_at);
CREATE INDEX IF NOT EXISTS idx_agent_waits_bot
    ON agent_waits(bot_id, created_at DESC);
