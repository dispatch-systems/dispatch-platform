-- What Communications keeps of a DSP's dispatch phones: the phones, the codes that pair them, and
-- every text and picture they relay, until the DSP is deleted. Phones made these as its own
-- migration 1, before Communications adopted them: on a database it migrated, each is found made
-- already. Shipped migrations never change: add the next one instead.

-- Each phone paired to the DSP, by the SHA-256 of the key it was given (`key_sha256`), and under
-- the name someone at the DSP gives it. A phone removed (`removed_at`) keeps its texts; its key
-- works no more. A stand-in (`stand_in`) is the demo's phone, which no app holds: it says each
-- text Dispatch sends from it delivered at once.
CREATE TABLE IF NOT EXISTS phones_devices (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    number TEXT NOT NULL,
    model TEXT NOT NULL,
    app_version TEXT NOT NULL,
    key_sha256 TEXT NOT NULL UNIQUE,
    paired_at INTEGER NOT NULL,
    paired_by TEXT NOT NULL,
    last_seen_at INTEGER,
    removed_at INTEGER,
    stand_in INTEGER NOT NULL DEFAULT 0 CHECK (stand_in IN (0, 1))
);

-- The code that pairs the next phone, by its SHA-256: one at a time, until it expires or pairs.
CREATE TABLE IF NOT EXISTS phones_pairings (
    code_sha256 TEXT PRIMARY KEY,
    created_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    used_at INTEGER,
    device_id TEXT REFERENCES phones_devices (id)
);

-- Each text: one the phone reports (`origin` phone: received, or sent from the phone's own
-- messaging app), by the phone's own ID of it (`client_id`), or one sent from Dispatch (`origin`
-- dispatch), by whom (`sent_by`), waiting for the phone until it says how it went (`status`), of
-- its latest attempt (`attempt`, the change that queued it or tried it again): a report of an
-- earlier one that it was sent or failed is left alone.
-- `number` is the other party's. `change` orders every change to a text, and to its files, for
-- those reading them.
CREATE TABLE IF NOT EXISTS phones_texts (
    id TEXT PRIMARY KEY,
    device_id TEXT NOT NULL REFERENCES phones_devices (id),
    client_id TEXT,
    direction TEXT NOT NULL CHECK (direction IN ('in', 'out')),
    origin TEXT NOT NULL CHECK (origin IN ('phone', 'dispatch')),
    number TEXT NOT NULL,
    body TEXT NOT NULL,
    at INTEGER NOT NULL,
    sent_by TEXT,
    status TEXT CHECK (status IN ('waiting', 'sent', 'delivered', 'failed')),
    status_at INTEGER,
    error TEXT,
    attempt INTEGER,
    change INTEGER NOT NULL,
    UNIQUE (device_id, client_id)
);
CREATE INDEX IF NOT EXISTS phones_texts_change ON phones_texts (change);
CREATE INDEX IF NOT EXISTS phones_texts_waiting ON phones_texts (device_id, status);
CREATE INDEX IF NOT EXISTS phones_texts_number ON phones_texts (number, at);

-- Each picture or file of a text, its bytes kept by their SHA-256 in the DSP's data
-- (`data/phones/files/`). One a phone reports waits for its bytes (`sha256` null); one added in
-- Dispatch before its text is sent belongs to its adder alone (`text_id` null).
CREATE TABLE IF NOT EXISTS phones_files (
    id TEXT PRIMARY KEY,
    text_id TEXT REFERENCES phones_texts (id),
    added_by TEXT,
    name TEXT NOT NULL,
    mime TEXT NOT NULL,
    size INTEGER NOT NULL,
    sha256 TEXT,
    added_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS phones_files_text ON phones_files (text_id);

-- The last change given, so each is after every one before it.
CREATE TABLE IF NOT EXISTS phones_change (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    last INTEGER NOT NULL
);
INSERT OR IGNORE INTO phones_change (id, last) VALUES (1, 0);
