-- Durable automatic continuation for long-running goals.
--
-- A goal is not just a contract; it is a scheduler entry. `next_run_at` makes
-- an active goal wake up after a bounded delay even when the app is idle or was
-- closed. The round/retry counters keep repeated continuation from becoming an
-- unbounded token burn:
--   * rounds_used/max_rounds bound the total reviewed cycles for the goal.
--   * consecutive_failures drives exponential backoff after a failed cycle.
--   * blocked_rounds lets a blocker persist for three cycles before the goal is
--     parked, instead of abandoning the project on one bad turn.
ALTER TABLE goals ADD COLUMN next_run_at TEXT;
ALTER TABLE goals ADD COLUMN last_run_at TEXT;
ALTER TABLE goals ADD COLUMN rounds_used INTEGER NOT NULL DEFAULT 0;
ALTER TABLE goals ADD COLUMN max_rounds INTEGER NOT NULL DEFAULT 24;
ALTER TABLE goals ADD COLUMN consecutive_failures INTEGER NOT NULL DEFAULT 0;
ALTER TABLE goals ADD COLUMN blocked_rounds INTEGER NOT NULL DEFAULT 0;
ALTER TABLE goals ADD COLUMN last_blocker TEXT NOT NULL DEFAULT '';

CREATE INDEX IF NOT EXISTS idx_goals_due
    ON goals(status, next_run_at);

-- Legacy active goals have no schedule yet. Arm them once; the scheduler then
-- owns every subsequent wakeup through record_verdict().
UPDATE goals
   SET next_run_at = strftime('%Y-%m-%dT%H:%M:%f+00:00', 'now')
 WHERE status = 'active' AND next_run_at IS NULL;
