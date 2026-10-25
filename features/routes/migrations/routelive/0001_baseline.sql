-- Live route data: today's routes as the last whole live read left them, in the tables
-- historical route data keeps its days in, defined as routedata's stand after its migrations, so
-- one shaping and one reading serve both. It holds one day, kept in place read by read, with the
-- raw responses of its last whole read; emptied as a new day starts. Without the indexes only
-- reads across many days need, and the staged rows only staging writes.
CREATE TABLE IF NOT EXISTS storage_identity (dsp_id TEXT NOT NULL, provider TEXT NOT NULL, source TEXT NOT NULL);

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
  rescue_actions TEXT, sequence_edits TEXT, pause_events TEXT,
  PRIMARY KEY(publication_id,itinerary_id)
);

CREATE TABLE IF NOT EXISTS stops (
  publication_id TEXT NOT NULL REFERENCES route_publications(id) ON DELETE CASCADE,
  day TEXT NOT NULL, itinerary_id TEXT NOT NULL, stop_id TEXT NOT NULL, sequence INTEGER,
  address_id TEXT, stop_type TEXT, route_code TEXT,
  planned_start_at INTEGER, planned_end_at INTEGER, expected_start_at INTEGER, actual_start_at INTEGER,
  flags TEXT NOT NULL CHECK(json_valid(flags)),
  PRIMARY KEY(publication_id,itinerary_id,stop_id)
);

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
  active INTEGER NOT NULL DEFAULT 1,
  PRIMARY KEY(publication_id,itinerary_id,task_id)
);
-- A driver's removed and undelivered packages, found without reading the day's other tasks, as
-- in historical route data.
CREATE INDEX IF NOT EXISTS tasks_removed ON tasks(publication_id, itinerary_id) WHERE active=0;
CREATE INDEX IF NOT EXISTS tasks_undelivered ON tasks(transporter_id, day)
    WHERE active=1 AND task_type='DROP_OFF' AND task_state IS NOT 'DELIVERED';

CREATE TABLE IF NOT EXISTS addresses (
  address_id TEXT PRIMARY KEY, address1 TEXT, address2 TEXT, address3 TEXT,
  city TEXT, state TEXT, postal_code TEXT,
  latitude REAL, longitude REAL, first_seen_day TEXT NOT NULL, last_seen_day TEXT NOT NULL
);
-- The drivers live reads named: those with an itinerary, and those the route list assigns.
CREATE TABLE IF NOT EXISTS drivers (
  transporter_id TEXT PRIMARY KEY, first_name TEXT, last_name TEXT, initials TEXT, work_phone TEXT,
  person_type TEXT NOT NULL CHECK(json_valid(person_type)), company_id TEXT,
  first_seen_day TEXT NOT NULL, last_seen_day TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS driver_days (
  publication_id TEXT NOT NULL REFERENCES route_publications(id) ON DELETE CASCADE,
  day TEXT NOT NULL, transporter_id TEXT NOT NULL, itinerary_id TEXT NOT NULL, route_code TEXT,
  departed_at INTEGER, first_stop_at INTEGER, last_stop_at INTEGER, session_end_at INTEGER,
  stops_total INTEGER NOT NULL, stops_completed INTEGER NOT NULL, tasks_total INTEGER NOT NULL,
  delivered INTEGER NOT NULL, picked_up INTEGER NOT NULL, not_delivered INTEGER NOT NULL,
  breaks_secs INTEGER, overtime_secs INTEGER,
  PRIMARY KEY(publication_id,itinerary_id)
);
CREATE TABLE IF NOT EXISTS route_raw (
  publication_id TEXT NOT NULL REFERENCES route_publications(id) ON DELETE CASCADE,
  name TEXT NOT NULL, encoding TEXT NOT NULL CHECK(encoding='gzip'),
  raw_bytes INTEGER NOT NULL, body BLOB NOT NULL,
  PRIMARY KEY(publication_id,name)
);
CREATE TABLE IF NOT EXISTS breaks (
  publication_id TEXT NOT NULL REFERENCES route_publications(id) ON DELETE CASCADE,
  day TEXT NOT NULL, itinerary_id TEXT NOT NULL, transporter_id TEXT NOT NULL, ordinal INTEGER NOT NULL,
  planned INTEGER NOT NULL CHECK(planned IN (0,1)), break_id TEXT, kind TEXT, state TEXT, sequence INTEGER, punch_id TEXT,
  started_at INTEGER, ended_at INTEGER, planned_start_at INTEGER, planned_end_at INTEGER, min_duration_ms INTEGER,
  PRIMARY KEY(publication_id,itinerary_id,planned,ordinal)
);
CREATE TABLE IF NOT EXISTS unknown_stops (
  publication_id TEXT NOT NULL REFERENCES route_publications(id) ON DELETE CASCADE,
  day TEXT NOT NULL, itinerary_id TEXT NOT NULL, transporter_id TEXT NOT NULL, ordinal INTEGER NOT NULL,
  entered_at INTEGER, exited_at INTEGER, enter_latitude REAL, enter_longitude REAL, exit_latitude REAL, exit_longitude REAL,
  PRIMARY KEY(publication_id,itinerary_id,ordinal)
);

-- Every hold of the day on the company's work phones, from its claim until it ended, as the
-- live reads settled them; one without an end is held now.
CREATE TABLE IF NOT EXISTS work_phones (
    day TEXT NOT NULL,
    transporter_id TEXT NOT NULL,
    phone TEXT NOT NULL,
    since INTEGER NOT NULL,
    until INTEGER,
    PRIMARY KEY(day, transporter_id, phone, since)
) WITHOUT ROWID;
-- What each driver's record showed of a work phone today, which each read settles the holds
-- from: the first phone it showed, the one it showed at the last read that listed them, and
-- the one it showed when they last claimed one, which showing again claims nothing.
CREATE TABLE IF NOT EXISTS work_phones_shown (
    transporter_id TEXT PRIMARY KEY,
    first_shown TEXT,
    last_shown TEXT,
    claimed TEXT
) WITHOUT ROWID;
