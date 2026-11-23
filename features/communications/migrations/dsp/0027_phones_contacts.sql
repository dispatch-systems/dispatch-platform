-- Communications' migration 27: each phone's contacts, as Dispatch knows them, for a line that
-- keeps its contacts on its phones (every line but Dispatch Phone's own): each number the phone's
-- contacts hold, by its name there, and when it last changed (`changed_at`); or a name someone at
-- the DSP saved for it, to be handed to the phone, until the phone says it saved it (`saved_at`,
-- when it was saved), and when it was last handed (`handed_at`). The names another line saved
-- before go to each of its phones so, and stay in `communications_line_contacts`, which the
-- release before reads. Each step is taken only if it wasn't. Shipped migrations never change:
-- add the next one instead.

CREATE TABLE IF NOT EXISTS phones_contacts (
    device_id TEXT NOT NULL,
    number TEXT NOT NULL,
    name TEXT NOT NULL,
    changed_at INTEGER NOT NULL,
    saved_at INTEGER,
    handed_at INTEGER,
    PRIMARY KEY (device_id, number)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS phones_contacts_saved
    ON phones_contacts (device_id, saved_at) WHERE saved_at IS NOT NULL;

INSERT OR IGNORE INTO phones_contacts (device_id, number, name, changed_at, saved_at)
SELECT d.id, c.number, c.name, c.saved_at, c.saved_at
FROM communications_line_contacts c
JOIN phones_devices d ON d.line = c.line AND d.removed_at IS NULL;
