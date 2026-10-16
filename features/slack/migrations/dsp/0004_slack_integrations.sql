-- The integrations switched on in each of the DSP's Slack workspaces: what other features do
-- through Slack, by the IDs they declare them under, each with who switched it on and when. One
-- switched off has no row, and a workspace's go with it.
CREATE TABLE IF NOT EXISTS slack_integrations (
    team_id TEXT NOT NULL,
    integration TEXT NOT NULL,
    turned_on_by TEXT NOT NULL,
    turned_on_at TEXT NOT NULL,
    PRIMARY KEY (team_id, integration)
);
