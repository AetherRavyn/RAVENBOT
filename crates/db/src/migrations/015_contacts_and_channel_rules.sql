CREATE TABLE IF NOT EXISTS bot_contacts (
    bot_id TEXT PRIMARY KEY REFERENCES bots(id) ON DELETE CASCADE,
    pinned INTEGER NOT NULL DEFAULT 0,
    hidden INTEGER NOT NULL DEFAULT 0,
    last_read_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_bot_contacts_pinned ON bot_contacts(pinned);

ALTER TABLE channels ADD COLUMN responder_rules TEXT;
