-- Approval gates ("bots ask before they act") + junction backfill
-- Version 10

-- Per-bot approval mode (ask/auto/full). Column form keeps reads cheap;
-- unknown values fail closed to ask at parse time.
ALTER TABLE bots ADD COLUMN approval_mode TEXT NOT NULL DEFAULT 'ask';

-- Pending approval requests: one row per parked tool call. The transcript
-- card, the composer block, and the resume path all read this table.
CREATE TABLE IF NOT EXISTS approvals (
    id TEXT PRIMARY KEY NOT NULL,
    bot_id TEXT NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    run_id TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    tool_label TEXT NOT NULL DEFAULT '',
    arguments TEXT NOT NULL DEFAULT '{}',
    risk TEXT NOT NULL DEFAULT 'high',
    status TEXT NOT NULL DEFAULT 'pending',
    note TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    decided_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_approvals_thread ON approvals(thread_id, status);
CREATE INDEX IF NOT EXISTS idx_approvals_run ON approvals(run_id, status);
CREATE INDEX IF NOT EXISTS idx_approvals_bot ON approvals(bot_id, status);
