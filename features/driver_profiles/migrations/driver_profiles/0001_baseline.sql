-- Identifies the file as this DSP's driver_profiles storage, so another cannot pass for it.
CREATE TABLE IF NOT EXISTS storage_identity (dsp_id TEXT NOT NULL, provider TEXT NOT NULL, source TEXT NOT NULL);
-- Every associate Amazon has listed for the DSP's company, as the newest roster lists them.
-- Someone the roster no longer lists keeps their row, with listed 0, so what names them
-- elsewhere still finds a name; their photo goes. roles and qualifications are JSON lists;
-- row is the row as Amazon sent it.
CREATE TABLE IF NOT EXISTS associates (
    transporter_id TEXT PRIMARY KEY,
    company_id TEXT NOT NULL,
    name TEXT NOT NULL,
    email TEXT,
    personal_phone TEXT,
    work_phone TEXT,
    status TEXT NOT NULL,
    roles TEXT NOT NULL,
    qualifications TEXT NOT NULL,
    license_expires TEXT,
    photo_address TEXT,
    listed INTEGER NOT NULL CHECK(listed IN (0,1)),
    first_listed TEXT NOT NULL,
    last_listed TEXT NOT NULL,
    row TEXT NOT NULL
);
-- The photos kept, each a file in the DSP's cache named by its digest. The address the
-- roster gave a photo names it from one collection to the next.
CREATE TABLE IF NOT EXISTS photos (
    address TEXT PRIMARY KEY,
    sha256 TEXT NOT NULL,
    format TEXT NOT NULL,
    bytes INTEGER NOT NULL,
    stored_at TEXT NOT NULL
);
-- Its own settings: whether the DSP's first schedule was made, which 0002_drop_settings drops.
CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL CHECK(json_valid(value)));
-- Each collection that published, newest last.
CREATE TABLE IF NOT EXISTS roster_runs (
    job_id TEXT PRIMARY KEY,
    company_id TEXT NOT NULL,
    collected_at TEXT NOT NULL,
    listed INTEGER NOT NULL,
    photos INTEGER NOT NULL,
    photos_waiting INTEGER NOT NULL
);
