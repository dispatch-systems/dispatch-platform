CREATE TABLE IF NOT EXISTS storage_identity (dsp_id TEXT NOT NULL, provider TEXT NOT NULL, source TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS routedata_schema (version INTEGER PRIMARY KEY CHECK(version=1));
INSERT OR IGNORE INTO routedata_schema VALUES (1);

-- One collection of one day's routes at a station for a provider. The newest is
-- active; a recollection replaces the previous publication and everything under it,
-- since route data has no disputes to keep older readings for.
CREATE TABLE IF NOT EXISTS route_publications (
  id TEXT PRIMARY KEY, job_id TEXT NOT NULL UNIQUE, day TEXT NOT NULL, station TEXT NOT NULL,
  service_area_id TEXT NOT NULL, provider TEXT NOT NULL, timezone TEXT NOT NULL,
  mode TEXT NOT NULL CHECK(mode IN ('final','snapshot')),
  started_at TEXT NOT NULL, collected_at TEXT NOT NULL,
  active INTEGER NOT NULL DEFAULT 0 CHECK(active IN (0,1)),
  route_count INTEGER NOT NULL, itinerary_count INTEGER NOT NULL,
  stop_count INTEGER NOT NULL, task_count INTEGER NOT NULL, adapter_version INTEGER NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS active_route_day ON route_publications(day,station,service_area_id,provider) WHERE active=1;
CREATE INDEX IF NOT EXISTS route_publications_day ON route_publications(day,collected_at DESC);

-- The station's planned routes for the day, as the routes page lists them: one row per
-- route, with the drivers assigned to it and the summary Amazon sent.
CREATE TABLE IF NOT EXISTS routes (
  publication_id TEXT NOT NULL REFERENCES route_publications(id) ON DELETE CASCADE,
  day TEXT NOT NULL, route_id TEXT NOT NULL, rms_route_id TEXT, route_code TEXT, company_id TEXT,
  service_type TEXT, route_status TEXT, progress_status TEXT,
  planned_departure_at INTEGER, created_at INTEGER, duration_secs INTEGER,
  total_stops INTEGER, total_tasks INTEGER, unassigned_stops INTEGER, unassigned_packages INTEGER,
  late_departing INTEGER, pre_dispatch INTEGER, same_day INTEGER,
  transporter_ids TEXT NOT NULL CHECK(json_valid(transporter_ids)),
  summary TEXT NOT NULL CHECK(json_valid(summary)),
  PRIMARY KEY(publication_id,route_id)
);
CREATE INDEX IF NOT EXISTS routes_day ON routes(day,route_code);

-- One driver's itinerary for the day: the list's summary flattened, with the detail's
-- route-level fields. Times are epoch milliseconds.
CREATE TABLE IF NOT EXISTS itineraries (
  publication_id TEXT NOT NULL REFERENCES route_publications(id) ON DELETE CASCADE,
  day TEXT NOT NULL, itinerary_id TEXT NOT NULL, transporter_id TEXT NOT NULL, driver_name TEXT NOT NULL,
  route_code TEXT, route_id TEXT, company_id TEXT, vin TEXT, service_type TEXT,
  execution_status TEXT, progress_status TEXT,
  planned_departure_at INTEGER, departed_at INTEGER, started_at INTEGER, wave_start_at INTEGER,
  session_end_at INTEGER, projected_completion_at INTEGER, last_stop_at INTEGER, block_minutes INTEGER,
  total_locations INTEGER, completed_locations INTEGER, total_packages INTEGER,
  delivered INTEGER, remaining INTEGER, undeliverable INTEGER,
  stops_impacted INTEGER, packages_impacted INTEGER, breaks_secs INTEGER, overtime_secs INTEGER,
  stop_completion_rate REAL,
  breaks TEXT NOT NULL CHECK(json_valid(breaks)), planned_breaks TEXT NOT NULL CHECK(json_valid(planned_breaks)),
  summary TEXT NOT NULL CHECK(json_valid(summary)),
  PRIMARY KEY(publication_id,itinerary_id)
);
CREATE INDEX IF NOT EXISTS itineraries_day_driver ON itineraries(day,transporter_id);

-- One row per stop of an itinerary, in sequence.
CREATE TABLE IF NOT EXISTS stops (
  publication_id TEXT NOT NULL REFERENCES route_publications(id) ON DELETE CASCADE,
  day TEXT NOT NULL, itinerary_id TEXT NOT NULL, stop_id TEXT NOT NULL, sequence INTEGER,
  address_id TEXT, stop_type TEXT, route_code TEXT,
  planned_start_at INTEGER, planned_end_at INTEGER, expected_start_at INTEGER, actual_start_at INTEGER,
  flags TEXT NOT NULL CHECK(json_valid(flags)),
  PRIMARY KEY(publication_id,itinerary_id,stop_id)
);
CREATE INDEX IF NOT EXISTS stops_address ON stops(address_id);

-- One row per package action at a stop: a pickup at the station or a drop-off.
CREATE TABLE IF NOT EXISTS tasks (
  publication_id TEXT NOT NULL REFERENCES route_publications(id) ON DELETE CASCADE,
  day TEXT NOT NULL, itinerary_id TEXT NOT NULL, stop_id TEXT NOT NULL, task_id TEXT NOT NULL,
  transporter_id TEXT NOT NULL, tracking_id TEXT, order_id TEXT, task_type TEXT, task_state TEXT,
  state_context TEXT, execution_status TEXT, address_id TEXT,
  window_start_at INTEGER, window_end_at INTEGER, executed_at INTEGER,
  latitude REAL, longitude REAL, box_type TEXT, weight REAL, weight_unit TEXT,
  length REAL, width REAL, height REAL, volume_unit TEXT,
  time_windowed INTEGER, high_value INTEGER, customer_return INTEGER, address_type TEXT,
  events TEXT NOT NULL CHECK(json_valid(events)),
  PRIMARY KEY(publication_id,task_id)
);
CREATE INDEX IF NOT EXISTS tasks_day_driver ON tasks(day,transporter_id);
CREATE INDEX IF NOT EXISTS tasks_tracking ON tasks(tracking_id);
CREATE INDEX IF NOT EXISTS tasks_address ON tasks(address_id);

-- Locations the day's stops were at, shared across days, as Amazon last sent them.
CREATE TABLE IF NOT EXISTS addresses (
  address_id TEXT PRIMARY KEY, address1 TEXT, address2 TEXT, address3 TEXT,
  city TEXT, state TEXT, postal_code TEXT, customer_name TEXT, customer_phone TEXT,
  latitude REAL, longitude REAL, first_seen_day TEXT NOT NULL, last_seen_day TEXT NOT NULL
);
-- Drivers seen on the routes, shared across days.
CREATE TABLE IF NOT EXISTS drivers (
  transporter_id TEXT PRIMARY KEY, first_name TEXT, last_name TEXT, initials TEXT, work_phone TEXT,
  person_type TEXT NOT NULL CHECK(json_valid(person_type)), company_id TEXT,
  first_seen_day TEXT NOT NULL, last_seen_day TEXT NOT NULL
);
-- One row per driver itinerary per day, computed at publication: the first thing a
-- dashboard reads.
CREATE TABLE IF NOT EXISTS driver_days (
  publication_id TEXT NOT NULL REFERENCES route_publications(id) ON DELETE CASCADE,
  day TEXT NOT NULL, transporter_id TEXT NOT NULL, itinerary_id TEXT NOT NULL, route_code TEXT,
  departed_at INTEGER, first_stop_at INTEGER, last_stop_at INTEGER, session_end_at INTEGER,
  stops_total INTEGER NOT NULL, stops_completed INTEGER NOT NULL, tasks_total INTEGER NOT NULL,
  delivered INTEGER NOT NULL, picked_up INTEGER NOT NULL, not_delivered INTEGER NOT NULL,
  breaks_secs INTEGER, overtime_secs INTEGER,
  PRIMARY KEY(publication_id,itinerary_id)
);
CREATE INDEX IF NOT EXISTS driver_days_day ON driver_days(day,transporter_id);
-- Every response the collection took, gzip-compressed, so a later release can read
-- fields this one did not, without collecting again.
CREATE TABLE IF NOT EXISTS route_raw (
  publication_id TEXT NOT NULL REFERENCES route_publications(id) ON DELETE CASCADE,
  name TEXT NOT NULL, encoding TEXT NOT NULL CHECK(encoding='gzip'),
  raw_bytes INTEGER NOT NULL, body BLOB NOT NULL,
  PRIMARY KEY(publication_id,name)
);
