-- Pick driver: whom someone at the DSP said held a work phone on a day, when live route data
-- showed nobody holding it as its texts were written. That day's texts with the phone are filed
-- under the driver picked, but where a hold covers one. Kept, as the rest, until the DSP is
-- deleted. Shipped migrations never change: add the next one instead.

-- The work phone, by its ten digits as Routes keeps work phones; the DSP's day, YYYY-MM-DD; and
-- the driver picked, by their ID of the source naming who held the DSP's work phones, as a
-- driver's conversation is keyed. One pick a phone a day: picking again replaces it.
CREATE TABLE IF NOT EXISTS communications_picked (
    phone TEXT NOT NULL,
    day TEXT NOT NULL,
    source TEXT NOT NULL,
    driver_id TEXT NOT NULL,
    picked_by TEXT NOT NULL,
    picked_at INTEGER NOT NULL,
    PRIMARY KEY (phone, day)
);
