-- Each request for a demo the public site's form sent: who asked, for which DSP and what they
-- wrote, and where the platform owner stands with it. `submission` is the site's own ID for it,
-- so one sent again is kept once.
CREATE TABLE IF NOT EXISTS demo_requests (
    id TEXT PRIMARY KEY,
    submission TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    email TEXT NOT NULL,
    dsp_name TEXT NOT NULL,
    message TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'new',
    received_at TEXT NOT NULL
);
