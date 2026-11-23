-- Identifies the file as this DSP's shifts storage, so another cannot pass for it.
CREATE TABLE IF NOT EXISTS storage_identity (dsp_id TEXT NOT NULL, provider TEXT NOT NULL, source TEXT NOT NULL);
-- Each collection that published, newest last: the station's week it read, and how many
-- assignments it found added, changed and removed since the last read of that week.
CREATE TABLE IF NOT EXISTS shift_runs (
    job_id TEXT PRIMARY KEY,
    station TEXT NOT NULL,
    service_area_id TEXT NOT NULL,
    week_start TEXT NOT NULL,
    week_end TEXT NOT NULL,
    collected_at TEXT NOT NULL,
    drivers INTEGER NOT NULL,
    assignments INTEGER NOT NULL,
    added INTEGER NOT NULL,
    changed INTEGER NOT NULL,
    removed INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS shift_runs_by_week ON shift_runs(service_area_id, week_start, collected_at);
-- Every shift assignment Amazon's Scheduling has listed, by its own id, as the newest
-- collection of its week had it: a driver (by transporter ID) on a shift, a backup for one,
-- or time off (kind PRIMARY, BACKUP or TIMEOFF), on a day. Times are minutes after the
-- day's midnight. One a later collection of its week no longer lists keeps its row, with
-- listed 0. row is the assignment as Amazon sent it.
CREATE TABLE IF NOT EXISTS shift_assignments (
    mapping_id TEXT PRIMARY KEY,
    service_area_id TEXT NOT NULL,
    date TEXT NOT NULL,
    transporter_id TEXT NOT NULL,
    driver_name TEXT,
    employee_id TEXT,
    kind TEXT NOT NULL,
    shift_id TEXT NOT NULL,
    shift_name TEXT NOT NULL,
    shift_type TEXT NOT NULL,
    position TEXT,
    start_min INTEGER NOT NULL,
    start_max INTEGER NOT NULL,
    duration_min INTEGER NOT NULL,
    duration_max INTEGER NOT NULL,
    qualification TEXT,
    reason TEXT,
    status TEXT NOT NULL,
    state TEXT,
    comment TEXT,
    created_at INTEGER,
    listed INTEGER NOT NULL CHECK(listed IN (0,1)),
    first_seen TEXT NOT NULL,
    last_seen TEXT NOT NULL,
    row TEXT NOT NULL CHECK(json_valid(row))
);
CREATE INDEX IF NOT EXISTS shift_assignments_by_day ON shift_assignments(service_area_id, date);
CREATE INDEX IF NOT EXISTS shift_assignments_by_driver ON shift_assignments(transporter_id, date);
-- Each change a collection found, oldest first: an assignment added, changed or removed,
-- with what it was before and after, as the fields shift_assignments compares.
CREATE TABLE IF NOT EXISTS shift_changes (
    id INTEGER PRIMARY KEY,
    job_id TEXT NOT NULL,
    mapping_id TEXT NOT NULL,
    date TEXT NOT NULL,
    transporter_id TEXT NOT NULL,
    change TEXT NOT NULL CHECK(change IN ('added','changed','removed')),
    before TEXT CHECK(before IS NULL OR json_valid(before)),
    after TEXT CHECK(after IS NULL OR json_valid(after)),
    seen_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS shift_changes_by_assignment ON shift_changes(mapping_id, id);
CREATE INDEX IF NOT EXISTS shift_changes_by_day ON shift_changes(date);
