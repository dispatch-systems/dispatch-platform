# Driver Profiles

Everyone Amazon lists for the DSP's company, active or not, with their contacts, roles,
qualifications, license expiry and photo, collected from Cortex's associates page (Amazon's DA
Console). Driver Match reads them as it reads every feature's people. It has no page of its own:
its roster bar and its schedules are on its Settings page.

- **Switch:** `driver_profiles`, off for every DSP until the platform owner switches it on. It
  needs the `associates` connection (Cortex), which switches on with it. It began as Driver
  Match's home, so a DSP that had Driver Match on before it existed has it switched on, once.
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
- **Schedules:** only the DSP's own, added on its Settings page; Driver Profiles makes none.
- **Data:** `associates`, `photos` and `roster_runs`, in a database of its own
  beside Cortex's. Someone the roster stops listing keeps their row, with `listed` 0, so what
  names them elsewhere still finds a name.
- **Photos** are files in the DSP's cache, `cache/driver_profiles/photos/<sha256>.<ext>`.
  Backups leave the cache out, as a restore collects them again, and they go with the DSP's
  folder. A photo is deleted once Amazon no longer lists whoever it shows; an inactive
  associate Amazon still lists keeps theirs.
- **Settings:** its own page (`?tab=driver-profiles`), for those who hold
  `driver_profiles.collect` (`backend/settings.rs`): `driver_profiles.roster` "Amazon roster", a
  custom section its roster bar draws (when the roster was collected and is collected next, and
  Collect now), then `driver_profiles.collection` "Collection", the `associates` schedules, kept
  by core's schedule routes under `…/schedules`. It stores no values of its own there.
- **People:** its `Roster` names everyone Amazon has listed by the name Amazon has on file,
  read first among Amazon's spellings; someone counts as listed while the roster lists them
  as active. It names each listed associate's photo by its digest and serves it, so Driver
  Match shows it.
- **Log:** `driver_profiles.collection_requested`, under Collections.
- **Exposed:** `amazon_roster`, through its contract in `contract/`
  (`dispatch-driver-profiles-contract`), for any feature or agent tool, while it's on for the
  DSP: when the roster was last collected, or none before the first, and for the transporter
  IDs asked, in their order, whether the roster still lists each, their status, roles,
  qualifications, license expiry and when it last listed them. It leaves out an ID it never
  listed, and refuses an empty ID, one over 128 characters or more than 32 IDs with
  `invalid_input`. Names, contacts and photos are the directory's; it never offers the row as
  Amazon sent it, the company or the work phone.
