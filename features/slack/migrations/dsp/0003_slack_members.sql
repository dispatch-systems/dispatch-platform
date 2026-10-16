-- The members of each of the DSP's Slack workspaces, as Slack lists them, for Driver Match to
-- tell apart: their name, the name they go by in Slack when it's another, the email address
-- and phone number on their profile, whether their account is active, and whether they're a
-- guest. Bots aren't kept, and a workspace's members go with it.
CREATE TABLE IF NOT EXISTS slack_members (
    team_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    name TEXT NOT NULL,
    display_name TEXT,
    email TEXT,
    phone TEXT,
    active INTEGER NOT NULL CHECK (active IN (0, 1)),
    guest INTEGER NOT NULL CHECK (guest IN (0, 1)),
    PRIMARY KEY (team_id, user_id)
);
