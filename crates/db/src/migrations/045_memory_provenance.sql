-- Where a memory came from, for accountability and the memory panel.
-- Existing rows predate provenance, so they are marked unknown and their
-- updated timestamp is backfilled from creation time.
ALTER TABLE memory_facts ADD COLUMN source TEXT NOT NULL DEFAULT 'unknown';
ALTER TABLE memory_facts ADD COLUMN source_thread_id TEXT;
ALTER TABLE memory_facts ADD COLUMN source_run_id TEXT;
ALTER TABLE memory_facts ADD COLUMN updated_at TEXT NOT NULL DEFAULT '';
UPDATE memory_facts SET updated_at = created_at WHERE updated_at = '';

ALTER TABLE office_memories ADD COLUMN source TEXT NOT NULL DEFAULT 'unknown';
ALTER TABLE office_memories ADD COLUMN source_thread_id TEXT;
ALTER TABLE office_memories ADD COLUMN source_run_id TEXT;
ALTER TABLE office_memories ADD COLUMN updated_at TEXT NOT NULL DEFAULT '';
UPDATE office_memories SET updated_at = created_at WHERE updated_at = '';
