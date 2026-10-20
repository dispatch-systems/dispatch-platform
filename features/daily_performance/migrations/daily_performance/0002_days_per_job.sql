-- One job reads several days and publishes each, so a publication is one job's one day.
-- daily_publications.job_id was UNIQUE, which only a rebuild drops. daily_datasets and
-- daily_rows reference it, so they are rebuilt against the new table, the old tables go
-- child before parent, and the renames make the children reference daily_publications
-- again, with the same columns, constraints and indexes. The previous release reads the
-- tables unchanged and publishes one day per job, which stays unique.
CREATE TABLE daily_publications_v2(
 id TEXT PRIMARY KEY,job_id TEXT NOT NULL,date TEXT NOT NULL,station TEXT NOT NULL,
 company_id TEXT NOT NULL,dsp_code TEXT NOT NULL,started_at TEXT NOT NULL,collected_at TEXT NOT NULL,
 active INTEGER NOT NULL CHECK(active IN (0,1)),row_count INTEGER NOT NULL CHECK(row_count>=0),
 UNIQUE(job_id,date));
INSERT INTO daily_publications_v2(id,job_id,date,station,company_id,dsp_code,started_at,collected_at,active,row_count)
  SELECT id,job_id,date,station,company_id,dsp_code,started_at,collected_at,active,row_count FROM daily_publications;
CREATE TABLE daily_datasets_v2(
 publication_id TEXT NOT NULL REFERENCES daily_publications_v2(id),dataset TEXT NOT NULL,
 source_url TEXT NOT NULL,row_count INTEGER NOT NULL CHECK(row_count>=0),
 coverage TEXT NOT NULL CHECK(coverage IN ('observed','unconfirmed')),
 PRIMARY KEY(publication_id,dataset));
INSERT INTO daily_datasets_v2(publication_id,dataset,source_url,row_count,coverage)
  SELECT publication_id,dataset,source_url,row_count,coverage FROM daily_datasets;
CREATE TABLE daily_rows_v2(
 publication_id TEXT NOT NULL REFERENCES daily_publications_v2(id),dataset TEXT NOT NULL,
 row_index INTEGER NOT NULL,date TEXT NOT NULL,transporter_id TEXT,tracking_id TEXT,event_id TEXT,
 impact INTEGER CHECK(impact IN (0,1)),row TEXT NOT NULL CHECK(json_valid(row)),
 PRIMARY KEY(publication_id,dataset,row_index));
INSERT INTO daily_rows_v2(publication_id,dataset,row_index,date,transporter_id,tracking_id,event_id,impact,row)
  SELECT publication_id,dataset,row_index,date,transporter_id,tracking_id,event_id,impact,row FROM daily_rows;
DROP TABLE daily_rows;
DROP TABLE daily_datasets;
DROP TABLE daily_publications;
ALTER TABLE daily_publications_v2 RENAME TO daily_publications;
ALTER TABLE daily_datasets_v2 RENAME TO daily_datasets;
ALTER TABLE daily_rows_v2 RENAME TO daily_rows;
CREATE UNIQUE INDEX IF NOT EXISTS daily_active ON daily_publications(station,date) WHERE active=1;
CREATE INDEX IF NOT EXISTS daily_history ON daily_publications(station,collected_at);
CREATE INDEX IF NOT EXISTS daily_driver_rows ON daily_rows(dataset,transporter_id,date,publication_id);
CREATE INDEX IF NOT EXISTS daily_date_rows ON daily_rows(dataset,date,publication_id,row_index);
CREATE INDEX IF NOT EXISTS daily_event_rows ON daily_rows(dataset,event_id,date);
