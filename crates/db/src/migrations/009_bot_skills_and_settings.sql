-- Bot skill persistence + provider key + app settings storage
-- Version 9

-- Per-bot enabled skills as JSON (atomic with the bot row, backfilled from
-- the legacy bot_skills junction table, which remains for compatibility)
ALTER TABLE bots ADD COLUMN skills TEXT NOT NULL DEFAULT '[]';

-- Backfill: bots that have junction rows get them as their skills JSON
UPDATE bots SET skills = (
    SELECT COALESCE(json_group_array(skill_id), '[]')
    FROM bot_skills WHERE bot_skills.bot_id = bots.id
) WHERE EXISTS (SELECT 1 FROM bot_skills WHERE bot_skills.bot_id = bots.id);

-- Provider API keys persisted across restarts (values stored as provided;
-- OS keychain migration is tracked separately)
CREATE TABLE IF NOT EXISTS provider_keys (
    provider TEXT PRIMARY KEY NOT NULL,
    api_key TEXT NOT NULL DEFAULT '',
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Generic app settings (ollama_url, defaults, ...)
CREATE TABLE IF NOT EXISTS app_settings (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL DEFAULT '',
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
