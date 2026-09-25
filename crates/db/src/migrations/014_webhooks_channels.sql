-- Webhook triggers on routines + named channels (contexts)
-- Version 14

-- Inbound webhook trigger per routine. The secret is shown once on creation
-- and required as a bearer token. Disabled unless explicitly enabled.
ALTER TABLE routines ADD COLUMN webhook_secret TEXT;
ALTER TABLE routines ADD COLUMN webhook_enabled INTEGER NOT NULL DEFAULT 0;

-- Channels: named contexts (Work / Personal / project) with their own shared
-- instructions, working folder, and bot roster. Threads may be filed under one.
CREATE TABLE IF NOT EXISTS channels (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    instructions TEXT NOT NULL DEFAULT '',
    working_folder TEXT,
    color TEXT,
    position INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS channel_bots (
    channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    bot_id TEXT NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    PRIMARY KEY (channel_id, bot_id)
);

ALTER TABLE threads ADD COLUMN channel_id TEXT REFERENCES channels(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_threads_channel ON threads(channel_id);
