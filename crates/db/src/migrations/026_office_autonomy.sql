-- Office agents operate autonomously.
--
-- Office work is a 24/7 job — parking every read or shell command on an
-- Allow/Deny card made it impossible. The safety boundary is now the
-- `ravenbot_skills::safety` denylist (rm of the machine/workspace root,
-- disk/partition/power commands, destructive git, privileged docker), which
-- refuses those commands outright regardless of approval mode.
--
-- Only agents that are members of an office are changed. A plain fleet agent
-- used for interactive chat keeps its own approval mode.
UPDATE bots
SET approval_mode = 'full',
    updated_at = datetime('now')
WHERE id IN (SELECT bot_id FROM chatroom_members);
