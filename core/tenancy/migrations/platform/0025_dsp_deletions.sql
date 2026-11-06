-- Each DSP being deleted, from the moment its rows here are gone until what it kept elsewhere
-- is: its jobs, its folder and its objects. A delete a restart cut short is finished from here.
CREATE TABLE IF NOT EXISTS dsp_deletions (
    dsp_id TEXT PRIMARY KEY,
    started_at TEXT NOT NULL
);
