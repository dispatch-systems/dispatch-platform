-- Whether a number was ever a work phone, found by the phone among the itineraries that show one:
-- kept by publication and itinerary, they'd otherwise be read for every day the window keeps.
CREATE INDEX IF NOT EXISTS itineraries_work_phone ON itineraries(work_phone) WHERE work_phone IS NOT NULL;
