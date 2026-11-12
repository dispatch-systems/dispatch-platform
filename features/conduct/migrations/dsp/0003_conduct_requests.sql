-- Receipts for retrying a photo upload or signature after an uncertain response.
CREATE TABLE IF NOT EXISTS conduct_requests (
    id TEXT PRIMARY KEY,
    actor_id TEXT NOT NULL,
    writeup_id TEXT NOT NULL REFERENCES conduct_writeups(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    expected_revision INTEGER NOT NULL,
    payload TEXT NOT NULL,
    result_revision INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS conduct_requests_writeup ON conduct_requests(writeup_id);
