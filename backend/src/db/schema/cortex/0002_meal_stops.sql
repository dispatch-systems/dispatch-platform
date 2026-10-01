-- The stops that held each meal's last delivery before it and first delivery after it, by
-- their place in the route from 0, as Cortex's itinerary page names a stop in its address.
-- Records published before this have none, and their deliveries link to the route alone.
CREATE TABLE IF NOT EXISTS meal_stops (
  publication_id TEXT NOT NULL, itinerary_id TEXT NOT NULL, meal_id TEXT NOT NULL,
  last_delivery_stop INTEGER CHECK(last_delivery_stop BETWEEN 0 AND 1999),
  first_delivery_stop INTEGER CHECK(first_delivery_stop BETWEEN 0 AND 1999),
  PRIMARY KEY(publication_id,itinerary_id,meal_id),
  FOREIGN KEY(publication_id,itinerary_id,meal_id) REFERENCES meal_records(publication_id,itinerary_id,meal_id) ON DELETE CASCADE
);
