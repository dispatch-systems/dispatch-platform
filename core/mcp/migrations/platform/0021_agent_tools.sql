-- What a key or app may use. agent_keys gained, in code before this runs, all_tools: whether
-- a tool added since its tools were last chosen is allowed when it only reads. One that
-- changes something always waits until the platform owner switches it on.

-- agent_key_tools: each tool a key or app may or may not use, as the platform owner last
-- chose it. A tool with no row, one added since, follows all_tools. Keys and apps from
-- before have none, and use every tool that reads.
CREATE TABLE IF NOT EXISTS agent_key_tools (
    key_id TEXT NOT NULL REFERENCES agent_keys(id) ON DELETE CASCADE,
    tool TEXT NOT NULL,
    allowed INTEGER NOT NULL CHECK(allowed IN (0,1)),
    PRIMARY KEY(key_id, tool)
);
