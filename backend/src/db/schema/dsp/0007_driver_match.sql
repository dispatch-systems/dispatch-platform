-- Driver Match: one code per person, and every ID a source knows them by. Collected rows
-- keep the IDs their source gave them; an ID leads to its person here.
--
-- people: a merged person's code leads to the person who kept their IDs (merged_into),
-- so a code stored anywhere still finds them. split_from names the person an ID was split
-- off from, when this person began that way.
CREATE TABLE IF NOT EXISTS people (
    code TEXT PRIMARY KEY,
    created_at TEXT NOT NULL,
    merged_into TEXT REFERENCES people(code),
    merged_at TEXT,
    merged_by TEXT,
    split_from TEXT REFERENCES people(code),
    created_by TEXT
);
-- person_ids: one row per source ID, so an ID can never belong to two people; a person
-- may hold several of one source, after a rehire or a merge. Only the ID is kept: names,
-- departments and dates stay in the databases that collected them. linked_by says how the
-- ID came to its person: as its first ID ('new'), by a unique name ('name') or name
-- variant ('variant'), by a link saved on the meal-break page ('saved'), or by someone's
-- decision ('person').
CREATE TABLE IF NOT EXISTS person_ids (
    source TEXT NOT NULL CHECK(source IN ('paycom','amazon')),
    external_id TEXT NOT NULL,
    code TEXT NOT NULL REFERENCES people(code),
    linked_by TEXT NOT NULL CHECK(linked_by IN ('new','name','variant','saved','person')),
    linked_at TEXT NOT NULL,
    actor_id TEXT,
    PRIMARY KEY(source, external_id)
);
CREATE INDEX IF NOT EXISTS person_ids_code ON person_ids(code);
-- people_apart: two people someone decided are different, never suggested as one again.
CREATE TABLE IF NOT EXISTS people_apart (
    first TEXT NOT NULL REFERENCES people(code),
    second TEXT NOT NULL REFERENCES people(code),
    decided_at TEXT NOT NULL,
    actor_id TEXT,
    PRIMARY KEY(first, second),
    CHECK(first < second)
);
