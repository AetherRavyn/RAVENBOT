-- Run liveness and an append-only work event log.
--
-- `runs.updated_at` records when a run's *state* changed. It cannot answer
-- "is this run still making progress?" because a model call or a long tool can
-- take minutes without a state transition. `heartbeat_at` is touched at model,
-- tool, approval, and checkpoint boundaries, so a watchdog can distinguish a
-- slow-but-working run from a wedged one.
--
-- `work_events` is append-only and deliberately not foreign-keyed: the event
-- outlives the row it describes, and a deleted task must not erase the record
-- that it ran.
ALTER TABLE runs ADD COLUMN heartbeat_at TEXT;
ALTER TABLE runs ADD COLUMN phase TEXT NOT NULL DEFAULT '';
ALTER TABLE runs ADD COLUMN last_tool TEXT;
ALTER TABLE runs ADD COLUMN last_error TEXT;
ALTER TABLE runs ADD COLUMN lease_owner TEXT;
ALTER TABLE runs ADD COLUMN lease_expires_at TEXT;

CREATE TABLE IF NOT EXISTS work_events (
    id TEXT PRIMARY KEY NOT NULL,
    work_item_id TEXT,
    run_id TEXT,
    thread_id TEXT,
    kind TEXT NOT NULL,
    payload_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f+00:00', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_work_events_run
    ON work_events(run_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_work_events_thread
    ON work_events(thread_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_work_events_kind
    ON work_events(kind, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_runs_heartbeat
    ON runs(state, heartbeat_at);
