-- Stable queue order. Existing rows keep their creation order, and new rows get a
-- position after the current tail. Reordering only changes pending/failed rows.
ALTER TABLE office_queue ADD COLUMN position INTEGER NOT NULL DEFAULT 0;
UPDATE office_queue SET position = rowid;
