-- jobs.kind named every job kind in a CHECK, so each new kind took a rebuild. Enqueueing
-- now refuses a kind no registered collector collects, so the table is rebuilt once more,
-- as 0004 did for DVIC, with the same columns, defaults, constraints and indexes and no
-- kind list. job_metrics references jobs and would lose its rows to ON DELETE CASCADE when
-- jobs is dropped, so it is rebuilt first, against the new table, and the old tables go
-- child before parent. The renames make job_metrics reference jobs again. The previous
-- release reads the table unchanged and only ever queues the kinds it knows.
CREATE TABLE jobs_v5 (id TEXT PRIMARY KEY, dsp_id TEXT NOT NULL, environment TEXT NOT NULL, kind TEXT NOT NULL, status TEXT NOT NULL CHECK(status IN ('queued','running','waiting_verification','succeeded','failed','cancelled')), progress INTEGER NOT NULL DEFAULT 0, message TEXT NOT NULL DEFAULT 'Waiting for a worker', attempt INTEGER NOT NULL DEFAULT 0, max_attempts INTEGER NOT NULL DEFAULT 3, available_at INTEGER NOT NULL, created_at TEXT NOT NULL, started_at TEXT, completed_at TEXT, error TEXT, release TEXT NOT NULL, actor_id TEXT, lease_owner TEXT, lease_until INTEGER, connection_revision INTEGER NOT NULL, idempotency_key TEXT NOT NULL, request TEXT NOT NULL DEFAULT '{}', UNIQUE(dsp_id,idempotency_key));
INSERT INTO jobs_v5 (id,dsp_id,environment,kind,status,progress,message,attempt,max_attempts,available_at,created_at,started_at,completed_at,error,release,actor_id,lease_owner,lease_until,connection_revision,idempotency_key,request)
  SELECT id,dsp_id,environment,kind,status,progress,message,attempt,max_attempts,available_at,created_at,started_at,completed_at,error,release,actor_id,lease_owner,lease_until,connection_revision,idempotency_key,request FROM jobs;
CREATE TABLE job_metrics_v5 (
    job_id TEXT NOT NULL REFERENCES jobs_v5(id) ON DELETE CASCADE,
    attempt INTEGER NOT NULL CHECK(attempt > 0),
    owner TEXT NOT NULL,
    metrics TEXT NOT NULL CHECK(json_valid(metrics)),
    PRIMARY KEY(job_id, attempt)
);
INSERT INTO job_metrics_v5 (job_id,attempt,owner,metrics) SELECT job_id,attempt,owner,metrics FROM job_metrics;
DROP TABLE job_metrics;
DROP TABLE jobs;
ALTER TABLE jobs_v5 RENAME TO jobs;
ALTER TABLE job_metrics_v5 RENAME TO job_metrics;
CREATE INDEX IF NOT EXISTS jobs_claim ON jobs(status,available_at,created_at);
CREATE INDEX IF NOT EXISTS jobs_dsp ON jobs(dsp_id,created_at DESC);
CREATE UNIQUE INDEX IF NOT EXISTS one_active_dsp ON jobs(dsp_id) WHERE status IN ('running','waiting_verification');
CREATE INDEX IF NOT EXISTS jobs_created ON jobs(created_at DESC);
