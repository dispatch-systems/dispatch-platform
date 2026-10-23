-- person_roles: the role someone gave a person, whatever their sources say: a driver, a helper,
-- offboarded, or management under a title, by its id. One for each person who has one; a merge
-- carries it to whoever keeps the IDs, unless they have one of their own. It starts with every
-- title of person_titles, which the previous release still reads: that table stays, no longer
-- written, until a later release drops it.
CREATE TABLE IF NOT EXISTS person_roles (
    code TEXT PRIMARY KEY REFERENCES people(code),
    role TEXT NOT NULL,
    decided_at TEXT NOT NULL,
    actor_id TEXT
);
INSERT OR IGNORE INTO person_roles(code,role,decided_at,actor_id)
    SELECT code,title,decided_at,actor_id FROM person_titles;
