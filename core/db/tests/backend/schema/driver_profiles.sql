CREATE TABLE associates ( transporter_id TEXT PRIMARY KEY, company_id TEXT NOT NULL, name TEXT NOT NULL, email TEXT, personal_phone TEXT, work_phone TEXT, status TEXT NOT NULL, roles TEXT NOT NULL, qualifications TEXT NOT NULL, license_expires TEXT, photo_address TEXT, listed INTEGER NOT NULL CHECK(listed IN (0,1)), first_listed TEXT NOT NULL, last_listed TEXT NOT NULL, row TEXT NOT NULL );
CREATE TABLE owner_migrations (owner TEXT NOT NULL, id INTEGER NOT NULL, name TEXT NOT NULL, applied_at INTEGER NOT NULL, PRIMARY KEY(owner, id));
CREATE TABLE photos ( address TEXT PRIMARY KEY, sha256 TEXT NOT NULL, format TEXT NOT NULL, bytes INTEGER NOT NULL, stored_at TEXT NOT NULL );
CREATE TABLE roster_runs ( job_id TEXT PRIMARY KEY, company_id TEXT NOT NULL, collected_at TEXT NOT NULL, listed INTEGER NOT NULL, photos INTEGER NOT NULL, photos_waiting INTEGER NOT NULL );
CREATE TABLE schema_migrations (id INTEGER PRIMARY KEY, name TEXT NOT NULL, applied_at INTEGER NOT NULL);
CREATE TABLE storage_identity (dsp_id TEXT NOT NULL, provider TEXT NOT NULL, source TEXT NOT NULL);
