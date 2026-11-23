-- Local credential vault. Metadata stays searchable, while the secret payload lives
-- in the OS keychain when available, with an owner-only SQLite fallback.
CREATE TABLE IF NOT EXISTS vault_items (
    id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL DEFAULT 'login',
    name TEXT NOT NULL,
    username TEXT NOT NULL DEFAULT '',
    website TEXT NOT NULL DEFAULT '',
    last4 TEXT,
    secret TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_vault_items_updated
    ON vault_items(updated_at DESC);
