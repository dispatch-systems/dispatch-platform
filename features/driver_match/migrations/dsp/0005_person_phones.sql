-- person_phones: the phone numbers someone at the DSP added to a person, beside those their
-- sources have, as a driver's new or second phone: they tell who texts from a number, and join
-- no one to anyone. Each number, as Communications writes it, is added to one person at most;
-- a merge carries it to whoever keeps the IDs.
CREATE TABLE IF NOT EXISTS person_phones (
    phone TEXT PRIMARY KEY,
    code TEXT NOT NULL REFERENCES people(code),
    added_at TEXT NOT NULL,
    actor_id TEXT
);
CREATE INDEX IF NOT EXISTS person_phones_code ON person_phones(code);
