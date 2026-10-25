-- A driver's routes over a range of days, found by driver first: (day, transporter_id) can only
-- narrow a range of days, so it read every driver's rows in the range to find one's.
CREATE INDEX IF NOT EXISTS tasks_driver_day ON tasks(transporter_id, day);
CREATE INDEX IF NOT EXISTS driver_days_driver_day ON driver_days(transporter_id, day);
