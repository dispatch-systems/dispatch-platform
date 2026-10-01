-- Every Driver Match code in use, so no two people share one, even in different DSPs.
-- A code is never removed: a merged person's code still leads to whoever kept their IDs.
CREATE TABLE IF NOT EXISTS driver_codes (
    code TEXT PRIMARY KEY,
    dsp_id TEXT NOT NULL,
    created_at TEXT NOT NULL
);
