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
  nowhere, and come back as they were. So does an ID a list of people the DSP can reach no
  longer names, as the members of a disconnected workspace: it keeps nobody from another
  account, and comes back with its workspace. A source is one of the DSP's systems once it
  names someone, or someone holds an ID it still could name, so no card says "Not in Slack"
  while a workspace waits for its admin to let Dispatch read its members, or once the last
  one is disconnected.
- **Permission:** `driver_match.manage`, to see everyone's codes and decide who is whom.
- **Its screen** is the Driver Match tab of a DSP's Settings, shown while the DSP has a feature
  on whose data names people: the questions the backend sends, asked one at a time, then
  everyone as a card with their photo, from core's `/api/dsp/people/{code}/photo`, and the
  sources that have them, by role or a source
  lacking them. Its badge counts the same questions. Driver Profiles draws its roster bar above
  it.
- **API**, all behind `driver_match.manage`: `GET /api/dsp/driver-match` (the sources on, the
  questions waiting, as pairs that may be one person or name variants to confirm, by people's
  codes, and everyone), `…/counts` (how many questions wait), `…/people/{code}` (one person's
  IDs, what they appear in, their days and history), and the decisions `POST …/merge` (two codes become one), `…/split` (one ID moves to a new
  person), `…/apart` (two people are kept apart) and `…/confirm` (a match by a name or a
  contact was right), which answer with nothing. A decision made on stale data is refused
  (`person_changed`), as is splitting a person's only ID (`person_single_id`).
- **Codes:** six letters and digits that can't be mistaken for one another, unique across the
  platform (`driver_codes`). A person keeps their code for good; merged, both keep the code
  merged into.
- **Matching:** after every collection that finishes and whenever the people a feature names change
  outside one, as a feature whose data names people being switched on (it hears
  `collection.succeeded` and `people.changed`); and hourly to catch up anything missed. Every ID the
  DSP's data holds gets a code, source by source in their order. A new ID joins someone by, in turn:
  another ID its source says is the same person's, as their account in another workspace, when one
  person holds those and their names agree (`account`); an email address or phone number only the
  two of them have, among every person of either source, when their names agree; a name nobody else
  carries; the same name written another way, as without a second surname or by a middle name. A
  nickname, as Alex for Alexander, joins nobody: it suggests a pair to decide. Email addresses are
  compared in lower case and phone numbers by their ten digits; a number that isn't North American
  is shown but matches nobody. A contact two people of one source share, as a family's phone, joins
  nobody but suggests a rehire. What counts as one name written two ways, and which first names are
  short for others, is `backend/names.rs`: no other owner matches names.
- **Names:** a person is shown by the name their first source writes, in the sources' order,
  so a payroll's legal name comes before Amazon's.
- **Roles:** Amazon's roster says who drives or only helps. Someone no roster lists and
  nothing has seen for 21 days before the newest data has left. A list of people the DSP can
  reach, as Slack's members, says nothing of who still works for it: someone only such lists
  know works in the office while one lists them, and has left once none does. People no
  source shows driving, outside the departments a source's settings count as drivers, work in
  the office.
- **Questions:** two people no source knows both of, whose names tie them, or any two sharing
  a contact only they have, with how many days they worked together; each reason says whether
  it argues for them being one (working fewer than half their days together argues against).
  A list of people the DSP can reach may list one person twice, as an account in each of two
  workspaces under different addresses, so its knowing both keeps no pair apart. The one
  holding fewer IDs joins the other, whose code both keep. Then the IDs a name written another
  way joined to someone. Nobody who has left is asked about.
- **Photos:** a person's photo is the first a kind of data holds under one of their IDs, their
  IDs in the sources' order and each ID's kinds in the order their data is read
  (`backend/photos.rs`). The tab, a person's photo and the directory answer by that one rule,
  so every feature showing people shows the same photo.
