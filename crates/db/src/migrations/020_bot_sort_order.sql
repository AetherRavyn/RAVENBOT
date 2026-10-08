-- Manual sidebar ordering (P4 drag-reorder). Bots sort by this first,
-- falling back to updated_at for rows never dragged (all zeros).
ALTER TABLE bots ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0;
