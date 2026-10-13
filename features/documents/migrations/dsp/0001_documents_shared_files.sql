-- What Documents keeps for a DSP. Shipped migrations never change: add the next one instead.

-- Each share Dispatch made of a file someone else owns that Documents holds a shortcut to: the
-- file, the teammate's address it's shared with, and Drive's ID for that share. Dispatch takes
-- back only the shares it made: when a teammate leaves, or the shortcut leaves Documents.
CREATE TABLE IF NOT EXISTS documents_shared_files (
    file_id TEXT NOT NULL,
    email TEXT NOT NULL,
    share_id TEXT NOT NULL,
    shared_at TEXT NOT NULL,
    PRIMARY KEY (file_id, email)
);
