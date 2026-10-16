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

One code per person across every source the DSP's features name people by: Paycom's employee
codes, Amazon's transporter IDs and any source a feature declares. After each collection a new
ID joins someone only on a certain match; anything less gets a person of its own and waits for
a decision in its Settings tab. Collected rows keep their sources' IDs, and Driver Match stores
only what was decided.

- **Switch:** `driver_match`, a part of Driver Profiles, on with it unless switched off. It
  needs no connection of its own: it matches whatever the DSP's features name. A DSP that had
  Driver Match on before it became a part has Driver Profiles switched on for it, once.
- **Only what is on takes part.** It reads the people of the features the DSP has on and
  nothing else, and does nothing while it is off itself. A source no feature on names sits
  idle: its IDs keep their people, shown nowhere, and come back as they were.
- **Permission:** `driver_match.manage`, to see everyone's codes and decide who is whom.
- **Its screen** is the Driver Match tab of a DSP's Settings: the questions waiting, the pairs
  to decide and the names written another way to confirm, asked one at a time; the roster's
  last and next collection (for those who hold `driver_profiles.collect`); then everyone as a
  card with their photo and the sources that have them, by role or a source lacking them.
- **API**, all behind `driver_match.manage`: `GET /api/dsp/driver-match` (the sources on,
  everyone, and the pairs that may be one person), `…/counts`, `…/drivers/{code}` (one
  person's IDs, days and activity), `…/drivers/{code}/photo` (their photo from Amazon's roster),
  and the decisions `POST …/merge` (two codes become one), `…/split` (one ID moves to a new
  person), `…/apart` (two people are kept apart) and `…/confirm` (a match's link is right). A
  decision made on stale data is refused (`driver_changed`), as is splitting a person's only ID
  (`driver_single_id`).
- **Codes:** six letters and digits that can't be mistaken for one another, unique across the
  platform (`driver_codes`). A person keeps their code for good; merged, both keep the code
  merged into.
- **Matching:** after every collection that finishes, and hourly to catch up anything missed,
  every ID the DSP's data holds gets a code, source by source in their order. A new ID joins
  someone by, in turn: a link saved on the meal-break page; an email address or phone number
  only the two of them have, among every ID of either source, when their names agree; a name
  nobody else carries; the same name written another way, as without a second surname or by a
  middle name. A nickname joins nobody: it suggests a pair to decide. Email addresses are
  compared in lower case and phone numbers by their ten digits. A contact two IDs of one source
  share, as a family's phone, joins nobody but suggests a rehire.
- **Sources and kinds of data:** each feature whose data names people declares the kind of
  data it is in its `people` (`PeopleData`: its id, order, label, unit and icon) and the source
  of its IDs (`SourceData`, declared by whoever owns those IDs: Cortex for Amazon's, Paycom for
  its own). Driver Match reads them from the generated `driver-data.ts`, and names none
  itself, so a build without a feature lists them nowhere.
- **Roles:** Amazon's roster says who drives or only helps. Someone no source lists and
  nothing has seen for 21 days before the newest data has left. People no source shows driving,
  outside the departments a source's settings count as drivers, work in the office.
- **Review:** two people no source knows both of, whose names tie them, or any two sharing a
  contact only they have, with how many days they worked together. The one holding fewer
  IDs joins the other, whose code both keep.
- **Directory:** it answers core's directory, so a feature showing people asks who an ID is
  without depending on it: Timecard's meal breaks name each driver's employee by it. Off, it
  answers nobody.
- **Log:** its decisions are audited as `driver_match.merged`, `driver_match.split`,
  `driver_match.kept_apart` and `driver_match.confirmed`, naming drivers by their codes; the
  log puts today's names to them.
- **Storage:** `driver_codes` in the platform's database (platform migration 10) and
  `people`, `person_ids` and `people_apart` in each DSP's (DSP migration 7, and its own
  migration 1, which lets `person_ids` hold any source). Links saved on the meal-break page
  before Driver Match are read as decisions.
