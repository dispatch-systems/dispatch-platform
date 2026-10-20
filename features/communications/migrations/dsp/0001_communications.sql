-- What Communications keeps for a DSP. Shipped migrations never change: add the next one instead.
-- TODO(scaffold): the columns it really keeps.
CREATE TABLE IF NOT EXISTS communications_items (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL
);
