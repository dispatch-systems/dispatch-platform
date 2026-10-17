-- Import Files, Slack's own integration: what the DSP imports from its workspaces' channels.

-- Each channel linked to a folder where the DSP keeps its files: the files posted there from
-- when it was linked are saved to that folder. Its name, and the folder's, as they were last
-- seen; who linked it and when, in milliseconds. A workspace's go with it.
CREATE TABLE IF NOT EXISTS slack_import_links (
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

-- Each file Slack told of, or listed, in a linked channel: once, by Slack's ID, whichever
-- channels it's in. What's known of it, who posted it, and how saving it went: waiting, with
-- when it's next tried; saved, with where it went; or left, as too large, not a file Slack
-- hosts, gone from Slack or failed. A workspace's go with it.
CREATE TABLE IF NOT EXISTS slack_imports (
    team_id TEXT NOT NULL,
    file_id TEXT NOT NULL,
    channel_id TEXT NOT NULL,
    name TEXT,
    mime TEXT,
    size INTEGER,
    posted_by TEXT,
    posted_name TEXT,
    status TEXT NOT NULL
        CHECK (status IN ('waiting', 'saved', 'too_large', 'skipped', 'gone', 'failed')),
    reason TEXT,
    attempts INTEGER NOT NULL DEFAULT 0,
    due_at INTEGER NOT NULL,
    saved_id TEXT,
    saved_link TEXT,
    noticed_at INTEGER NOT NULL,
    done_at INTEGER,
    PRIMARY KEY (team_id, file_id)
);
CREATE INDEX IF NOT EXISTS slack_imports_due ON slack_imports (status, due_at);
CREATE INDEX IF NOT EXISTS slack_imports_recent ON slack_imports (team_id, noticed_at);
