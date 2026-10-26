-- Whether a number was ever a work phone, found by the phone: the holds are kept by day and
-- driver, so without it the lookup read every day's holds the retention window keeps.
CREATE INDEX IF NOT EXISTS work_phones_phone ON work_phones(phone);
