-- Chatroom support + bot avatar/rank extensions
ALTER TABLE bots ADD COLUMN avatar_url TEXT;
ALTER TABLE bots ADD COLUMN avatar_style TEXT;
ALTER TABLE bots ADD COLUMN rank TEXT;
ALTER TABLE bots ADD COLUMN specialty TEXT;

-- Chatrooms (office teams)
CREATE TABLE IF NOT EXISTS chatrooms (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    office_template TEXT NOT NULL DEFAULT 'custom',
    avatar_url TEXT,
    avatar_style TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Chatroom members with rank/specialty per membership (overrides bot rank if present)
CREATE TABLE IF NOT EXISTS chatroom_members (
    chatroom_id TEXT NOT NULL REFERENCES chatrooms(id) ON DELETE CASCADE,
    bot_id TEXT NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    rank TEXT NOT NULL,
    specialty TEXT NOT NULL,
    joined_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (chatroom_id, bot_id)
);

CREATE INDEX idx_chatroom_members_chatroom ON chatroom_members(chatroom_id);
CREATE INDEX idx_chatroom_members_bot ON chatroom_members(bot_id);

-- Group thread mapping: a chatroom has one group thread for collaboration
CREATE TABLE IF NOT EXISTS chatroom_threads (
    chatroom_id TEXT PRIMARY KEY NOT NULL REFERENCES chatrooms(id) ON DELETE CASCADE,
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
