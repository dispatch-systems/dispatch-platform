# Schedules

Every shift Amazon's Scheduling lists for the DSP's drivers, collected from Cortex's week view,
with every change to one kept: who was on which shift, a backup or time off, on each day, and
when that changed. Internally it is `shifts`; the dashboard calls it Schedules. It has no page
of its own yet: the week is collected, on demand and by its rules, from Settings' Collectors
page.

- **Switch:** `shifts`, off for every DSP until the platform owner switches it on. It needs the
  `shifts` connection (Cortex), which switches on with it.
- **Permission:** `shifts.collect`, to see when schedules were collected, collect them on demand
  and set when they're collected, on the Collectors page (its keeper's `permission` and
  `schedules`).
- **API:** `POST /api/dsp/shifts/jobs/{id}/cancel`, behind `shifts.collect`. The week is
  collected, and its schedule set, as the `shifts` collection on core's Collectors routes; what
  it collects, as the Collectors page shows under it, is `frontend/collected.ts` (the
  `collected` slot).
- **Collection:** Cortex's `shifts` reads the Sunday-to-Saturday week holding the day it runs
  at the DSP's station, and no other: no backfill, no week ahead. It reads the roster the week
  view draws, `/schedulingV2/home/api/v4/rosters`, over plain HTTP with the session the job's
  browser started from, at the station's service area, which discovery finds once and keeps
  for the next collections, as Daily Performance's does. The roster answers for the signed-in
  account at that service area, so it is read only for an account that sees one company, the
  one whose short code is the DSP's abbreviation, and each row must name that service area. A
  week naming no assignment is refused rather than taken to mean every shift was removed. The
  week view's spreadsheet download is built in the page from this same roster: its "Shifts &
  Availability" sheet is these assignments. Its "Rostered Work Blocks", and drivers' emails and
  phones, are read but not kept.
- **Measured** on a live DSP's session (127 drivers, 482 assignments): one read of about 1.1 s
  and 528 KB. Two weeks answer as fast; four are refused.
- **Schedules:** only the DSP's own, set on the Collectors page; Schedules makes none.
- **Data:** in a database of its own beside Cortex's:
  - `shift_assignments`, each assignment by Amazon's id (`mappingId`), as the newest read of its
    week had it: the driver by transporter ID, with their name and the DSP's ID for them, the
    day, the shift, whether they're on it, a backup or off (`kind`), its start and length in
    minutes, the note, and the assignment as Amazon sent it. One a later read of its week no
    longer lists keeps its row, with `listed` 0.
  - `shift_changes`, every assignment a read found added, changed or removed, with what it was
    before and after. A change in a driver's name alone is no change.
  - `shift_runs`, each read: the week, the station's service area, and how many drivers,
    assignments and changes it found. A read older than its week's newest, finishing last,
    changes nothing.
- **Log:** none of its own: Collect now is core's, on the Collectors page.
