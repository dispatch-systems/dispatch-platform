-- Each driver's holds on the company's work phones a day: which phone, from when, and until
-- when once it ended, in epoch milliseconds. A day's are its read's, replaced with it.
CREATE TABLE IF NOT EXISTS work_phones (
    day TEXT NOT NULL,
    transporter_id TEXT NOT NULL,
    phone TEXT NOT NULL,
    since INTEGER NOT NULL,
    until INTEGER,
    PRIMARY KEY(day, transporter_id, phone, since)
) WITHOUT ROWID;
