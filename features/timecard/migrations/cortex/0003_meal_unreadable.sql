-- The routes whose meal punches broke a rule the meal rules read them by, with the rule's
-- name: such a route's meals are unknown, not absent, and only it goes without them.
CREATE TABLE IF NOT EXISTS meal_unreadable (
  publication_id TEXT NOT NULL, itinerary_id TEXT NOT NULL,
  rule TEXT NOT NULL CHECK(length(rule) BETWEEN 1 AND 64),
  PRIMARY KEY(publication_id,itinerary_id),
  FOREIGN KEY(publication_id,itinerary_id) REFERENCES meal_itineraries(publication_id,itinerary_id) ON DELETE CASCADE
);
