-- Production-level office: goal, policy, terms, budget, manual agent management
ALTER TABLE chatrooms ADD COLUMN goal TEXT;
ALTER TABLE chatrooms ADD COLUMN policy TEXT;
ALTER TABLE chatrooms ADD COLUMN terms TEXT;
ALTER TABLE chatrooms ADD COLUMN budget REAL;
ALTER TABLE chatrooms ADD COLUMN budget_distribution TEXT; -- JSON: {bot_id: amount}
ALTER TABLE chatrooms ADD COLUMN created_by TEXT;

-- Ensure chatroom_members has proper handling for manual additions
CREATE INDEX IF NOT EXISTS idx_chatrooms_goal ON chatrooms(goal);
