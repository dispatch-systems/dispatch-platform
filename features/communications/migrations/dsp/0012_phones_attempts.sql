-- Each earlier attempt at a text Dispatch sent, by the phone it was given to, kept as Retry queues
-- a new one, which may go from another phone: a delivery report of an earlier attempt counts only
-- from the phone that had it. Kept, as the rest, until the DSP is deleted. Shipped migrations
-- never change: add the next one instead.
CREATE TABLE IF NOT EXISTS phones_attempts (
    text_id TEXT NOT NULL REFERENCES phones_texts (id),
    attempt INTEGER NOT NULL,
    device_id TEXT NOT NULL REFERENCES phones_devices (id),
    PRIMARY KEY (text_id, attempt)
);
