-- The DSP's people who see what is in beta there, as the platform owner chose them, and since
-- when. Each goes with their account.
CREATE TABLE IF NOT EXISTS beta_testers (
    user_id TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    added_at TEXT NOT NULL
);
