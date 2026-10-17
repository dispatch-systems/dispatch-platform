-- Sync Canvases, Slack's own integration: the canvases the DSP keeps as documents.

-- Each channel linked to a folder where the DSP keeps its files: every canvas in it is kept
-- there, as canvases come. Its name, and the folder's, as they were last seen; who linked it and
-- when, in milliseconds. A workspace's go with it.
CREATE TABLE IF NOT EXISTS slack_canvas_links (
    team_id TEXT NOT NULL,
    channel_id TEXT NOT NULL,
    channel_name TEXT NOT NULL,
    private INTEGER NOT NULL CHECK (private IN (0, 1)),
    folder_id TEXT NOT NULL,
    folder_name TEXT NOT NULL,
    linked_by TEXT NOT NULL,
    linked_at INTEGER NOT NULL,
    PRIMARY KEY (team_id, channel_id)
);

-- Each canvas chosen on its own, from a channel's, to keep in a folder where the DSP keeps its
-- files. Its title, its channel's name and the folder's, as they were last seen; who chose it
-- and when, in milliseconds. A workspace's go with it.
CREATE TABLE IF NOT EXISTS slack_canvas_picks (
    team_id TEXT NOT NULL,
    canvas_id TEXT NOT NULL,
    title TEXT NOT NULL,
    channel_id TEXT NOT NULL,
    channel_name TEXT NOT NULL,
    private INTEGER NOT NULL CHECK (private IN (0, 1)),
    folder_id TEXT NOT NULL,
    folder_name TEXT NOT NULL,
    picked_by TEXT NOT NULL,
    picked_at INTEGER NOT NULL,
    PRIMARY KEY (team_id, canvas_id)
);

-- Each canvas a link or a pick covers, once, by Slack's ID, with the channel it was last found
-- in: when Slack last said it was edited, the edit its document holds, and the edit it was last
-- left at, gone or failed, in milliseconds; when
-- Slack last told of a change, and when the try under way began and ends; and how syncing it
-- goes: waiting, with when it's next tried and since when, unless a try under way began
-- since; synced, with its document; or left, gone from Slack or failed until it's edited
-- again. Kept while nothing covers it, for its document to be written over once something does
-- again. A workspace's go with it.
CREATE TABLE IF NOT EXISTS slack_canvases (
    team_id TEXT NOT NULL,
    canvas_id TEXT NOT NULL,
    channel_id TEXT NOT NULL,
    title TEXT,
    status TEXT NOT NULL CHECK (status IN ('waiting', 'synced', 'gone', 'failed')),
    reason TEXT,
    attempts INTEGER NOT NULL DEFAULT 0,
    due_at INTEGER NOT NULL,
    waiting_since INTEGER,
    heard_at INTEGER,
    claimed_at INTEGER,
    claimed_until INTEGER,
    edited_at INTEGER,
    written_edit INTEGER,
    tried_edit INTEGER,
    doc_id TEXT,
    doc_link TEXT,
    noticed_at INTEGER NOT NULL,
    synced_at INTEGER,
    PRIMARY KEY (team_id, canvas_id)
);
CREATE INDEX IF NOT EXISTS slack_canvases_due ON slack_canvases (status, due_at);
