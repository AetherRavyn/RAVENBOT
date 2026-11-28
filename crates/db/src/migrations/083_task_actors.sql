-- Carry the caller's workspace actor onto background task runs.
--
-- A task started by an operator or a paired-chat identity must not run with the
-- owner-only configuration powers of a local desktop run. The actor is copied
-- when the task is created and applied to the task's run by the host loop.
ALTER TABLE agent_tasks ADD COLUMN actor_id TEXT;
ALTER TABLE agent_tasks ADD COLUMN actor_name TEXT;
ALTER TABLE agent_tasks ADD COLUMN actor_role TEXT;
ALTER TABLE agent_tasks ADD COLUMN actor_workspace_id TEXT;
