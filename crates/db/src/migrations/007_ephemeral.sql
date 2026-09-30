-- Ephemeral (temporary) threads: skip agent-memory persistence
ALTER TABLE threads ADD COLUMN ephemeral INTEGER NOT NULL DEFAULT 0;
