-- No read finds tasks by driver but a driver's undelivered drop-offs, which tasks_undelivered
-- holds alone: tasks_driver_day, added beside it, served none of them, and only slowed every
-- write of a task. The previous release reads no differently without it.
DROP INDEX IF EXISTS tasks_driver_day;
