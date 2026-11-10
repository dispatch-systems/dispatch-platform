-- What Conduct keeps of backing a DSP's signed copies up where it keeps its files. Shipped
-- migrations never change: add the next one instead.

-- A signed writeup's photo as it's backed up: where it was saved, once it is, or how its tries
-- went until then, the next one due at `due_at`, in milliseconds. A photo without a row hasn't
-- been tried.
CREATE TABLE IF NOT EXISTS conduct_backups (
    writeup_id TEXT PRIMARY KEY REFERENCES conduct_writeups(id),
    saved_id TEXT,
    saved_at INTEGER,
    attempts INTEGER NOT NULL DEFAULT 0,
    due_at INTEGER NOT NULL DEFAULT 0,
    reason TEXT
);

-- The folders the photos go in, each by its ID where the DSP keeps its files: the one that
-- holds everyone's (no person, no category), each person's inside it (no category), and each
-- category's inside theirs.
CREATE TABLE IF NOT EXISTS conduct_backup_folders (
    person TEXT NOT NULL,
    category_id TEXT NOT NULL,
    folder_id TEXT NOT NULL,
    PRIMARY KEY (person, category_id)
);
