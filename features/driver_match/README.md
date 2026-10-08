# Driver Match

One code per person across Paycom and every Amazon source. After each collection a new ID
joins someone only on a certain match; anything less gets a person of its own and waits for a
decision in its Settings tab. Collected rows keep their sources' IDs, and Driver Match stores
only what was decided.

- **Switch:** `driver_match`, off for every DSP until the platform owner switches it on. It
  needs the timecards and routes connections (Paycom and Cortex), which switch on with it.
  Timecard depends on it, to name drivers on the meal-break page.
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
  kind of data it is in its `people` (`PeopleData`), and Driver Match reads them all, in their
  order. Someone only one source knows, off Paycom's roster and unseen for 21 days before the
  newest data, counts as having left. People outside the departments a source's settings count
  as drivers work in the office.
- **Log:** its decisions are audited as `driver_match.merged`, `driver_match.split` and
  `driver_match.kept_apart`, naming drivers by their codes; the log puts today's names to them.
- **Agents:** it fills core's `identity`, as "Driver Match", so agents may name a driver by
  their code and every answer names drivers by it.
- **Storage:** `driver_codes` in the platform's database (platform migration 10) and
  `people`, `person_ids` and `people_apart` in each DSP's (DSP migration 7). Links saved on
  the meal-break page before Driver Match are read as decisions.
