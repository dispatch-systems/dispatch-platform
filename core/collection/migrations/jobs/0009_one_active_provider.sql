-- A worker ran one job per DSP at a time; now one per DSP and provider, so a DSP's Cortex
-- job runs while its Paycom job does. A job's provider is its kind up to the first '.',
-- which the registry holds every kind to. Dropping one_active_dsp breaks the additive-only
-- rule, with the owner's approval: the previous release never reads the index and keeps one
-- job per DSP running in its own code (its claim and its scheduler), so rolled back it
-- leases nothing this index refuses.
DROP INDEX IF EXISTS one_active_dsp;
CREATE UNIQUE INDEX IF NOT EXISTS one_active_provider ON jobs(dsp_id,substr(kind,1,instr(kind,'.')-1)) WHERE status IN ('running','waiting_verification');
