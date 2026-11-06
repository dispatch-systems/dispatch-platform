-- Each DSP being deleted, from the moment its rows here are gone until what it kept elsewhere
-- is: its jobs, its folder and its objects. A delete a restart cut short is finished from here.
-- `mail` is the IDs of the messages it deleted, as a JSON array: development named what it
-- captured of them by ID alone before captures named their DSP.
CREATE TABLE IF NOT EXISTS dsp_deletions (
    dsp_id TEXT PRIMARY KEY,
    started_at TEXT NOT NULL,
    mail TEXT NOT NULL DEFAULT '[]'
);
