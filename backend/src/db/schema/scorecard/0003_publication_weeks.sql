-- A job collects up to four weeks and publishes each as its own publication, so a
-- publication is unique by job and week instead of by job. SQLite cannot change the
-- constraint in place, so the table is rebuilt. Every table that references it would
-- lose its rows to ON DELETE CASCADE, or its link to SET NULL, if the old table were
-- dropped under it, so each is rebuilt first against the new table, the old tables go
-- child before parent, and the renames point them back, as jobs/0002 does. A release
-- before this one publishes one week per job, which the new key still takes.
CREATE TABLE scorecard_publications_v2 (
  id TEXT PRIMARY KEY, job_id TEXT NOT NULL, week TEXT NOT NULL, station TEXT NOT NULL,
  company_id TEXT NOT NULL, dsp_code TEXT NOT NULL, started_at TEXT NOT NULL, collected_at TEXT NOT NULL,
  active INTEGER NOT NULL DEFAULT 0 CHECK(active IN (0,1)), row_count INTEGER NOT NULL, adapter_version INTEGER NOT NULL,
  UNIQUE(job_id,week)
);
INSERT INTO scorecard_publications_v2 (id,job_id,week,station,company_id,dsp_code,started_at,collected_at,active,row_count,adapter_version)
  SELECT id,job_id,week,station,company_id,dsp_code,started_at,collected_at,active,row_count,adapter_version
  FROM scorecard_publications;

CREATE TABLE scorecard_weeks_v2 (
  week TEXT NOT NULL, station TEXT NOT NULL, checked_at TEXT NOT NULL,
  posted INTEGER NOT NULL CHECK(posted IN (0,1)),
  publication_id TEXT REFERENCES scorecard_publications_v2(id) ON DELETE SET NULL,
  PRIMARY KEY(week,station)
);
INSERT INTO scorecard_weeks_v2 (week,station,checked_at,posted,publication_id)
  SELECT week,station,checked_at,posted,publication_id FROM scorecard_weeks;
DROP TABLE scorecard_weeks;

CREATE TABLE scorecard_sources_v2 (
  publication_id TEXT NOT NULL REFERENCES scorecard_publications_v2(id) ON DELETE CASCADE,
  dataset TEXT NOT NULL, source TEXT NOT NULL CHECK(source IN ('api','csv')),
  url TEXT NOT NULL, row_count INTEGER NOT NULL,
  PRIMARY KEY(publication_id,dataset)
);
INSERT INTO scorecard_sources_v2 (publication_id,dataset,source,url,row_count)
  SELECT publication_id,dataset,source,url,row_count FROM scorecard_sources;
DROP TABLE scorecard_sources;

CREATE TABLE driver_scorecards_v2 (
  publication_id TEXT NOT NULL REFERENCES scorecard_publications_v2(id) ON DELETE CASCADE,
  row_index INTEGER NOT NULL, week TEXT, data_date TEXT, transporter_id TEXT, tracking_id TEXT, event_id TEXT,
  impact INTEGER, row TEXT NOT NULL CHECK(json_valid(row)), PRIMARY KEY(publication_id,row_index)
);
INSERT INTO driver_scorecards_v2 (publication_id,row_index,week,data_date,transporter_id,tracking_id,event_id,impact,row) SELECT publication_id,row_index,week,data_date,transporter_id,tracking_id,event_id,impact,row FROM driver_scorecards;
DROP TABLE driver_scorecards;

CREATE TABLE driver_safety_v2 (
  publication_id TEXT NOT NULL REFERENCES scorecard_publications_v2(id) ON DELETE CASCADE,
  row_index INTEGER NOT NULL, week TEXT, data_date TEXT, transporter_id TEXT, tracking_id TEXT, event_id TEXT,
  impact INTEGER, row TEXT NOT NULL CHECK(json_valid(row)), PRIMARY KEY(publication_id,row_index)
);
INSERT INTO driver_safety_v2 (publication_id,row_index,week,data_date,transporter_id,tracking_id,event_id,impact,row) SELECT publication_id,row_index,week,data_date,transporter_id,tracking_id,event_id,impact,row FROM driver_safety;
DROP TABLE driver_safety;

