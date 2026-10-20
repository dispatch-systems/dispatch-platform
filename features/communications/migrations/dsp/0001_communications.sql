-- What Communications keeps for a DSP: its direct messages with each member of its Slack
-- workspaces, and the photos and files attached to them, until the DSP is deleted. A workspace
-- disconnected leaves them. Shipped migrations never change: add the next one instead.

-- One conversation with each member of a workspace (`user_id`), in the direct message the app has
-- with them there (`channel_id`), under the name their Slack profile last gave them. One with no
-- message yet (`last_at` null) was opened, or found by catching up, and holds nothing. Whether
-- it waits on an answer follows from its messages and when someone at the DSP marked it done
-- (`done_at`), and is kept beside them: `state`, and since when it waits. Catching up has read
-- its messages up to `caught_up_at`.
CREATE TABLE IF NOT EXISTS communications_conversations (
    id TEXT PRIMARY KEY,
    team_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    channel_id TEXT,
    name TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'responded' CHECK (state IN ('waiting', 'responded', 'done')),
    waiting_since INTEGER,
    last_at INTEGER,
    done_at INTEGER,
    done_by TEXT,
    caught_up_at INTEGER,
    UNIQUE (team_id, user_id)
);
CREATE INDEX IF NOT EXISTS communications_conversations_latest
    ON communications_conversations (last_at, id);
CREATE INDEX IF NOT EXISTS communications_conversations_channels
    ON communications_conversations (team_id, channel_id);

-- Each message, by Slack's `ts` in its conversation, which orders them: one the member wrote (`in`), or one someone
-- at the DSP sent (`out`), by their account (`sent_by`) under the name it went out under
-- (`sender_name`). `thread_ts` is the thread it was written in, if any. An edit in Slack keeps
-- only its latest text, with when it was made (`edited_at`).
CREATE TABLE IF NOT EXISTS communications_messages (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES communications_conversations (id),
    ts TEXT NOT NULL,
    thread_ts TEXT,
    direction TEXT NOT NULL CHECK (direction IN ('in', 'out')),
    sent_by TEXT,
    sender_name TEXT,
    text TEXT NOT NULL,
    at INTEGER NOT NULL,
    edited_at INTEGER,
    UNIQUE (conversation_id, ts)
);
CREATE INDEX IF NOT EXISTS communications_messages_latest
    ON communications_messages (conversation_id, at);

-- Each photo or file attached to a message: one a member sent, by Slack's ID, downloaded into the
-- DSP's own files (`waiting` until it is, tried again after a failure), or a photo someone at
-- the DSP added (`uploaded_by`), kept before its message is sent (`message_id` null). Its bytes
-- are kept by their SHA-256 (`sha256`). One too large to keep, or that Slack doesn't host, keeps
-- its name, type and size alone.
CREATE TABLE IF NOT EXISTS communications_files (
    id TEXT PRIMARY KEY,
    message_id TEXT REFERENCES communications_messages (id),
    slack_file_id TEXT,
    uploaded_by TEXT,
    name TEXT NOT NULL,
    mime TEXT NOT NULL,
    size INTEGER NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('waiting', 'kept', 'too_large', 'unavailable', 'failed')),
    sha256 TEXT,
    added_at INTEGER NOT NULL,
    due_at INTEGER,
    claimed_until INTEGER,
    attempts INTEGER NOT NULL DEFAULT 0,
    reason TEXT
);
CREATE INDEX IF NOT EXISTS communications_files_message ON communications_files (message_id);
CREATE INDEX IF NOT EXISTS communications_files_due ON communications_files (status, due_at);

-- Each change to a conversation, in order, so a page waiting on them reads only those it hasn't
-- seen. The latest thousand are kept: the latest always is, so each change's revision is after
-- every one before it.
CREATE TABLE IF NOT EXISTS communications_changes (
    revision INTEGER PRIMARY KEY,
    conversation_id TEXT NOT NULL
);
