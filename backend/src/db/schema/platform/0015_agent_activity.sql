-- agent_activity: each call an agent made with a key or as a connected app, for the Agents
-- page's Activity log, kept 90 days. at is when it started, in milliseconds since the epoch.
-- surface is the endpoint (rest:<endpoint>) or tool (mcp:<tool>); outcome is 'ok' or the code
-- the call was refused or failed with; ms is how long it took and bytes how much it answered.
-- The key's and the DSP's names are kept as they were, for a key or DSP gone since. Nothing
-- the agent asked beyond the endpoint or tool is kept, and never a token. Calls are written
-- from memory in batches, never one at a time. At most 10,000 of a key's calls are kept a
-- UTC day; then one row, surface 'activity:capped' and outcome 'capped', marks the day.
CREATE TABLE IF NOT EXISTS agent_activity (
    id INTEGER PRIMARY KEY,
    at INTEGER NOT NULL,
    key_id TEXT NOT NULL,
    key_name TEXT NOT NULL,
    key_kind TEXT NOT NULL CHECK(key_kind IN ('key','app')),
    surface TEXT NOT NULL,
    dsp_id TEXT,
    dsp_name TEXT,
    outcome TEXT NOT NULL,
    ms INTEGER NOT NULL,
    bytes INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS agent_activity_time ON agent_activity(at);
CREATE INDEX IF NOT EXISTS agent_activity_key ON agent_activity(key_id, at);
-- The refused calls alone, which the log can be narrowed to.
CREATE INDEX IF NOT EXISTS agent_activity_refused ON agent_activity(at) WHERE outcome<>'ok';
