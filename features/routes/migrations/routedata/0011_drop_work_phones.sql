-- Each day's work phone holds, which nothing reads or writes any more: a day's work phones are its
-- itineraries' (`itineraries.work_phone`). Its phone index goes with it. The previous release
-- never had it.
DROP TABLE IF EXISTS work_phones;
