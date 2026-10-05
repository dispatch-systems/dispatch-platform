-- Break punches and planned breaks, one row each, as the itinerary lists them.
CREATE TABLE IF NOT EXISTS breaks (
  publication_id TEXT NOT NULL REFERENCES route_publications(id) ON DELETE CASCADE,
  day TEXT NOT NULL, itinerary_id TEXT NOT NULL, transporter_id TEXT NOT NULL, ordinal INTEGER NOT NULL,
  planned INTEGER NOT NULL CHECK(planned IN (0,1)), break_id TEXT, kind TEXT, state TEXT, sequence INTEGER, punch_id TEXT,
  started_at INTEGER, ended_at INTEGER, planned_start_at INTEGER, planned_end_at INTEGER, min_duration_ms INTEGER,
  PRIMARY KEY(publication_id,itinerary_id,planned,ordinal)
);
CREATE INDEX IF NOT EXISTS breaks_day ON breaks(day,transporter_id);
-- Places the van dwelt that were not stops: where it entered and left, and when.
CREATE TABLE IF NOT EXISTS unknown_stops (
  publication_id TEXT NOT NULL REFERENCES route_publications(id) ON DELETE CASCADE,
  day TEXT NOT NULL, itinerary_id TEXT NOT NULL, transporter_id TEXT NOT NULL, ordinal INTEGER NOT NULL,
  entered_at INTEGER, exited_at INTEGER, enter_latitude REAL, enter_longitude REAL, exit_latitude REAL, exit_longitude REAL,
  PRIMARY KEY(publication_id,itinerary_id,ordinal)
);
CREATE INDEX IF NOT EXISTS unknown_stops_day ON unknown_stops(day,transporter_id);
CREATE INDEX IF NOT EXISTS tasks_itinerary ON tasks(publication_id,itinerary_id);
-- The questions asked most, pre-joined: which package went where, by whom, with what outcome.
CREATE VIEW IF NOT EXISTS deliveries AS
  SELECT t.day, t.transporter_id, i.driver_name, i.route_code, t.itinerary_id, t.stop_id, t.task_id, t.tracking_id,
    t.order_id, t.task_type, t.task_state, t.state_context, t.execution_status, t.executed_at, t.window_start_at,
    t.window_end_at, t.latitude scan_latitude, t.longitude scan_longitude, t.address_id, a.address1, a.city, a.state,
    a.postal_code, a.latitude address_latitude, a.longitude address_longitude, t.address_type, t.box_type, t.weight,
    t.active, t.publication_id
  FROM tasks t
  LEFT JOIN itineraries i ON i.publication_id=t.publication_id AND i.itinerary_id=t.itinerary_id
  LEFT JOIN addresses a ON a.address_id=t.address_id;
-- Each stop with its location and what happened there.
CREATE VIEW IF NOT EXISTS driver_stops AS
  SELECT s.day, i.transporter_id, i.driver_name, s.route_code, s.itinerary_id, s.stop_id, s.sequence, s.stop_type,
    s.planned_start_at, s.planned_end_at, s.address_id, a.address1, a.city, a.state, a.postal_code, a.latitude, a.longitude,
    (SELECT count(*) FROM tasks t WHERE t.publication_id=s.publication_id AND t.itinerary_id=s.itinerary_id AND t.stop_id=s.stop_id) tasks,
    (SELECT count(*) FROM tasks t WHERE t.publication_id=s.publication_id AND t.itinerary_id=s.itinerary_id AND t.stop_id=s.stop_id AND t.task_state='DELIVERED') delivered,
    (SELECT min(executed_at) FROM tasks t WHERE t.publication_id=s.publication_id AND t.itinerary_id=s.itinerary_id AND t.stop_id=s.stop_id) first_scan_at,
    (SELECT max(executed_at) FROM tasks t WHERE t.publication_id=s.publication_id AND t.itinerary_id=s.itinerary_id AND t.stop_id=s.stop_id) last_scan_at,
    s.flags, s.publication_id
  FROM stops s
  JOIN itineraries i ON i.publication_id=s.publication_id AND i.itinerary_id=s.itinerary_id
  LEFT JOIN addresses a ON a.address_id=s.address_id;
