-- person_titles: the title someone gave a person, as Dispatch or Owner, which no source says:
-- it makes them management, whatever their sources show. One for each person who has one; a
-- merge carries it to whoever keeps the IDs, unless they have one of their own.
CREATE TABLE IF NOT EXISTS person_titles (
    code TEXT PRIMARY KEY REFERENCES people(code),
    title TEXT NOT NULL,
    decided_at TEXT NOT NULL,
    actor_id TEXT
);
