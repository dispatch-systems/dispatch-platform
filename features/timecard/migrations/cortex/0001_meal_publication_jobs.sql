-- A day's bundled scopes keep independent minimized publications, while their public
-- collection identity remains the actual queued job. Previous runtimes ignore this table.
CREATE TABLE IF NOT EXISTS meal_publication_jobs (
  publication_key TEXT NOT NULL PRIMARY KEY REFERENCES meal_publications(job_id) ON DELETE CASCADE,
  job_id TEXT NOT NULL,
  scope_index INTEGER NOT NULL CHECK(scope_index>=0),
  UNIQUE(job_id,scope_index)
);
