# Driver Profiles

Everyone Amazon lists for the DSP's company, active or not, with their contacts, roles,
qualifications, license expiry and photo, collected from Cortex's associates page (Amazon's DA
Console). Driver Match reads them as it reads every feature's people. It has no page of its own:
the roster is collected, on demand and by its rules, from Settings' Collectors page.

- **Switch:** `driver_profiles`, off for every DSP until the platform owner switches it on. It
  needs the `associates` connection (Cortex), which switches on with it. It began as Driver
  Match's home, so a DSP that had Driver Match on before it existed has it switched on, once.
- **Permission:** `driver_profiles.collect`, to see when the roster was collected, collect it
  on demand and set when it's collected, on the Collectors page (its keeper's `permission` and
  `schedules`).
- **API:** `POST /api/dsp/driver-profiles/jobs/{id}/cancel`, behind `driver_profiles.collect`.
  The roster is collected, and its schedule set, as the `associates` collection on core's
  Collectors routes.
- **Collection:** Cortex's `associates` reads the company whose short code is the DSP's
  abbreviation, then its whole roster in one request over HTTP, then downloads the photos the
  DSP lacks or holds for over 30 days, about sixty at most a run; the next brings the rest. A
  roster naming no one is refused rather than taken to mean everyone left.
- **Schedules:** only the DSP's own, set on the Collectors page; Driver Profiles makes none.
- **Data:** `associates`, `photos` and `roster_runs`, in a database of its own beside
  Cortex's. Someone the roster stops listing keeps their row, with `listed` 0, so what names
  them elsewhere still finds a name.
- **Photos** are files in the DSP's cache, `cache/driver_profiles/photos/<sha256>.<ext>`.
  Backups leave the cache out, as a restore collects them again, and they go with the DSP's
  folder. A photo is deleted once Amazon no longer lists whoever it shows; an inactive
  associate Amazon still lists keeps theirs.
- **People:** its `Roster` names everyone Amazon has listed by the name Amazon has on file,
  read first among Amazon's spellings; someone counts as listed while the roster lists them
  as active. It names each listed associate's photo by its digest and serves it, so Driver
  Match shows it.
- **Log:** `driver_profiles.collection_requested`, under Collections, which its own Collect now
  wrote before the Collectors page collected the roster; it's worded still, for those entries.
- **Exposed:** `amazon_roster`, through its contract in `contract/`
  (`dispatch-driver-profiles-contract`), for any feature or agent tool, while it's on for the
  DSP: when the roster was last collected, or none before the first, and for the transporter
  IDs asked, in their order, whether the roster still lists each, their status, roles,
  qualifications, license expiry and when it last listed them. It leaves out an ID it never
  listed, and refuses an empty ID, one over 128 characters or more than 32 IDs with
  `invalid_input`. Names, contacts and photos are the directory's; it never offers the row as
  Amazon sent it, the company or the work phone.
