-- A driver's removed and undelivered packages, found without reading the rest: whether a task
-- was removed or delivered is in its row alone, so counting an itinerary's removed tasks and
-- finding a driver's undelivered drop-offs read every task of their days, about 9,000 rows for
-- 60 of one driver's, from disk once the file outgrows memory. Each holds only those tasks.
CREATE INDEX IF NOT EXISTS tasks_removed ON tasks(publication_id, itinerary_id) WHERE active=0;
CREATE INDEX IF NOT EXISTS tasks_undelivered ON tasks(transporter_id, day)
    WHERE active=1 AND task_type='DROP_OFF' AND task_state IS NOT 'DELIVERED';
