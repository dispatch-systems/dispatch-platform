-- Communications' migration 24, after its columns (`backend::texts::storage::add_lines`): the
-- names saved for numbers, the numbers blocked and the browsers told of new messages, each kept
-- by the line it's of (`line`, by its feature's name; none for Dispatch Phone's own), as every
-- one kept before is. Run only on tables without the line. Shipped migrations never change: add
-- the next one instead.

CREATE TABLE communications_contacts_by_line (
    line TEXT,
    number TEXT NOT NULL,
    name TEXT NOT NULL,
    saved_by TEXT NOT NULL,
    saved_at INTEGER NOT NULL
);
INSERT INTO communications_contacts_by_line (line, number, name, saved_by, saved_at)
    SELECT NULL, number, name, saved_by, saved_at FROM communications_contacts;
DROP TABLE communications_contacts;
ALTER TABLE communications_contacts_by_line RENAME TO communications_contacts;
CREATE UNIQUE INDEX communications_contacts_numbers
    ON communications_contacts (number, ifnull(line, ''));

CREATE TABLE communications_blocked_by_line (
    line TEXT,
    number TEXT NOT NULL,
    blocked_by TEXT NOT NULL,
    blocked_at INTEGER NOT NULL
);
INSERT INTO communications_blocked_by_line (line, number, blocked_by, blocked_at)
    SELECT NULL, number, blocked_by, blocked_at FROM communications_blocked;
DROP TABLE communications_blocked;
ALTER TABLE communications_blocked_by_line RENAME TO communications_blocked;
CREATE UNIQUE INDEX communications_blocked_numbers
    ON communications_blocked (number, ifnull(line, ''));

CREATE TABLE communications_push_by_line (
    endpoint TEXT NOT NULL,
    line TEXT,
    user_id TEXT NOT NULL,
    p256dh TEXT NOT NULL,
    auth TEXT NOT NULL,
    site TEXT NOT NULL CHECK (site IN ('admin', 'dsp')),
    created_at INTEGER NOT NULL
);
INSERT INTO communications_push_by_line (endpoint, line, user_id, p256dh, auth, site, created_at)
    SELECT endpoint, NULL, user_id, p256dh, auth, site, created_at FROM communications_push;
DROP TABLE communications_push;
ALTER TABLE communications_push_by_line RENAME TO communications_push;
CREATE UNIQUE INDEX communications_push_endpoints
    ON communications_push (endpoint, ifnull(line, ''));
CREATE INDEX communications_push_users ON communications_push (user_id);