CREATE TABLE returns_to_station_v2 (
  publication_id TEXT NOT NULL REFERENCES scorecard_publications_v2(id) ON DELETE CASCADE,
  row_index INTEGER NOT NULL, week TEXT, data_date TEXT, transporter_id TEXT, tracking_id TEXT, event_id TEXT,
  impact INTEGER, row TEXT NOT NULL CHECK(json_valid(row)), PRIMARY KEY(publication_id,row_index)
);
INSERT INTO returns_to_station_v2 (publication_id,row_index,week,data_date,transporter_id,tracking_id,event_id,impact,row) SELECT publication_id,row_index,week,data_date,transporter_id,tracking_id,event_id,impact,row FROM returns_to_station;
DROP TABLE returns_to_station;

CREATE TABLE customer_feedback_v2 (
  publication_id TEXT NOT NULL REFERENCES scorecard_publications_v2(id) ON DELETE CASCADE,
  row_index INTEGER NOT NULL, week TEXT, data_date TEXT, transporter_id TEXT, tracking_id TEXT, event_id TEXT,
  impact INTEGER, row TEXT NOT NULL CHECK(json_valid(row)), PRIMARY KEY(publication_id,row_index)
);
INSERT INTO customer_feedback_v2 (publication_id,row_index,week,data_date,transporter_id,tracking_id,event_id,impact,row) SELECT publication_id,row_index,week,data_date,transporter_id,tracking_id,event_id,impact,row FROM customer_feedback;
DROP TABLE customer_feedback;

CREATE TABLE delivery_concessions_v2 (
  publication_id TEXT NOT NULL REFERENCES scorecard_publications_v2(id) ON DELETE CASCADE,
  row_index INTEGER NOT NULL, week TEXT, data_date TEXT, transporter_id TEXT, tracking_id TEXT, event_id TEXT,
  impact INTEGER, row TEXT NOT NULL CHECK(json_valid(row)), PRIMARY KEY(publication_id,row_index)
);
INSERT INTO delivery_concessions_v2 (publication_id,row_index,week,data_date,transporter_id,tracking_id,event_id,impact,row) SELECT publication_id,row_index,week,data_date,transporter_id,tracking_id,event_id,impact,row FROM delivery_concessions;
DROP TABLE delivery_concessions;

CREATE TABLE pickup_failures_v2 (
  publication_id TEXT NOT NULL REFERENCES scorecard_publications_v2(id) ON DELETE CASCADE,
  row_index INTEGER NOT NULL, week TEXT, data_date TEXT, transporter_id TEXT, tracking_id TEXT, event_id TEXT,
  impact INTEGER, row TEXT NOT NULL CHECK(json_valid(row)), PRIMARY KEY(publication_id,row_index)
);
INSERT INTO pickup_failures_v2 (publication_id,row_index,week,data_date,transporter_id,tracking_id,event_id,impact,row) SELECT publication_id,row_index,week,data_date,transporter_id,tracking_id,event_id,impact,row FROM pickup_failures;
DROP TABLE pickup_failures;

CREATE TABLE safety_events_v2 (
  publication_id TEXT NOT NULL REFERENCES scorecard_publications_v2(id) ON DELETE CASCADE,
  row_index INTEGER NOT NULL, week TEXT, data_date TEXT, transporter_id TEXT, tracking_id TEXT, event_id TEXT,
  impact INTEGER, row TEXT NOT NULL CHECK(json_valid(row)), PRIMARY KEY(publication_id,row_index)
);
INSERT INTO safety_events_v2 (publication_id,row_index,week,data_date,transporter_id,tracking_id,event_id,impact,row) SELECT publication_id,row_index,week,data_date,transporter_id,tracking_id,event_id,impact,row FROM safety_events;
DROP TABLE safety_events;

