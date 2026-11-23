-- Remote access: paired devices, short-lived pairing codes, and an audit trail.
--
-- Tokens and pairing codes are stored only as SHA-256 hashes. A database read
-- cannot recover a usable token. Device rows are revoked rather than deleted so
-- the audit trail keeps a name to attribute past actions to.

CREATE TABLE IF NOT EXISTS remote_devices (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'viewer',
    token_hash TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_seen_at TEXT,
    revoked_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_remote_devices_token
    ON remote_devices(token_hash);

CREATE TABLE IF NOT EXISTS remote_pairings (
    id TEXT PRIMARY KEY NOT NULL,
    code_hash TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'viewer',
    expires_at TEXT NOT NULL,
    used_at TEXT,
    attempts INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_remote_pairings_code
    ON remote_pairings(code_hash, used_at);

CREATE TABLE IF NOT EXISTS remote_audit (
    id TEXT PRIMARY KEY NOT NULL,
    device_id TEXT,
    device_name TEXT,
    role TEXT,
    method TEXT NOT NULL,
    path TEXT NOT NULL,
    status INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_remote_audit_created
    ON remote_audit(created_at);
