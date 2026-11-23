-- Native Telegram connector. Pairings and chat links are separate from the
-- generic MCP connector so an inbound chat can be routed to a real thread.
-- Pairing codes are stored as SHA-256 hashes. The bot token is stored through
-- the keychain path with an owner-only database fallback.

CREATE TABLE IF NOT EXISTS telegram_pairings (
    code_hash TEXT PRIMARY KEY NOT NULL,
    bot_id TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    used_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS telegram_links (
    chat_id TEXT PRIMARY KEY NOT NULL,
    chat_title TEXT,
    bot_id TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_seen_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_telegram_links_thread ON telegram_links(thread_id);
