-- Which DSP each Slack workspace belongs to: one only. Shipped migrations never change: add the
-- next one instead. Slack names the workspace in every request it sends, never the DSP, so a
-- request finds its DSP here.
CREATE TABLE IF NOT EXISTS slack_teams (
    team_id TEXT PRIMARY KEY,
    dsp_id TEXT NOT NULL,
    connected_at TEXT NOT NULL
);
