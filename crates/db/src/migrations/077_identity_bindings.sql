-- Identity bindings for remote devices and durable work.
--
-- Remote access pairs a device with a role. This migration lets that device
-- also be bound to a local RAVENBOT account, so remote actions are attributed
-- to a person and the account's workspace grant can cap the device role.
--
-- Work items already record the bot that owns a task. `owner_account` is the
-- human account that has claimed it; the nullable pair is deliberate, because
-- a task can be unclaimed and a claim can be released. `claimed_at` is the
-- compare-and-set timestamp, not a lease: graph execution keeps its own lease.

ALTER TABLE remote_devices ADD COLUMN account_id TEXT;
ALTER TABLE remote_devices ADD COLUMN account_name TEXT;
ALTER TABLE remote_devices ADD COLUMN account_role TEXT;

ALTER TABLE remote_audit ADD COLUMN account_name TEXT;

ALTER TABLE work_items ADD COLUMN owner_account TEXT;
ALTER TABLE work_items ADD COLUMN claimed_at TEXT;

CREATE INDEX IF NOT EXISTS idx_work_items_owner
    ON work_items(owner_account, updated_at DESC);
