-- Native Microsoft Teams connector over the Bot Framework. A conversation is
-- keyed by its Teams conversation id, and the stored service URL is used only
-- for replying to that conversation. Pairing codes are stored as hashes.

CREATE TABLE IF NOT EXISTS teams_pairings (
    code_hash TEXT PRIMARY KEY NOT NULL,
    bot_id TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    used_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS teams_links (
    conversation_id TEXT PRIMARY KEY NOT NULL,
    tenant_id TEXT NOT NULL,
    conversation_type TEXT,
    service_url TEXT NOT NULL,
    chat_title TEXT,
    bot_id TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_seen_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_teams_links_thread ON teams_links(thread_id);

CREATE TABLE IF NOT EXISTS teams_events (
    conversation_id TEXT NOT NULL,
    activity_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (conversation_id, activity_id)
);

CREATE INDEX IF NOT EXISTS idx_teams_events_created ON teams_events(created_at);
