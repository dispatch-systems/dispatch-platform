-- What a read sends of the drivers and addresses the days share, kept with its publication
-- until it becomes its day's read, which writes them over the shared rows: a read that never
-- does, collected before the day's own or left unfinished, changes none of them.
CREATE TABLE IF NOT EXISTS staged_addresses (
  publication_id TEXT NOT NULL REFERENCES route_publications(id) ON DELETE CASCADE,
  address_id TEXT NOT NULL, address1 TEXT, address2 TEXT, address3 TEXT,
  city TEXT, state TEXT, postal_code TEXT, latitude REAL, longitude REAL,
  PRIMARY KEY(publication_id,address_id)
);
CREATE TABLE IF NOT EXISTS staged_drivers (
  publication_id TEXT NOT NULL REFERENCES route_publications(id) ON DELETE CASCADE,
  transporter_id TEXT NOT NULL, first_name TEXT, last_name TEXT, initials TEXT, work_phone TEXT,
  person_type TEXT NOT NULL CHECK(json_valid(person_type)), company_id TEXT,
  PRIMARY KEY(publication_id,transporter_id)
);
