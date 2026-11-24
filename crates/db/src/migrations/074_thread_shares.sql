-- Read-only public conversation links served by the remote access listener.
--
-- Only the SHA-256 of the token is stored. A share is a snapshot boundary:
-- messages created after `created_at` are not exposed, so a link cannot leak
-- later turns added to the same thread. Revoking keeps the audit row but makes
-- the token unusable.
CREATE TABLE IF NOT EXISTS thread_shares (
  id TEXT PRIMARY KEY,
  thread_id TEXT NOT NULL,
  token_hash TEXT NOT NULL UNIQUE,
  created_at TEXT NOT NULL,
  expires_at TEXT NOT NULL,
  revoked_at TEXT,
  view_count INTEGER NOT NULL DEFAULT 0,
  last_viewed_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_thread_shares_thread
  ON thread_shares(thread_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_thread_shares_expiry
  ON thread_shares(expires_at);
