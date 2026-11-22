-- Slack's migration 12: each member's profile picture, as Dispatch keeps a copy of it for the
-- person Driver Match knows them as: the address Slack gave it at, which it's downloaded from
-- again only once Slack gives another; the version the address the page asks for it at carries;
-- and the picture, a PNG, JPEG or GIF. A member with Slack's own picture has none, and a
-- member's goes with them.
CREATE TABLE IF NOT EXISTS slack_member_pictures (
    team_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    source TEXT NOT NULL,
    version TEXT NOT NULL,
    picture BLOB NOT NULL,
    PRIMARY KEY (team_id, user_id)
);
