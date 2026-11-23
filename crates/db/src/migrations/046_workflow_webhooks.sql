-- Inbound loopback webhook triggers for saved workflows. The URL token itself
-- is never stored, only its SHA-256 for routing.
CREATE TABLE IF NOT EXISTS workflow_webhooks (
    workflow_id TEXT PRIMARY KEY NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
    secret_hash TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_workflow_webhooks_secret
    ON workflow_webhooks(secret_hash);
