CREATE TABLE IF NOT EXISTS storage_identity (dsp_id TEXT NOT NULL, provider TEXT NOT NULL, source TEXT NOT NULL);
-- One source object, without its temporary download signature.
CREATE TABLE IF NOT EXISTS dvic_reports (
    id TEXT PRIMARY KEY, company_id TEXT NOT NULL, dsp_code TEXT NOT NULL, station TEXT NOT NULL,
    source_key TEXT NOT NULL, name TEXT NOT NULL, week TEXT NOT NULL, report_date TEXT NOT NULL,
    modified_at INTEGER NOT NULL, etag TEXT, sha256 TEXT NOT NULL, revision_id TEXT NOT NULL,
    row_count INTEGER NOT NULL, short_count INTEGER NOT NULL, min_date TEXT, max_date TEXT,
    checked_at TEXT NOT NULL, UNIQUE(company_id,station,source_key)
);
CREATE INDEX IF NOT EXISTS dvic_reports_week ON dvic_reports(station,week,report_date);
-- Retain normalized source rows for every distinct workbook version, including
-- explicit corrections that meet the minimum; signed URLs are never stored.
CREATE TABLE IF NOT EXISTS dvic_revisions (
    id TEXT PRIMARY KEY, report_id TEXT NOT NULL REFERENCES dvic_reports(id), sha256 TEXT NOT NULL,
    modified_at INTEGER NOT NULL, collected_at TEXT NOT NULL, rows TEXT NOT NULL CHECK(json_valid(rows)),
    UNIQUE(report_id,sha256)
);
-- Canonical observations. A correction can meet the minimum; keep that observation
-- so an older backfill cannot resurrect the short record. Reads select short=1.
CREATE TABLE IF NOT EXISTS dvic_inspections (
    company_id TEXT NOT NULL, inspection_key TEXT NOT NULL, dsp_code TEXT NOT NULL, station TEXT NOT NULL,
    start_date TEXT NOT NULL, transporter_id TEXT NOT NULL, transporter_name TEXT NOT NULL,
    vin TEXT NOT NULL, fleet_type TEXT NOT NULL, inspection_type TEXT NOT NULL, inspection_status TEXT NOT NULL,
    start_time TEXT NOT NULL, end_time TEXT NOT NULL, duration_seconds REAL NOT NULL CHECK(duration_seconds>=0),
    minimum_seconds INTEGER NOT NULL CHECK(minimum_seconds IN (90,300)), short INTEGER NOT NULL CHECK(short IN (0,1)),
    report_date TEXT NOT NULL, source_modified_at INTEGER NOT NULL,
    revision_id TEXT NOT NULL REFERENCES dvic_revisions(id),
    PRIMARY KEY(company_id,inspection_key)
);
CREATE INDEX IF NOT EXISTS dvic_inspections_day ON dvic_inspections(station,start_date,inspection_key) WHERE short=1;
CREATE INDEX IF NOT EXISTS dvic_inspections_driver ON dvic_inspections(transporter_id,start_date) WHERE short=1;
CREATE TABLE IF NOT EXISTS dvic_weeks (
    station TEXT NOT NULL, company_id TEXT NOT NULL, week TEXT NOT NULL,
    checked_at TEXT NOT NULL, report_count INTEGER NOT NULL, PRIMARY KEY(station,company_id,week)
);
CREATE TABLE IF NOT EXISTS dvic_runs (
    job_id TEXT PRIMARY KEY, station TEXT NOT NULL, company_id TEXT NOT NULL, started_at TEXT NOT NULL,
    collected_at TEXT NOT NULL, weeks TEXT NOT NULL CHECK(json_valid(weeks)),
    reports INTEGER NOT NULL, downloaded INTEGER NOT NULL, unchanged INTEGER NOT NULL, rows INTEGER NOT NULL
);
