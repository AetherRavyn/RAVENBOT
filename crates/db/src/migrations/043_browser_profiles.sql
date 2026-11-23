-- Named browser profiles that can be selected across agent/office scopes.
-- Automatic `agent-*` / `office-*` directories remain unregistered and are
-- managed by the scope selector.
CREATE TABLE IF NOT EXISTS browser_profiles (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_browser_profiles_name
    ON browser_profiles(name COLLATE NOCASE);
