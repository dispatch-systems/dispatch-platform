-- What adding Slack from New on Settings' DSP Connections keeps. Each workspace's name in
-- Dispatch, `label`, which whoever adds it may give it; without one it's known by its name in
-- Slack. An install a member started: the workspace it named to Slack, to reconnect or update
-- it, none for one started to add a workspace, which mustn't then be one the DSP already has;
-- and, like the install link, the name the workspace it adds is to have.
ALTER TABLE slack_workspaces ADD COLUMN label TEXT;
ALTER TABLE slack_connect_requests ADD COLUMN team_id TEXT;
ALTER TABLE slack_connect_requests ADD COLUMN label TEXT;
ALTER TABLE slack_install_links ADD COLUMN label TEXT;
