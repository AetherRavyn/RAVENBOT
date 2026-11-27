-- Remote SSO (OIDC) attempts.
--
-- One row per authorization-code flow. The state is stored only as a SHA-256
-- hash; the nonce and PKCE verifier must remain recoverable until the callback
-- finishes, so they live here only for the ten-minute attempt TTL and the row
-- is deleted on first use. No authorization code, access token or ID token is
-- ever written here.

CREATE TABLE IF NOT EXISTS remote_sso_attempts (
    id TEXT PRIMARY KEY NOT NULL,
    state_hash TEXT NOT NULL UNIQUE,
    device_id TEXT NOT NULL,
    nonce TEXT NOT NULL,
    verifier TEXT NOT NULL,
    issuer TEXT NOT NULL,
    client_id TEXT NOT NULL,
    redirect_url TEXT NOT NULL,
    token_endpoint TEXT NOT NULL,
    jwks_uri TEXT NOT NULL,
    username_claim TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_remote_sso_attempts_expires
    ON remote_sso_attempts(expires_at);

CREATE INDEX IF NOT EXISTS idx_remote_sso_attempts_device
    ON remote_sso_attempts(device_id, expires_at);
