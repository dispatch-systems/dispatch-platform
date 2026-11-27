-- Each photo address Amazon last served no photo at, its placeholder or a refusal, and when
-- it said so. An offboarded associate's isn't asked for again until a month on; anyone else's
-- is asked for at every collection. An address goes once a photo is kept for it or the roster
-- no longer lists whoever it names.
CREATE TABLE IF NOT EXISTS unserved_photos (
    address TEXT PRIMARY KEY,
    checked_at TEXT NOT NULL
);