CREATE TABLE dsp_quality_v2 (
  publication_id TEXT NOT NULL REFERENCES scorecard_publications_v2(id) ON DELETE CASCADE,
  row_index INTEGER NOT NULL, week TEXT, data_date TEXT, transporter_id TEXT, tracking_id TEXT, event_id TEXT,
  impact INTEGER, row TEXT NOT NULL CHECK(json_valid(row)), PRIMARY KEY(publication_id,row_index)
);
INSERT INTO dsp_quality_v2 (publication_id,row_index,week,data_date,transporter_id,tracking_id,event_id,impact,row) SELECT publication_id,row_index,week,data_date,transporter_id,tracking_id,event_id,impact,row FROM dsp_quality;
DROP TABLE dsp_quality;

CREATE TABLE dsp_team_v2 (
  publication_id TEXT NOT NULL REFERENCES scorecard_publications_v2(id) ON DELETE CASCADE,
  row_index INTEGER NOT NULL, week TEXT, data_date TEXT, transporter_id TEXT, tracking_id TEXT, event_id TEXT,
  impact INTEGER, row TEXT NOT NULL CHECK(json_valid(row)), PRIMARY KEY(publication_id,row_index)
);
INSERT INTO dsp_team_v2 (publication_id,row_index,week,data_date,transporter_id,tracking_id,event_id,impact,row) SELECT publication_id,row_index,week,data_date,transporter_id,tracking_id,event_id,impact,row FROM dsp_team;
DROP TABLE dsp_team;

CREATE TABLE dsp_compliance_v2 (
  publication_id TEXT NOT NULL REFERENCES scorecard_publications_v2(id) ON DELETE CASCADE,
  row_index INTEGER NOT NULL, week TEXT, data_date TEXT, transporter_id TEXT, tracking_id TEXT, event_id TEXT,
  impact INTEGER, row TEXT NOT NULL CHECK(json_valid(row)), PRIMARY KEY(publication_id,row_index)
);
INSERT INTO dsp_compliance_v2 (publication_id,row_index,week,data_date,transporter_id,tracking_id,event_id,impact,row) SELECT publication_id,row_index,week,data_date,transporter_id,tracking_id,event_id,impact,row FROM dsp_compliance;
DROP TABLE dsp_compliance;

CREATE TABLE dsp_safety_v2 (
  publication_id TEXT NOT NULL REFERENCES scorecard_publications_v2(id) ON DELETE CASCADE,
  row_index INTEGER NOT NULL, week TEXT, data_date TEXT, transporter_id TEXT, tracking_id TEXT, event_id TEXT,
  impact INTEGER, row TEXT NOT NULL CHECK(json_valid(row)), PRIMARY KEY(publication_id,row_index)
);
INSERT INTO dsp_safety_v2 (publication_id,row_index,week,data_date,transporter_id,tracking_id,event_id,impact,row) SELECT publication_id,row_index,week,data_date,transporter_id,tracking_id,event_id,impact,row FROM dsp_safety;
DROP TABLE dsp_safety;

CREATE TABLE dsp_working_device_v2 (
  publication_id TEXT NOT NULL REFERENCES scorecard_publications_v2(id) ON DELETE CASCADE,
  row_index INTEGER NOT NULL, week TEXT, data_date TEXT, transporter_id TEXT, tracking_id TEXT, event_id TEXT,
  impact INTEGER, row TEXT NOT NULL CHECK(json_valid(row)), PRIMARY KEY(publication_id,row_index)
);
INSERT INTO dsp_working_device_v2 (publication_id,row_index,week,data_date,transporter_id,tracking_id,event_id,impact,row) SELECT publication_id,row_index,week,data_date,transporter_id,tracking_id,event_id,impact,row FROM dsp_working_device;
DROP TABLE dsp_working_device;

CREATE TABLE dsp_feedback_v2 (
  publication_id TEXT NOT NULL REFERENCES scorecard_publications_v2(id) ON DELETE CASCADE,
  row_index INTEGER NOT NULL, week TEXT, data_date TEXT, transporter_id TEXT, tracking_id TEXT, event_id TEXT,
  impact INTEGER, row TEXT NOT NULL CHECK(json_valid(row)), PRIMARY KEY(publication_id,row_index)
);
INSERT INTO dsp_feedback_v2 (publication_id,row_index,week,data_date,transporter_id,tracking_id,event_id,impact,row) SELECT publication_id,row_index,week,data_date,transporter_id,tracking_id,event_id,impact,row FROM dsp_feedback;
DROP TABLE dsp_feedback;

