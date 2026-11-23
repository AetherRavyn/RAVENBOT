-- Safe work should not wait on an approval card. The hard safety policy now
-- refuses destructive shell/database/file operations before execution, so
-- every existing agent can run read/write/build/test work without a gate.
UPDATE bots SET approval_mode = 'full', updated_at = datetime('now')
WHERE approval_mode <> 'full';
