-- Texting: what the DSP's dispatch phones relay through `texting`, each text filed in a
-- conversation of its own: a work phone's under the driver who held it as it was written, across
-- days; any other number's under that number. Kept, as the rest, until the DSP is deleted.
-- Shipped migrations never change: add the next one instead.

-- A conversation by text: a driver's (`work`, by their ID of the source that names who held the
-- DSP's work phones, `source` and `driver_id`), for every work phone they held; a work phone's
-- nobody held as its texts were written (`work`, by its `number` alone); or any other number's
-- (`number`). Whether it waits follows from its texts and when someone at the DSP marked it done
-- (`done_at`), as a Slack conversation's does, and is kept beside them: `state`, since when it
-- waits, and when its latest text was written (`last_at`).
CREATE TABLE IF NOT EXISTS communications_text_conversations (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('work', 'number')),
    source TEXT,
    driver_id TEXT,
    number TEXT,
    state TEXT NOT NULL DEFAULT 'responded' CHECK (state IN ('waiting', 'responded', 'done')),
    waiting_since INTEGER,
    last_at INTEGER,
    done_at INTEGER,
    done_by TEXT
);
CREATE UNIQUE INDEX IF NOT EXISTS communications_text_conversations_drivers
    ON communications_text_conversations (source, driver_id) WHERE driver_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS communications_text_conversations_numbers
    ON communications_text_conversations (kind, number) WHERE driver_id IS NULL;
CREATE INDEX IF NOT EXISTS communications_text_conversations_latest
    ON communications_text_conversations (kind, last_at, id);

-- Each text, by Phones' ID of it, in the conversation it's filed in: one the other party wrote
-- (`in`) or one written to them (`out`), through the DSP's phone `phone`, with their `number`.
-- One Dispatch sent names who sent it (`sent_by`) and the name they're shown by, and how it went
-- (`status`, with `error` once it failed); one sent from the phone itself has neither. Its
-- pictures and files, as Phones has them (`files`, JSON). One that came from a number while it
-- was blocked (`blocked`) never makes its conversation wait.
CREATE TABLE IF NOT EXISTS communications_texts (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES communications_text_conversations (id),
    phone TEXT NOT NULL,
    direction TEXT NOT NULL CHECK (direction IN ('in', 'out')),
    number TEXT NOT NULL,
    body TEXT NOT NULL,
    at INTEGER NOT NULL,
    sent_by TEXT,
    sender_name TEXT,
    status TEXT CHECK (status IN ('waiting', 'sent', 'delivered', 'failed')),
    error TEXT,
    files TEXT NOT NULL DEFAULT '[]',
    blocked INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS communications_texts_latest
    ON communications_texts (conversation_id, at, id);
CREATE INDEX IF NOT EXISTS communications_texts_written ON communications_texts (at);

-- How far the DSP's texts were read: the last of Phones' changes filed.
CREATE TABLE IF NOT EXISTS communications_texting (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    read_to INTEGER NOT NULL
);

-- The names someone at the DSP saved for numbers that text it, shared by everyone at the DSP.
CREATE TABLE IF NOT EXISTS communications_contacts (
    number TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    saved_by TEXT NOT NULL,
    saved_at INTEGER NOT NULL
);

-- The numbers someone at the DSP blocked: their texts are still kept, but their conversation
-- isn't listed or counted, and nothing they send while blocked makes it wait.
CREATE TABLE IF NOT EXISTS communications_blocked (
    number TEXT PRIMARY KEY,
    blocked_by TEXT NOT NULL,
    blocked_at INTEGER NOT NULL
);
