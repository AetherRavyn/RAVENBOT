-- Native Discord connector over the Gateway WebSocket. No public webhook or
-- interactions endpoint is required. Pairing codes are stored as hashes and a
-- channel link maps a Discord channel or DM to one RAVENBOT conversation.

CREATE TABLE IF NOT EXISTS discord_pairings (
    code_hash TEXT PRIMARY KEY NOT NULL,
    bot_id TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    used_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS discord_links (
    channel_id TEXT PRIMARY KEY NOT NULL,
    guild_id TEXT,
    chat_title TEXT,
    bot_id TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_seen_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_discord_links_thread ON discord_links(thread_id);

-- Gateway sessions start fresh rather than resuming, but a reconnect or a
-- duplicated dispatch must not run the same message twice.
CREATE TABLE IF NOT EXISTS discord_events (
    channel_id TEXT NOT NULL,
    message_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (channel_id, message_id)
);

CREATE INDEX IF NOT EXISTS idx_discord_events_created ON discord_events(created_at);
