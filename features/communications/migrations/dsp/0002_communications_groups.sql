-- Messaging a group: one message to many members of the DSP's workspaces, each getting it in
-- their own direct message with the app, their answers read together. Kept, as the rest, until
-- the DSP is deleted. Shipped migrations never change: add the next one instead.

-- A group: whom someone at the DSP messaged together, under the name the page gave it. It's
-- listed among the conversations, and whether it waits follows from the conversations listed
-- under it, as a conversation's follows from its messages: `state`, since when, and when it
-- was last written in (`last_at`), by the DSP or in an answer.
CREATE TABLE IF NOT EXISTS communications_groups (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    created_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    state TEXT NOT NULL DEFAULT 'responded' CHECK (state IN ('waiting', 'responded')),
    waiting_since INTEGER,
    last_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS communications_groups_latest
    ON communications_groups (last_at, id);

-- Who a group is: each member it went to, once, by their workspace and ID there, in the order
-- they were added (`place`), under the name their Slack profile gave them then.
CREATE TABLE IF NOT EXISTS communications_group_people (
    group_id TEXT NOT NULL REFERENCES communications_groups (id),
    team_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    name TEXT NOT NULL,
    place INTEGER NOT NULL,
    PRIMARY KEY (group_id, team_id, user_id)
);
CREATE INDEX IF NOT EXISTS communications_group_people_members
    ON communications_group_people (team_id, user_id);

-- Each message someone at the DSP sent a group, by their account (`sent_by`), under the name it
-- goes out under (`sender_name`), and the letters of their initials, for the image it goes out
-- under when they have no Slack photo.
CREATE TABLE IF NOT EXISTS communications_group_messages (
    id TEXT PRIMARY KEY,
    group_id TEXT NOT NULL REFERENCES communications_groups (id),
    text TEXT NOT NULL,
    sent_by TEXT NOT NULL,
    sender_name TEXT NOT NULL,
    sender_initials TEXT,
    at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS communications_group_messages_latest
    ON communications_group_messages (group_id, at);

-- A group message going to each of its people: waiting to go (due at `due_at`, claimed while a
-- try has it), sent (`delivered_id`, the message in their conversation), or not sent (`failed`,
-- with why). A try that posted to Slack and stopped before keeping it leaves `posting_since`,
-- so the next looks for that post before posting again.
CREATE TABLE IF NOT EXISTS communications_deliveries (
    message_id TEXT NOT NULL REFERENCES communications_group_messages (id),
    team_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'waiting' CHECK (status IN ('waiting', 'sent', 'failed')),
    delivered_id TEXT REFERENCES communications_messages (id),
    due_at INTEGER NOT NULL,
    claimed_until INTEGER,
    posting_since INTEGER,
    attempts INTEGER NOT NULL DEFAULT 0,
    reason TEXT,
    PRIMARY KEY (message_id, team_id, user_id)
);
CREATE INDEX IF NOT EXISTS communications_deliveries_due
    ON communications_deliveries (status, due_at);

-- A group message's photo as uploaded to one workspace, by Slack's ID of it there, so each of
-- its people there is shown the one upload.
CREATE TABLE IF NOT EXISTS communications_group_uploads (
    message_id TEXT NOT NULL REFERENCES communications_group_messages (id),
    team_id TEXT NOT NULL,
    file_id TEXT NOT NULL REFERENCES communications_files (id),
    slack_file_id TEXT NOT NULL,
    PRIMARY KEY (message_id, team_id, file_id)
);
