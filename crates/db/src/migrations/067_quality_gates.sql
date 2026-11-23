-- Deterministic quality-gate results for office completion.
--
-- The lead's completion review is a model call; a red gate is evidence it
-- cannot overrule. Each run is kept so a human can see which command was run,
-- where, how long it took, and the tail of its output.
CREATE TABLE IF NOT EXISTS office_gate_runs (
    id TEXT PRIMARY KEY NOT NULL,
    office_id TEXT NOT NULL,
    thread_id TEXT,
    cycle INTEGER NOT NULL DEFAULT 0,
    command TEXT NOT NULL,
    cwd TEXT NOT NULL,
    exit_code INTEGER,
    passed INTEGER NOT NULL DEFAULT 0,
    timed_out INTEGER NOT NULL DEFAULT 0,
    duration_ms INTEGER NOT NULL DEFAULT 0,
    output_tail TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f+00:00', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_office_gate_runs_office
    ON office_gate_runs(office_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_office_gate_runs_thread
    ON office_gate_runs(thread_id, created_at DESC);
