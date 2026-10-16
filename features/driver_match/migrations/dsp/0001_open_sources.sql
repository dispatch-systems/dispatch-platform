-- person_ids: Driver Match keeps the IDs of whatever sources the features declare, and joins
-- an ID to its person by a contact as well as by a name ('email', 'phone'). Its source and
-- how an ID came to its person are no longer a fixed list; the rows stay as they were.
CREATE TABLE person_ids_open (
    source TEXT NOT NULL,
    external_id TEXT NOT NULL,
    code TEXT NOT NULL REFERENCES people(code),
    linked_by TEXT NOT NULL,
    linked_at TEXT NOT NULL,
    actor_id TEXT,
    PRIMARY KEY(source, external_id)
);
INSERT INTO person_ids_open(source,external_id,code,linked_by,linked_at,actor_id)
    SELECT source,external_id,code,linked_by,linked_at,actor_id FROM person_ids;
DROP TABLE person_ids;
ALTER TABLE person_ids_open RENAME TO person_ids;
CREATE INDEX person_ids_code ON person_ids(code);
