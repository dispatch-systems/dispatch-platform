-- A package moved from one driver to another is listed by both itineraries: removed
-- from the first, active on the second. Keyed by publication and task alone, whichever
-- itinerary was stored first kept the only row, so a rescued delivery could be filed as
-- removed under the wrong driver. Tasks are keyed by itinerary too, and every row is
-- kept. The rebuild drops tasks_itinerary, which the new key's prefix serves, and keeps
-- each task's recent events as compact [time, state, context] triples; the response
-- itself stays whole in route_raw. Stored days are rebuilt from route_raw by reprocessing.
--
-- Addresses lose customer_name and customer_phone: Amazon sends them empty, and no
-- release before this one opened route data, so no rollback target reads them.
--
-- The views are dropped first, since they read both tables, and come back reading only
-- active publications, as every other reader does.
DROP VIEW IF EXISTS deliveries;
DROP VIEW IF EXISTS driver_stops;

CREATE TABLE tasks_v2 (
  publication_id TEXT NOT NULL REFERENCES route_publications(id) ON DELETE CASCADE,
  day TEXT NOT NULL, itinerary_id TEXT NOT NULL, stop_id TEXT NOT NULL, task_id TEXT NOT NULL,
  transporter_id TEXT NOT NULL, tracking_id TEXT, order_id TEXT, task_type TEXT, task_state TEXT,
  state_context TEXT, execution_status TEXT, address_id TEXT,
  window_start_at INTEGER, window_end_at INTEGER, executed_at INTEGER,
  latitude REAL, longitude REAL, box_type TEXT, weight REAL, weight_unit TEXT,
  length REAL, width REAL, height REAL, volume_unit TEXT,
  time_windowed INTEGER, high_value INTEGER, customer_return INTEGER, address_type TEXT,
  events TEXT NOT NULL CHECK(json_valid(events)),
  active INTEGER NOT NULL DEFAULT 1,
  PRIMARY KEY(publication_id,itinerary_id,task_id)
);
INSERT INTO tasks_v2 (publication_id,day,itinerary_id,stop_id,task_id,transporter_id,tracking_id,order_id,
  task_type,task_state,state_context,execution_status,address_id,window_start_at,window_end_at,executed_at,
  latitude,longitude,box_type,weight,weight_unit,length,width,height,volume_unit,time_windowed,high_value,
  customer_return,address_type,events,active)
  SELECT publication_id,day,itinerary_id,stop_id,task_id,transporter_id,tracking_id,order_id,
    task_type,task_state,state_context,execution_status,address_id,window_start_at,window_end_at,executed_at,
    latitude,longitude,box_type,weight,weight_unit,length,width,height,volume_unit,time_windowed,high_value,
    customer_return,address_type,
    COALESCE((SELECT json_group_array(json_array(json_extract(e.value,'$.executionTime'),
      json_extract(e.value,'$.taskState'),json_extract(e.value,'$.taskStateContext')))
      FROM json_each(tasks.events) e), '[]'),
    active
  FROM tasks;
DROP TABLE tasks;
ALTER TABLE tasks_v2 RENAME TO tasks;
CREATE INDEX IF NOT EXISTS tasks_day_driver ON tasks(day,transporter_id);
CREATE INDEX IF NOT EXISTS tasks_tracking ON tasks(tracking_id);
CREATE INDEX IF NOT EXISTS tasks_address ON tasks(address_id);

CREATE TABLE addresses_v2 (
  address_id TEXT PRIMARY KEY, address1 TEXT, address2 TEXT, address3 TEXT,
  city TEXT, state TEXT, postal_code TEXT,
  latitude REAL, longitude REAL, first_seen_day TEXT NOT NULL, last_seen_day TEXT NOT NULL
);
INSERT INTO addresses_v2 (address_id,address1,address2,address3,city,state,postal_code,latitude,longitude,
  first_seen_day,last_seen_day)
  SELECT address_id,address1,address2,address3,city,state,postal_code,latitude,longitude,first_seen_day,last_seen_day
  FROM addresses;
DROP TABLE addresses;
ALTER TABLE addresses_v2 RENAME TO addresses;
CREATE INDEX IF NOT EXISTS addresses_last_seen ON addresses(last_seen_day);
CREATE INDEX IF NOT EXISTS drivers_last_seen ON drivers(last_seen_day);

-- How long the DSP keeps its route data, in days. Without a row, or without days, every
-- day is kept: nothing is deleted until a DSP chooses a window.
CREATE TABLE IF NOT EXISTS route_retention (
  id INTEGER PRIMARY KEY CHECK(id=1),
  days INTEGER CHECK(days IS NULL OR days BETWEEN 30 AND 3650),
  changed_by TEXT, changed_at TEXT NOT NULL
);

-- The questions asked most, pre-joined: which package went where, by whom, with what outcome.
CREATE VIEW IF NOT EXISTS deliveries AS
  SELECT t.day, t.transporter_id, i.driver_name, i.route_code, t.itinerary_id, t.stop_id, t.task_id, t.tracking_id,
    t.order_id, t.task_type, t.task_state, t.state_context, t.execution_status, t.executed_at, t.window_start_at,
    t.window_end_at, t.latitude scan_latitude, t.longitude scan_longitude, t.address_id, a.address1, a.city, a.state,
    a.postal_code, a.latitude address_latitude, a.longitude address_longitude, t.address_type, t.box_type, t.weight,
    t.active, t.publication_id
  FROM tasks t
  JOIN route_publications p ON p.id=t.publication_id AND p.active=1
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
  JOIN route_publications p ON p.id=s.publication_id AND p.active=1
  JOIN itineraries i ON i.publication_id=s.publication_id AND i.itinerary_id=s.itinerary_id
  LEFT JOIN addresses a ON a.address_id=s.address_id;