CREATE TABLE dsp_pickups_v2 (
  publication_id TEXT NOT NULL REFERENCES scorecard_publications_v2(id) ON DELETE CASCADE,
  row_index INTEGER NOT NULL, week TEXT, data_date TEXT, transporter_id TEXT, tracking_id TEXT, event_id TEXT,
  impact INTEGER, row TEXT NOT NULL CHECK(json_valid(row)), PRIMARY KEY(publication_id,row_index)
);
INSERT INTO dsp_pickups_v2 (publication_id,row_index,week,data_date,transporter_id,tracking_id,event_id,impact,row) SELECT publication_id,row_index,week,data_date,transporter_id,tracking_id,event_id,impact,row FROM dsp_pickups;
DROP TABLE dsp_pickups;

DROP TABLE scorecard_publications;
ALTER TABLE scorecard_publications_v2 RENAME TO scorecard_publications;
ALTER TABLE scorecard_weeks_v2 RENAME TO scorecard_weeks;
ALTER TABLE scorecard_sources_v2 RENAME TO scorecard_sources;
ALTER TABLE driver_scorecards_v2 RENAME TO driver_scorecards;
ALTER TABLE driver_safety_v2 RENAME TO driver_safety;
ALTER TABLE returns_to_station_v2 RENAME TO returns_to_station;
ALTER TABLE customer_feedback_v2 RENAME TO customer_feedback;
ALTER TABLE delivery_concessions_v2 RENAME TO delivery_concessions;
ALTER TABLE pickup_failures_v2 RENAME TO pickup_failures;
ALTER TABLE safety_events_v2 RENAME TO safety_events;
ALTER TABLE dsp_quality_v2 RENAME TO dsp_quality;
ALTER TABLE dsp_team_v2 RENAME TO dsp_team;
ALTER TABLE dsp_compliance_v2 RENAME TO dsp_compliance;
ALTER TABLE dsp_safety_v2 RENAME TO dsp_safety;
ALTER TABLE dsp_working_device_v2 RENAME TO dsp_working_device;
ALTER TABLE dsp_feedback_v2 RENAME TO dsp_feedback;
ALTER TABLE dsp_pickups_v2 RENAME TO dsp_pickups;
CREATE UNIQUE INDEX IF NOT EXISTS active_scorecard_week ON scorecard_publications(week,station,company_id) WHERE active=1;
CREATE INDEX IF NOT EXISTS scorecard_history ON scorecard_publications(week,collected_at DESC);
CREATE INDEX IF NOT EXISTS driver_scorecards_transporter ON driver_scorecards(transporter_id);
CREATE INDEX IF NOT EXISTS driver_safety_transporter ON driver_safety(transporter_id);
CREATE INDEX IF NOT EXISTS returns_to_station_transporter ON returns_to_station(transporter_id);
CREATE INDEX IF NOT EXISTS customer_feedback_transporter ON customer_feedback(transporter_id);
CREATE INDEX IF NOT EXISTS delivery_concessions_transporter ON delivery_concessions(transporter_id);
CREATE INDEX IF NOT EXISTS pickup_failures_transporter ON pickup_failures(transporter_id);
CREATE INDEX IF NOT EXISTS safety_events_transporter ON safety_events(transporter_id);
CREATE INDEX IF NOT EXISTS dsp_quality_transporter ON dsp_quality(transporter_id);
CREATE INDEX IF NOT EXISTS dsp_team_transporter ON dsp_team(transporter_id);
CREATE INDEX IF NOT EXISTS dsp_compliance_transporter ON dsp_compliance(transporter_id);
CREATE INDEX IF NOT EXISTS dsp_safety_transporter ON dsp_safety(transporter_id);
CREATE INDEX IF NOT EXISTS dsp_working_device_transporter ON dsp_working_device(transporter_id);
CREATE INDEX IF NOT EXISTS dsp_feedback_transporter ON dsp_feedback(transporter_id);
CREATE INDEX IF NOT EXISTS dsp_pickups_transporter ON dsp_pickups(transporter_id);
