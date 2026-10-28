-- The live day's work phone holds, and what each driver's record showed that they were settled
-- from, which nothing reads or writes any more: the day's work phones are its itineraries'
-- (`itineraries.work_phone`). The previous release never had them.
DROP TABLE IF EXISTS work_phones;
DROP TABLE IF EXISTS work_phones_shown;
