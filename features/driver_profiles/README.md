# Driver Profiles

Everyone Amazon lists for the DSP's company, active or not, with their contacts, roles,
qualifications, license expiry and photo, collected from Cortex's associates page (Amazon's DA
Console). It has no page of its own yet: it collects and keeps, and Driver Match reads who it
names.

- **Switch:** `driver_profiles`, off for every DSP until the platform owner switches it on. It
  needs the `associates` connection (Cortex), which switches on with it.
- **Permission:** `driver_profiles.collect`, to see when the roster was collected, collect it
  on demand and change its schedules.
- **API**, all behind `driver_profiles.collect`: `GET /api/dsp/driver-profiles/status` (when
  it was collected, how many are active and inactive, the photos kept and waiting, the next
  scheduled run and the recent jobs), `POST …/collect`, `POST …/jobs/{id}/cancel`, and core's
  schedule routes under `…/schedules` for the `associates` collection.
- **Collection:** Cortex's `associates` reads the company whose short code is the DSP's
  abbreviation, then its whole roster in one request over HTTP, then downloads the photos the
  DSP lacks or holds for over 30 days, about sixty at most a run; the next brings the rest. A
  roster naming no one is refused rather than taken to mean everyone left.
- **Schedule:** each DSP that has it gets one schedule, `Daily roster` at 05:00 its time, as
  soon as Cortex is connected and the DSP has an abbreviation. It is made once: changed or
  removed, it stays as the DSP left it.
- **Data:** `associates`, `photos`, `roster_runs` and `settings`, in a database of its own
  beside Cortex's. Someone the roster stops listing keeps their row, with `listed` 0, so what
  names them elsewhere still finds a name.
- **Photos** are files in the DSP's cache, `cache/driver_profiles/photos/<sha256>.<ext>`.
  Backups leave the cache out, as a restore collects them again, and they go with the DSP's
  folder. A photo is deleted once Amazon no longer lists whoever it shows; an inactive
  associate Amazon still lists keeps theirs.
- **People:** its `Roster` names everyone Amazon has listed by the name Amazon has on file,
  read first among Amazon's spellings; someone counts as listed while the roster lists them
  as active.
- **Log:** `driver_profiles.collection_requested`, under Collections.
