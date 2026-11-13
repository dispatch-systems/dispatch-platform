-- No read finds tasks by their address: tasks_address served none, and only slowed every write
-- of a task. The previous release reads no differently without it.
DROP INDEX IF EXISTS tasks_address;

-- Where each package is on a day whose delivered and picked-up task rows were let go
-- (`route_publications.tasks_pruned`, which `add_old_days` adds beside this): the itineraries whose response names it, so a package is
-- found across the days without its task rows. Written as a day's rows are let go, for those
-- rows alone; a day that keeps its rows has none here.
CREATE TABLE IF NOT EXISTS package_days (
    tracking_id TEXT NOT NULL,
    publication_id TEXT NOT NULL REFERENCES route_publications(id) ON DELETE CASCADE,
    itinerary_id TEXT NOT NULL,
    PRIMARY KEY (tracking_id, publication_id, itinerary_id)
);
CREATE INDEX IF NOT EXISTS package_days_publication ON package_days (publication_id);
