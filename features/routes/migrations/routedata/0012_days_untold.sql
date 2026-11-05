-- Each day whose route data changed where nothing was at hand to tell of it, as a collection or a
-- reprocess keeps it, until it's told (`routes.data_changed`): kept here rather than in the
-- server's memory, so a restart before then tells of it still. `noted` counts each time it was
-- noted, so one noted again while it's told stays to be told again. The previous release never had
-- it.
CREATE TABLE IF NOT EXISTS route_days_untold (
    day TEXT PRIMARY KEY,
    noted INTEGER NOT NULL
);
