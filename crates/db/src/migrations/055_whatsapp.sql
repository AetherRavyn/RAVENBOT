-- Native WhatsApp Cloud API connector. Meta delivers inbound messages over a
-- signed HTTPS webhook. Pairing codes are stored as hashes and a phone number
-- maps to one RAVENBOT conversation.

CREATE TABLE IF NOT EXISTS whatsapp_pairings (
    code_hash TEXT PRIMARY KEY NOT NULL,
    bot_id TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    used_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS whatsapp_links (
    wa_id TEXT PRIMARY KEY NOT NULL,
    display_name TEXT,
    bot_id TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_seen_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_whatsapp_links_thread ON whatsapp_links(thread_id);

-- Meta retries a webhook when it does not receive a timely 200. A message is
-- claimed once so a retry cannot start the same run twice.
CREATE TABLE IF NOT EXISTS whatsapp_events (
    wa_id TEXT NOT NULL,
    message_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (wa_id, message_id)
);

CREATE INDEX IF NOT EXISTS idx_whatsapp_events_created ON whatsapp_events(created_at);
