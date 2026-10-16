# Driver Profiles

Everyone Amazon lists for the DSP's company, active or not, with their contacts, roles,
qualifications, license expiry and photo, collected from Cortex's associates page (Amazon's DA
Console), and Driver Match, its part that gives each person one code across every source that
names them. It has no page of its own yet: Driver Match's Settings tab is its one screen.

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

## Driver Match

One code per person across Paycom and every Amazon source. After each collection a new ID
joins someone only on a certain match; anything less gets a person of its own and waits for a
decision in its Settings tab. Collected rows keep their sources' IDs, and Driver Match stores
only what was decided.

- **Switch:** `driver_match`, a part of Driver Profiles, on with it unless switched off. It
  needs the timecards and routes connections (Paycom and Cortex), which switch on with it.
  Timecard depends on it, to name drivers on the meal-break page. A DSP that had Driver
  Match on before it became a part has Driver Profiles switched on for it, once.
- **Permission:** `driver_match.manage`, to see everyone's codes and decide who is whom.
- **Its screen** is the Driver Match tab of a DSP's Settings, shown to those who hold the
  permission. Like every Settings tab, it appears only where a feature hosts Settings, which
  the mandatory Settings feature does.
- **API**, all behind `driver_match.manage`: `GET /api/dsp/driver-match` (everyone, and the
  pairs that may be one person), `…/counts`, `…/drivers/{code}` (one person's IDs, days and
  activity), and the decisions `POST …/merge` (two codes become one), `…/split` (one ID moves
  to a new person) and `…/apart` (two people are kept apart). A decision made on stale data is
  refused (`driver_changed`), as is splitting a person's only ID (`driver_single_id`).
- **Codes:** six letters and digits that can't be mistaken for one another, unique across the
  platform (`driver_codes`). A person keeps their code for good; merged, both keep the code
  merged into.
- **Matching:** after every collection that finishes, and hourly to catch up anything missed,
  every ID the DSP's data holds gets a code. Each feature whose data names people declares the
  kind of data it is in its `people` (`PeopleData`: its id, order, label, unit and icon).
  Driver Match reads them all, in their order, from the generated `driver-data.ts`, and names
  no kind itself, so a build without a feature lists its kinds nowhere. Someone only one source knows, off Paycom's roster and unseen for 21 days before the
  newest data, counts as having left. People outside the departments a source's settings count
  as drivers work in the office.
- **Log:** its decisions are audited as `driver_match.merged`, `driver_match.split` and
  `driver_match.kept_apart`, naming drivers by their codes; the log puts today's names to them.
- **Storage:** `driver_codes` in the platform's database (platform migration 10) and
  `people`, `person_ids` and `people_apart` in each DSP's (DSP migration 7). Links saved on
  the meal-break page before Driver Match are read as decisions.
