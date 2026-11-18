-- Communications' migration 24, after its columns (`backend::texts::storage::add_lines`): the
-- names another line saved for numbers, the numbers it blocked and the browsers told of its new
-- messages, each kept by the line it's of (`line`, by its feature's name), beside Dispatch
-- Phone's own, which stay in `communications_contacts`, `communications_blocked` and
-- `communications_push` as they were, so the release before the lines still reads and writes its
-- own. Each step is taken only if it wasn't. Shipped migrations never change: add the next one
-- instead.

CREATE TABLE IF NOT EXISTS communications_line_contacts (
    line TEXT NOT NULL,
    number TEXT NOT NULL,
    name TEXT NOT NULL,
    saved_by TEXT NOT NULL,
    saved_at INTEGER NOT NULL,
    PRIMARY KEY (line, number)
);

CREATE TABLE IF NOT EXISTS communications_line_blocked (
    line TEXT NOT NULL,
    number TEXT NOT NULL,
    blocked_by TEXT NOT NULL,
    blocked_at INTEGER NOT NULL,
    PRIMARY KEY (line, number)
);

CREATE TABLE IF NOT EXISTS communications_line_push (
    endpoint TEXT NOT NULL,
    line TEXT NOT NULL,
    user_id TEXT NOT NULL,
    p256dh TEXT NOT NULL,
    auth TEXT NOT NULL,
    site TEXT NOT NULL CHECK (site IN ('admin', 'dsp')),
    created_at INTEGER NOT NULL,
    PRIMARY KEY (endpoint, line)
);
CREATE INDEX IF NOT EXISTS communications_line_push_users ON communications_line_push (user_id);
