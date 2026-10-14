-- What Slack keeps for a DSP. Shipped migrations never change: add the next one instead.

-- Each Slack workspace the DSP added Dispatch to: what its admin let the bot do, as `scopes`,
-- space-separated, and the member who connected it, none when it came by the install link. Its
-- bot's tokens sit encrypted in the DSP's secrets, never here; the platform's `slack_teams`
-- names the DSP each belongs to.
CREATE TABLE IF NOT EXISTS slack_workspaces (
    team_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('connected', 'broken')),
    scopes TEXT NOT NULL,
    bot_user_id TEXT NOT NULL,
    app_id TEXT NOT NULL,
    connected_by TEXT,
    connected_at TEXT NOT NULL,
    broken_at TEXT
);

-- An install someone started and Slack hasn't sent back yet: the member who started it, or the
-- install link it came by. Each is used once, within ten minutes.
CREATE TABLE IF NOT EXISTS slack_connect_requests (
    state_hash TEXT PRIMARY KEY,
    user_id TEXT,
    link_hash TEXT,
    expires_at INTEGER NOT NULL,
    CHECK ((user_id IS NULL) <> (link_hash IS NULL))
);

-- The DSP's install link, for whoever runs its Slack to add Dispatch there without signing in
-- to Dispatch: at most one, used once, within seven days.
CREATE TABLE IF NOT EXISTS slack_install_links (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    link_hash TEXT NOT NULL,
    created_by TEXT NOT NULL,
    created_at TEXT NOT NULL,
    expires_at INTEGER NOT NULL
);
