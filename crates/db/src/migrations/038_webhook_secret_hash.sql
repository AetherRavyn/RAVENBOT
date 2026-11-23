-- Webhook bearer tokens are no longer queried by their plaintext value.
-- The hash lets the receiver route a token without a secret in SQL, while the
-- token itself lives in the OS keychain (or the owner-only fallback).
ALTER TABLE routines ADD COLUMN webhook_secret_hash TEXT;
CREATE INDEX IF NOT EXISTS idx_routines_webhook_hash
  ON routines(webhook_secret_hash, webhook_enabled);
