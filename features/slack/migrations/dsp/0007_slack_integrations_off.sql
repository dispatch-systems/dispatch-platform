-- The integrations that start on in every workspace, switched off in one of the DSP's: each
-- with who switched it off and when. Switched on again, it has no row here, and a workspace's
-- go with it, so one connected again has them on.
CREATE TABLE IF NOT EXISTS slack_integrations_off (
    team_id TEXT NOT NULL,
    integration TEXT NOT NULL,
    turned_off_by TEXT NOT NULL,
    turned_off_at TEXT NOT NULL,
    PRIMARY KEY (team_id, integration)
);
