-- What the server keeps of each DSP's live reading: each read's outcome and time, with counts
-- and codes only, and the days each reader started and stopped there. New tables only: the
-- previous release never reads them.
CREATE TABLE IF NOT EXISTS live_reads (
    reader TEXT NOT NULL,
    dsp_id TEXT NOT NULL,
    day TEXT NOT NULL,
    at INTEGER NOT NULL,
    ms INTEGER NOT NULL CHECK(ms >= 0),
    outcome TEXT NOT NULL CHECK(outcome IN ('kept','failed')),
    code TEXT,
    counts TEXT CHECK(counts IS NULL OR json_valid(counts)),
    CHECK((outcome = 'kept') = (code IS NULL))
);
CREATE INDEX IF NOT EXISTS live_reads_dsp ON live_reads(reader,dsp_id,at);
CREATE TABLE IF NOT EXISTS live_days (
    reader TEXT NOT NULL,
    dsp_id TEXT NOT NULL,
    day TEXT NOT NULL,
    started_at INTEGER NOT NULL,
    stopped_at INTEGER,
    PRIMARY KEY(reader,dsp_id,day)
);
