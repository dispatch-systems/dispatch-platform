-- Slack's migration 9: an install link for each of the DSP's workspaces, to update or reconnect
-- it, beside the one for adding a workspace: each works once, within seven days, and a new one
-- takes the place only of the one for the same workspace (`team_id`), or of the one for adding
-- a workspace (none). A workspace's goes with it. The link the DSP had stays, for adding one.
CREATE TABLE slack_install_links_by_workspace (
    link_hash TEXT PRIMARY KEY,
    team_id TEXT,
    created_by TEXT NOT NULL,
    created_at TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    label TEXT
);
INSERT INTO slack_install_links_by_workspace(link_hash,team_id,created_by,created_at,expires_at,label)
    SELECT link_hash,NULL,created_by,created_at,expires_at,label FROM slack_install_links;
DROP TABLE slack_install_links;
ALTER TABLE slack_install_links_by_workspace RENAME TO slack_install_links;
CREATE UNIQUE INDEX slack_install_links_for ON slack_install_links(ifnull(team_id, ''));
