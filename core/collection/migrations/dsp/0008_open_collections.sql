-- collection_schedules.collection named every collection in a CHECK, so each new
-- collection took a rebuild. Saving a schedule already refuses a collection no registered
-- collector schedules, so the table is rebuilt once more, as 0006 did for DVIC, with the
-- same columns, defaults and constraints and no collection list. Nothing references it
-- and it has no index. The previous release reads the table unchanged and only ever saves
-- the collections it knows.
CREATE TABLE collection_schedules_v5 (
    id TEXT PRIMARY KEY, name TEXT NOT NULL, collection TEXT NOT NULL,
    cadence TEXT NOT NULL CHECK(cadence IN ('interval','daily')), interval_minutes INTEGER, local_time TEXT NOT NULL,
    anchor INTEGER NOT NULL, enabled INTEGER NOT NULL CHECK(enabled IN (0,1)), next_run TEXT,
    revision INTEGER NOT NULL DEFAULT 1, last_error TEXT, created_at TEXT NOT NULL
);
INSERT INTO collection_schedules_v5 (id,name,collection,cadence,interval_minutes,local_time,anchor,enabled,next_run,revision,last_error,created_at)
  SELECT id,name,collection,cadence,interval_minutes,local_time,anchor,enabled,next_run,revision,last_error,created_at FROM collection_schedules;
DROP TABLE collection_schedules;
ALTER TABLE collection_schedules_v5 RENAME TO collection_schedules;
