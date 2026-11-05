-- Each day whose route data changed where nothing was at hand to tell of it, as a collection or a
-- reprocess keeps it, until it's told (`routes.data_changed`): kept here rather than in the
-- server's memory, so a restart before then tells of it still. One row each time it's noted, by an
-- ID never given again, so telling lets go only of the notes it read: one made since stays. The
-- previous release never had it.
CREATE TABLE IF NOT EXISTS route_days_untold (
    id INTEGER PRIMARY KEY,
    day TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS route_days_untold_day ON route_days_untold (day, id);

-- The last note's ID given, so none is given again once its note is let go.
CREATE TABLE IF NOT EXISTS route_days_noted (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    last INTEGER NOT NULL
);
INSERT OR IGNORE INTO route_days_noted (id, last) VALUES (1, 0);
