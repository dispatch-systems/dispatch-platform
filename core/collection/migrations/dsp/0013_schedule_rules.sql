-- Each schedule is one rule of its collection's: the days of the week it runs on, Monday the
-- lowest bit, and its times, or the time its repeat runs until, and its place among its
-- collection's rules. The previous release reads its first time, or the start and interval
-- of its repeat, as before.
ALTER TABLE collection_schedules ADD COLUMN days INTEGER NOT NULL DEFAULT 127 CHECK (days BETWEEN 1 AND 127);
ALTER TABLE collection_schedules ADD COLUMN times TEXT CHECK (times IS NULL OR json_valid(times));
ALTER TABLE collection_schedules ADD COLUMN until_time TEXT;
ALTER TABLE collection_schedules ADD COLUMN position INTEGER NOT NULL DEFAULT 0;
