-- What Documents keeps for a DSP. Shipped migrations never change: add the next one instead.

-- The Google account that holds the DSP's Documents, and the main folder Dispatch made in it.
-- At most one. Its refresh token sits encrypted in the DSP's secrets, never here.
CREATE TABLE IF NOT EXISTS documents_connection (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    status TEXT NOT NULL CHECK (status IN ('connected', 'broken')),
    account_email TEXT NOT NULL,
    account_kind TEXT NOT NULL CHECK (account_kind IN ('workspace', 'personal')),
    folder_id TEXT NOT NULL,
    folder_name TEXT NOT NULL,
    connected_by TEXT NOT NULL,
    connected_at TEXT NOT NULL,
    broken_at TEXT
);

-- A connection someone started and hasn't finished at Google yet: who started it, and the
-- PKCE verifier its code is exchanged with. Each is used once, within ten minutes.
CREATE TABLE IF NOT EXISTS documents_connect_requests (
    state_hash TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    verifier TEXT NOT NULL,
    expires_at INTEGER NOT NULL
);
