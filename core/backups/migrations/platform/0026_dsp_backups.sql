-- Each DSP's backup schedule: whether its rules run, and the generation its backups' files are
-- named for, new with each platform database, so backups taken from another never share a file
-- with these. Kept here, beside the DSP's folder, so a restore of that folder neither forgets the
-- backups taken since nor changes when they run.
CREATE TABLE IF NOT EXISTS backup_schedules (
    dsp_id TEXT PRIMARY KEY,
    enabled INTEGER NOT NULL DEFAULT 1,
    revision INTEGER NOT NULL DEFAULT 0,
    generation TEXT NOT NULL
);
-- Each rule of a DSP's backup schedule, in its order: on which days (Monday the lowest bit) and
-- when on each, as a collection's rules are, how long what it takes is kept, and which of the
-- DSP's data it leaves out, so what's added later is backed up too.
CREATE TABLE IF NOT EXISTS backup_rules (
    id TEXT PRIMARY KEY,
    dsp_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    days INTEGER NOT NULL,
    times TEXT,
    every INTEGER,
    from_time TEXT,
    to_time TEXT,
    keep_count INTEGER NOT NULL,
    keep_unit TEXT NOT NULL CHECK(keep_unit IN ('day', 'week', 'month', 'year')),
    skipped TEXT NOT NULL DEFAULT '[]',
    next_run INTEGER
);
CREATE INDEX IF NOT EXISTS backup_rules_dsp ON backup_rules(dsp_id, position);
-- Each backup of a DSP, from when it's asked for until it expires: the rule that took it, or none
-- for one taken by hand, and once it's taken, the files it holds by their objects. Kept after its
-- DSP is deleted until it expires.
CREATE TABLE IF NOT EXISTS backups (
    id TEXT PRIMARY KEY,
    dsp_id TEXT NOT NULL,
    rule_id TEXT,
    requested_by TEXT,
    status TEXT NOT NULL CHECK(status IN ('queued', 'running', 'succeeded', 'failed')),
    created_at INTEGER NOT NULL,
    finished_at INTEGER,
    expires_at INTEGER,
    size INTEGER,
    files TEXT,
    error TEXT
);
CREATE INDEX IF NOT EXISTS backups_dsp ON backups(dsp_id, created_at DESC);
CREATE INDEX IF NOT EXISTS backups_status ON backups(status);
