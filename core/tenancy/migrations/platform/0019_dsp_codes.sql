-- Each DSP's short code: its address, in lowercase. One DSP has a code until it is set up.
ALTER TABLE dsps ADD COLUMN code TEXT;
CREATE UNIQUE INDEX IF NOT EXISTS dsps_code ON dsps(code);
