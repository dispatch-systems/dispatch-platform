# Driver Match

One code per person across every source the DSP's features name people by: Paycom's employee
codes, Amazon's transporter IDs and any source a feature declares. After each collection a new
ID joins someone only on a certain match; anything less gets a person of its own and waits for
a decision in its Settings tab. Collected rows keep their sources' IDs, and Driver Match stores
only what was decided.

- **Switch:** `driver_match`, mandatory: every DSP has it, with no switch to turn it off. It
  needs no connection: it matches whatever the DSP's features name.
- **Only what is on takes part.** It reads the people of the features the DSP has on and
  nothing else. A source no feature on names sits idle: its IDs keep their people, shown
  nowhere, and come back as they were. A source is one of the DSP's systems once it has named
  someone, so no card says "Not in Slack" while a workspace waits for its admin to let
  Dispatch read its members.
- **Permission:** `driver_match.manage`, to see everyone's codes and decide who is whom.
- **Its screen** is the Driver Match tab of a DSP's Settings, shown while the DSP has a feature
  on whose data names people: the questions waiting, the pairs to decide and the names written
  another way to confirm, asked one at a time; then everyone as a card with their photo and
  the sources that have them, by role or a source lacking them. Driver Profiles draws its
  roster bar above it.
- **API**, all behind `driver_match.manage`: `GET /api/dsp/driver-match` (the sources on,
  everyone, and the pairs that may be one person), `…/counts`, `…/drivers/{code}` (one
  person's IDs, days and activity), `…/drivers/{code}/photo` (their photo), and the decisions
  `POST …/merge` (two codes become one), `…/split` (one ID moves to a new person), `…/apart`
  (two people are kept apart) and `…/confirm` (a match's link is right). A decision made on
  stale data is refused (`driver_changed`), as is splitting a person's only ID
  (`driver_single_id`).
- **Codes:** six letters and digits that can't be mistaken for one another, unique across the
  platform (`driver_codes`). A person keeps their code for good; merged, both keep the code
  merged into.
- **Matching:** after every collection that finishes, and hourly to catch up anything missed,
  every ID the DSP's data holds gets a code, source by source in their order. A new ID joins
  someone by, in turn: another ID its source says is the same person's, as their account in
  another workspace, whoever holds it (`account`); a link saved on the meal-break page; an
  email address or phone number only the two of them have, among every person of either
  source, when their names agree; a name
  nobody else carries; the same name written another way, as without a second surname or by a
  middle name. A nickname joins nobody: it suggests a pair to decide. Email addresses are
  compared in lower case and phone numbers by their ten digits. A contact two people of one
  source share, as a family's phone, joins nobody but suggests a rehire. What counts as one name
  written two ways, and which first names are short for others, is `backend/names.rs`: no other
  owner matches names.
- **Names:** a person is shown by the name their first source writes, in the sources' order,
  so a payroll's legal name comes before Amazon's.
- **Roles:** Amazon's roster says who drives or only helps. Someone no roster lists and
  nothing has seen for 21 days before the newest data has left. A list of people the DSP can
  reach, as Slack's members, says nothing of who still works for it: someone only such lists
  know works in the office while one lists them, and has left once none does. People no
  source shows driving, outside the departments a source's settings count as drivers, work in
  the office.
- **Review:** two people no source knows both of, whose names tie them, or any two sharing a
  contact only they have, with how many days they worked together. The one holding fewer
  IDs joins the other, whose code both keep.
- **Photos:** a person's photo is the first a connection holds under one of their IDs.
- **Log:** its decisions are audited as `driver_match.merged`, `driver_match.split`,
  `driver_match.kept_apart` and `driver_match.confirmed`, naming drivers by their codes; the
  log puts today's names to them.
- **Storage:** `driver_codes` in the platform's database (platform migration 10) and
  `people`, `person_ids` and `people_apart` in each DSP's (DSP migration 7, and its own
  migration 1, which lets `person_ids` hold any source). Links saved on the meal-break page
  before Driver Match are read as decisions.

## Connecting a feature

Driver Match names no feature: a feature whose data names people connects itself, in its own
folder, with one declaration in its manifest's `people` (`dispatch_core::manifest::people`).
`dispatchdev new feature <name> --tables dsp --people amazon` writes one to start from.

```rust
// backend/people.rs
pub const PEOPLE: People = People::new(
    PeopleData { id: "fuel", order: 70, label: "Fuel", unit: "fill-up", icon: "Fuel" },
    &dispatch_cortex::PEOPLE,
    |store, dsp| {
        people::named(
            &*store.fuel_db(dsp)?,
            "SELECT transporter_id id,driver_name name,min(day) first,max(day) last \
             FROM fill_ups GROUP BY 1,2 ORDER BY last DESC",
        )
    },
)
.records(|store, dsp| {
    people::records(
        &*store.fuel_db(dsp)?,
        "SELECT transporter_id id,count(*) count,max(day) last FROM fill_ups GROUP BY 1",
    )
});

// feature.rs
people: &[&backend::people::PEOPLE],
```

- **What it is** (`PeopleData`): its id, its place among the kinds (`order`, the next free
  tens), its label, what one record is (`unit`) and its icon, by the name lucide-react exports
  it as. Each person's sheet lists it so.
- **Whose IDs** (`SourceData`): the source its people are named by, declared by whoever owns
  those IDs: `&dispatch_cortex::PEOPLE` for Amazon's transporter IDs, `&dispatch_paycom::PEOPLE`
  for Paycom's employee codes, or a source of its own. The sources' `order` is the order they
  are listed in, and whose name a person is shown by.
- **Who it names** (`read`): one `Named` for each ID and each way it writes their name, the
  newest first. `people::named` reads them from a query by its columns' names: `id` and `name`,
  and any of `first` and `last` (the days it saw them), `listed` (1 while its source's roster
  lists them), `email`, `phone`, `roles` (a JSON list, as `["Driver"]`), `department`,
  `position` and `photo` (the photo's version). Someone a source knows by several IDs, as an
  account in each of several workspaces, is named by each, with the others in `same_as`: every
  one goes to the same person, whichever came first.
- **How much it holds** (`.records`): `id`, `count` and `last` for each ID, counted in its
  unit. Without it the kind is listed for nobody, and its data shows no one at work.
- **When they worked** (`.workdays`): each ID with each day it worked, and the days the data
  was collected for (`Workdays::daily` when every day it holds was collected), so two people
  who never worked the same days are less likely one.
- **Which departments drive** (`.driver_departments`): from the feature's settings; anyone it
  names outside them works in the office.
- **Photos** (`.photos`): serves the photo of the first of some IDs it holds one of, each named
  with its version in `read`.
- **Shortened names** (`.initials()`): it may write a last name as an initial, so its
  spellings are read after every other kind's.
- **People it can reach, not employs** (`.contacts_only()`): being on its list never keeps
  someone from having left, and someone only such lists know works in the office, as Slack's
  members.

A feature whose people change outside a collection, as Slack's members do, calls
`people::changed(state, dsp)` once they're kept, and they're told apart at once rather than
within the hour.

Nothing else changes: Driver Match reads the kind while its feature is on, every ID it names
gets a person, its Settings tab shows while one such feature is on, and the kind joins the
generated `driver-data.ts` and `people-features.ts` with `npm run contracts:generate`. The app's
people test (`app/tests/backend/people.rs`) runs every connection's queries against a new DSP,
so a connection is tried without a test of its own; the registry refuses two kinds with one id
or one order.

## Asking who someone is

A feature that shows people asks core, never Driver Match: `people::who(store, dsp, source)`
answers, for each ID of `source` someone holds, their code, every ID they hold by source, and
whether someone confirmed who they are, and nobody in a build without Driver Match. Timecard's
meal breaks find each driver's employee so (`person.ids_of(PAYCOM)`).
