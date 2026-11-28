-- API keys requested through the Vault. The secret still lives in the same
-- sealed `secret` column as a login; `key_name` is the machine-safe name that
-- becomes the agent-readable file under keys/api-keys. Existing rows keep a
-- NULL key_name, so the partial unique index leaves them alone.
ALTER TABLE vault_items ADD COLUMN key_name TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_vault_items_key_name
    ON vault_items(key_name) WHERE key_name IS NOT NULL;
