-- Copies backed up of photos since replaced, each by its ID where the DSP keeps its files:
-- taken back once its writeup's new photo is saved there, tried again after a failure.
CREATE TABLE IF NOT EXISTS conduct_backup_replaced (
    saved_id TEXT PRIMARY KEY,
    writeup_id TEXT NOT NULL REFERENCES conduct_writeups(id) ON DELETE CASCADE,
    attempts INTEGER NOT NULL DEFAULT 0,
    due_at INTEGER NOT NULL DEFAULT 0,
    reason TEXT
);
CREATE INDEX IF NOT EXISTS conduct_backup_replaced_writeup ON conduct_backup_replaced(writeup_id);
