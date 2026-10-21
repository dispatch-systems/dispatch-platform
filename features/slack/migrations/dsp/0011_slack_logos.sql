-- Slack's migration 11: each workspace's logo, as Dispatch keeps a copy of it to show: the
-- address Slack gave it at, which it's downloaded from again only once Slack gives another; the
-- version the address the page asks for it at carries; and the picture, a PNG, JPEG or GIF. A
-- workspace whose logo is Slack's own has none, and a workspace's goes with it.
CREATE TABLE IF NOT EXISTS slack_logos (
    team_id TEXT PRIMARY KEY,
    source TEXT NOT NULL,
    version TEXT NOT NULL,
    picture BLOB NOT NULL
);
