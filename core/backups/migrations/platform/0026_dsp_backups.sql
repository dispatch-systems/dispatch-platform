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
-- Each rule of a DSP's backup schedule, in its order: on days of the week (`days`, Monday the
-- lowest bit) or dates of the month (`dates`, the 1st bit 1, and its last day if `last_day`), at
-- set times or every so often between two times; how long what it takes is kept, forever, for a
-- while or as its newest few; and which of the DSP's data it leaves out, so what's added later is
-- backed up too.
CREATE TABLE IF NOT EXISTS backup_rules (
    id TEXT PRIMARY KEY,
    dsp_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    on_days TEXT NOT NULL CHECK(on_days IN ('week', 'month')),
    days INTEGER NOT NULL DEFAULT 0,
    dates INTEGER NOT NULL DEFAULT 0,
    last_day INTEGER NOT NULL DEFAULT 0,
    times TEXT,
    every INTEGER,
    from_time TEXT,
    to_time TEXT,
    keep TEXT NOT NULL CHECK(keep IN ('forever', 'for', 'last')),
    keep_count INTEGER,
    keep_unit TEXT CHECK(keep_unit IN ('day', 'week', 'month', 'year')),
    skipped TEXT NOT NULL DEFAULT '[]',
    next_run INTEGER
);
CREATE INDEX IF NOT EXISTS backup_rules_dsp ON backup_rules(dsp_id, position);
-- Each backup of a DSP, from when it's asked for until it expires: the rule that took it, or none
-- for one taken by hand, and what of the DSP it leaves out, as that rule said when it was asked; when it expires, none for one kept forever or as one of its rule's newest
-- `keep_last`; and once it's taken, the files it holds by their objects. Kept after its DSP is
-- deleted until it expires.
CREATE TABLE IF NOT EXISTS backups (
    id TEXT PRIMARY KEY,
    dsp_id TEXT NOT NULL,
    rule_id TEXT,
    requested_by TEXT,
    status TEXT NOT NULL CHECK(status IN ('queued', 'running', 'succeeded', 'failed')),
    created_at INTEGER NOT NULL,
    finished_at INTEGER,
    expires_at INTEGER,
    keep_last INTEGER,
    skipped TEXT NOT NULL DEFAULT '[]',
    size INTEGER,
    files TEXT,
    error TEXT
);
CREATE INDEX IF NOT EXISTS backups_dsp ON backups(dsp_id, created_at DESC);
CREATE INDEX IF NOT EXISTS backups_status ON backups(status);
