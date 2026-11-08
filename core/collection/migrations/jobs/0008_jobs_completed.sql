-- The scheduler ranks queued DSPs by their latest completed collection. The prefix
-- locates one DSP and the last entry answers MAX without scanning its job history.
CREATE INDEX IF NOT EXISTS jobs_dsp_completed ON jobs(dsp_id,completed_at);
