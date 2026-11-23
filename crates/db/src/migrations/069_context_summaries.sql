-- Durable context summaries.
--
-- Compaction used to live only in the model message list for one run. The next
-- run reloaded the whole transcript and compacted it again. This table stores
-- the structured handoff and the message it covers through, so a later run can
-- load `summary + messages after the floor` instead.
--
-- The full transcript is never deleted. `messages_fts` still indexes it, so a
-- summarized-away detail remains recoverable with `session_search`.
CREATE TABLE IF NOT EXISTS context_summaries (
    id TEXT PRIMARY KEY NOT NULL,
    thread_id TEXT NOT NULL,
    through_message_id TEXT,
    summary TEXT NOT NULL,
    token_estimate INTEGER NOT NULL DEFAULT 0,
    model TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f+00:00', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_context_summaries_thread
    ON context_summaries(thread_id, created_at DESC);

ALTER TABLE threads ADD COLUMN context_summary_id TEXT;
ALTER TABLE threads ADD COLUMN context_floor_message_id TEXT;
