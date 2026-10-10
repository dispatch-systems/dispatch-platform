-- agent_keys: keys the platform owner creates for outside agents. Only a SHA-256 of the
-- key is kept, with its last four characters to recognize it by. A key works for the user
-- who made it only while they are an active platform owner, and stops at expires_at
-- (never when null) or once revoked. access is what it may do: look things up ('read'),
-- or also run collections and test connections ('operator'). tools is the set of tools
-- it is offered; locations whether answers carry delivery addresses and GPS.
CREATE TABLE IF NOT EXISTS agent_keys (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    hash TEXT NOT NULL UNIQUE,
    hint TEXT NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id),
    all_dsps INTEGER NOT NULL CHECK(all_dsps IN (0,1)),
    access TEXT NOT NULL CHECK(access IN ('read','operator')),
    tools TEXT NOT NULL CHECK(tools IN ('full','essential')),
    locations INTEGER NOT NULL CHECK(locations IN (0,1)),
    created_at TEXT NOT NULL,
    expires_at TEXT,
    revoked_at TEXT,
    last_used_at TEXT,
    last_client TEXT
);
-- agent_key_dsps: the DSPs a key reaches when it does not reach them all.
CREATE TABLE IF NOT EXISTS agent_key_dsps (
    key_id TEXT NOT NULL REFERENCES agent_keys(id) ON DELETE CASCADE,
    dsp_id TEXT NOT NULL REFERENCES dsps(id),
    PRIMARY KEY(key_id, dsp_id)
);
