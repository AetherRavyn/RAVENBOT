-- Interrupted top-level agent runs are parked as paused so startup can resume
-- them from their checkpoint. The flag distinguishes a crash-park from a
-- deliberate user pause, which must stay paused until the user asks.
ALTER TABLE runs ADD COLUMN resume_on_start INTEGER NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS idx_runs_resume_on_start
    ON runs(resume_on_start, state);
