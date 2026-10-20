-- Identifies the file as this DSP's vans storage, so another cannot pass for it.
CREATE TABLE IF NOT EXISTS storage_identity (dsp_id TEXT NOT NULL, provider TEXT NOT NULL, source TEXT NOT NULL);
-- Every vehicle Amazon's fleet has listed for the DSP's company, by VIN, as the newest
-- collection lists it. One the fleet no longer lists keeps its row, with listed 0, so a route
-- that names its VIN still finds its name. row is the row as Amazon sent it.
CREATE TABLE IF NOT EXISTS vans (
    vin TEXT PRIMARY KEY,
    company_id TEXT NOT NULL,
    name TEXT,
    listed INTEGER NOT NULL CHECK(listed IN (0,1)),
    first_listed TEXT NOT NULL,
    last_listed TEXT NOT NULL,
    row TEXT NOT NULL
);
-- Its own settings: whether the DSP's first schedule was made.
CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL CHECK(json_valid(value)));
-- Each collection that published, newest last.
CREATE TABLE IF NOT EXISTS fleet_runs (
    job_id TEXT PRIMARY KEY,
    company_id TEXT NOT NULL,
    collected_at TEXT NOT NULL,
    listed INTEGER NOT NULL
);
