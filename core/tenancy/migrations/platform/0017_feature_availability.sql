-- Which features and parts were mandatory when the server last started, so one made optional
-- later can be switched on for every DSP that had it, once, rather than taken from them all.
CREATE TABLE IF NOT EXISTS feature_availability (
    feature TEXT PRIMARY KEY,
    mandatory INTEGER NOT NULL CHECK (mandatory IN (0, 1))
);
