-- Native Slack connector via Socket Mode. No public webhook port is required.
-- Pairing codes are hashes, and the channel link maps a Slack channel to one
-- RAVENBOT conversation.

CREATE TABLE IF NOT EXISTS slack_pairings (
    code_hash TEXT PRIMARY KEY NOT NULL,
    bot_id TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    used_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS slack_links (
    channel_id TEXT PRIMARY KEY NOT NULL,
    chat_title TEXT,
    bot_id TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_seen_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_slack_links_thread ON slack_links(thread_id);

-- Slack can redeliver an unacknowledged event after a reconnect. The pair of
-- channel id and event timestamp makes handling idempotent.
CREATE TABLE IF NOT EXISTS slack_events (
    channel_id TEXT NOT NULL,
    event_ts TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (channel_id, event_ts)
);

CREATE INDEX IF NOT EXISTS idx_slack_events_created ON slack_events(created_at);
