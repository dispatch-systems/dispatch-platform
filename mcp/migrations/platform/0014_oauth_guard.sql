-- Who may connect through Sign in with Dispatch, and when. Times are RFC 3339, as on the
-- other oauth tables.

-- oauth_pairing: the one window in which apps may ask to connect, opened by a platform owner
-- for ten minutes at a time. Closed once open_until has passed; there is at most one row.
CREATE TABLE IF NOT EXISTS oauth_pairing (
    id INTEGER PRIMARY KEY CHECK(id=1),
    open_until TEXT NOT NULL,
    opened_by TEXT NOT NULL,
    opened_at TEXT NOT NULL
);
-- oauth_apps: the platform owner's choice for each kind of app that may connect: a known app
-- (chatgpt, codex, claude-code, hermes), apps on the owner's own computer (local) or websites
-- and other apps (web). A kind without a row keeps its default, which the code holds.
CREATE TABLE IF NOT EXISTS oauth_apps (
    id TEXT PRIMARY KEY,
    allowed INTEGER NOT NULL CHECK(allowed IN (0,1)),
    changed_by TEXT NOT NULL,
    changed_at TEXT NOT NULL
);
