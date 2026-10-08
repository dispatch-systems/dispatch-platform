-- What Documents keeps for a DSP. Shipped migrations never change: add the next one instead.

-- Who added each file through Dispatch, and who last changed it there. Google names the
-- connected account for both, since Dispatch makes every file with it.
CREATE TABLE IF NOT EXISTS documents_files (
    file_id TEXT PRIMARY KEY,
    added_by TEXT NOT NULL,
    added_at TEXT NOT NULL,
    changed_by TEXT NOT NULL,
    changed_at TEXT NOT NULL
);
