CREATE TABLE fleet_runs ( job_id TEXT PRIMARY KEY, company_id TEXT NOT NULL, collected_at TEXT NOT NULL, listed INTEGER NOT NULL );
CREATE TABLE owner_migrations (owner TEXT NOT NULL, id INTEGER NOT NULL, name TEXT NOT NULL, applied_at INTEGER NOT NULL, PRIMARY KEY(owner, id));
CREATE TABLE schema_migrations (id INTEGER PRIMARY KEY, name TEXT NOT NULL, applied_at INTEGER NOT NULL);
CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL CHECK(json_valid(value)));
CREATE TABLE storage_identity (dsp_id TEXT NOT NULL, provider TEXT NOT NULL, source TEXT NOT NULL);
CREATE TABLE vans ( vin TEXT PRIMARY KEY, company_id TEXT NOT NULL, name TEXT, listed INTEGER NOT NULL CHECK(listed IN (0,1)), first_listed TEXT NOT NULL, last_listed TEXT NOT NULL, row TEXT NOT NULL );