- **Log:** its decisions are audited as `driver_match.merged`, `driver_match.split`,
  `driver_match.kept_apart` and `driver_match.confirmed`, naming people by their codes; the
  log puts today's names to them.
- **Storage:** `driver_codes` in the platform's database (platform migration 10) and
  `people`, `person_ids` and `people_apart` in each DSP's (DSP migration 7, and its own
  migration 1, which lets `person_ids` hold any source, and 2, which made the links the
  meal-break page saved before Driver Match its own decisions).

## How it's built

A pass reads, decides and writes, each in its own module, so a new rule goes where its kind
of rule already is:

- `sources.rs` reads what every kind of data the DSP has on says of each ID (`Identity`).
- `matching.rs` decides where each new ID goes, its rules tried in the order `decide` lists;
  `assign.rs` runs a pass and writes the decisions.
- `names.rs` is every rule for comparing names, and `contacts.rs` every kind of contact: a new
  kind, as a badge number, is one more `Contact`.
- `review.rs` gathers why two people might be one, and `pairs.rs` makes those pairs, which
  side joins the other and in what order they're asked.
- `codes.rs` writes and checks codes.
- `gather.rs` reads what the tab, a person's sheet and the badge need, once for each answer;
  `overview.rs` makes the tab and its questions, `details.rs` one person's sheet, `photos.rs`
  photos, and `decisions.rs` the decisions a person makes.

## Connecting a feature

Driver Match names no feature: a feature whose data names people connects itself, in its own
folder, with one declaration in its manifest's `people`. Core's manifest README ("People") is
the guide: what to declare, and what each part tells Driver Match.

### Orders

Each order decides more than where something is listed, so a new kind or source takes its
place here with its reason.

| Source            | Order | Why there                                                                       |
| ----------------- | ----- | ------------------------------------------------------------------------------- |
| Paycom (`paycom`) | 10    | A payroll's legal name is the name a person is shown by.                        |
| Amazon (`amazon`) | 20    | Shown by when no payroll knows them.                                            |
| Slack (`slack`)   | 30    | A list of people the DSP can reach: shown by only when nothing else knows them. |

A person is shown by the first source's name, and listed with their sources in this order.

| Kind                                    | Order | Why there                                                                                 |
| --------------------------------------- | ----- | ----------------------------------------------------------------------------------------- |
| Timecards (`timecards`)                 | 10    | Paycom's roster, its name as payroll has it.                                              |
| Roster (`driver_profiles`)              | 15    | The name Amazon has on file: read first among Amazon's.                                   |
| Routes (`routes`)                       | 20    | Routes spell names most plainly.                                                          |
| Meal breaks (`meal_breaks`)             | 30    | After routes.                                                                             |
| Weekly Scorecard (`weekly_scorecard`)   | 50    | Often adds a middle name.                                                                 |
| Daily Performance (`daily_performance`) | 60    |                                                                                           |
| Slack (`slack`)                         | 70    |                                                                                           |
| DVIC (`dvic`)                           | 90    | Sometimes writes only a last initial, so its spellings are read after every other kind's. |

An ID's spellings are read kind by kind in this order, and the first is the one it is shown
by; its department, position and photo come from the first kind that has them. Each person's
sheet lists their kinds in this order.

## Asking who someone is

A feature that shows people, or uses what is known of them, asks core's directory, never Driver
Match, which exposes it (`backend/directory.rs`): everyone as the tab lists them,
each with their code, the name they are shown by and every other way their sources write it,
their role, every ID they hold by source, their email addresses and phone numbers, whether
someone confirmed who they are and the version of their photo; and nobody in a build without
Driver Match. Core keeps it until a decision or a collection changes it (`people::everyone`), and
serves it to every member of the DSP. Timecard's employees name their person by it, and its meal
breaks find each driver's employee (`people.holding(cortex::SOURCE, id)`, then
`person.ids_of(dispatch_paycom::SOURCE)`).
