# Manifest

What a feature or a collector declares, and the one registry the app builds from them and
installs at startup. Core reaches features and collectors only through `registry()`, never by
name: what a feature adds reaches core through a slot, a field of `Feature` it fills in its
`feature.rs`, and the frontend's slots, fields of `FrontendFeature` it fills in its
`frontend/feature.ts`. Every slot is listed below, with the module that holds it.

## A slot

Each slot is a module of its own: `backend/<slot>.rs` here, and
`core/shell/frontend/runtime/slots/<slot>.ts` for the frontend. A backend slot's module holds
the slot's types and the builders a manifest writes it with, what the registry answers of it
(`impl Registry`), and `check`, what must hold of every feature's declaration when the registry
is installed, which `Registry::check` calls. A frontend slot's module declares what an owner
puts in it, adds its field to `FrontendFeature` by declaration merging, and exports what the
hosts read it with.

Adding a slot, which `dispatchdev new slot <name>` writes as far as step 3:

1. Backend: its module here, its field in `Feature` with its empty value in `feature()`, its
   names in the `pub use` list in `backend/mod.rs`, and its `check` in `Registry::check`.
2. Frontend: its module under `slots/`, and its names re-exported from `slots.ts`.
3. Its row below, which `app/tests/rules/catalogue.test.ts` holds this file to.
4. The host that reads it: the page or runtime that draws what the slot holds.

## Backend slots

Every field of `Feature`, as `feature.rs` fills it.

| Slot                       | Module         | What it adds                                                                                                                                 |
| -------------------------- | -------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `name`                     | `mod.rs`       | Its directory's name, which its switch, permissions and audit actions begin with.                                                            |
| `place`                    | `mod.rs`       | Where it sits among the features: its pages in the sidebar, its switch on the DSPs page, its section on the role sheet.                      |
| `depends_on`               | `mod.rs`       | The features and collectors it uses beyond those whose collections it keeps; its code may reach only these, core and itself.                 |
| `switch`                   | `switches`     | Whether every DSP has it (`mandatory`) or the platform owner switches it per DSP (`optional`), with the connections it requires.             |
| `subfeatures`              | `switches`     | The parts of its page switched on their own: its tabs (`tab`) and any other part (`sub`), each with the permissions it owns.                 |
| `permissions`              | `permissions`  | What a DSP's roles may grant, on the role sheet under its name: `perm`, with what each implies, sits under or lets its holder do.            |
| `routes`                   | `mod.rs`       | Its endpoints, each with its path and access, which the server checks before a handler runs; listed in its `tests/api/routes.txt`.           |
| `env`                      | `env`          | The server's environment variables it reads with `config.setting`, in groups that go together; part of a group refuses to start.             |
| `settings`                 | `settings`     | Its sections of a DSP's Settings, each of fields saved together, which Settings draws and core keeps: `settings::read` reads them.           |
| `keeps`                    | `keeping`      | The collections it keeps: for each, what a run needs, how what it brings is stored, what a schedule of it queues and who sets it (`Keeper`). |
| `schedule_aliases`         | `keeping`      | A name a schedule collects by that reads one of its collections another way, as Routes' Today so far, offered as its rules' choice.          |
| `live`                     | `mod.rs`       | The permissions that may follow its collections' progress through the collection updates the dashboard waits on.                             |
| `live_readers`             | `live_readers` | What it reads from a provider for each DSP every few minutes, on a schedule each DSP sets, within the server rather than as jobs.            |
| `migrations`               | `storage`      | What it adds to a database it keeps whole: that database's one list.                                                                         |
| `own_migrations`           | `storage`      | What it adds to a database other owners add to, numbered by the feature from 1 and recorded under its name.                                  |
| `retired_migrations`       | `storage`      | The ledger of a database it no longer opens, kept so its retired storage can be read during an import.                                       |
| `retired_identifiers`      | `retirement`   | Identifiers it renamed, migrated at startup and refused by current readers.                                                                  |
| `retired_schedule_aliases` | `retirement`   | Schedule names it no longer keeps, each split as a DSP opens into a rule of each collection that replaces it, timed as it was.               |
| `upgrade_storage`          | `retirement`   | An import of its storage from before, run before a collector's storage is opened.                                                            |
| `tables`                   | `storage`      | The tables it keeps, by database, which no other owner's code runs SQL on.                                                                   |
| `domains`                  | `caching`      | The kinds of data its reviewed writes change, for the read cache.                                                                            |
| `cached`                   | `caching`      | What evicts the reads it caches, and any other owner's read its data feeds.                                                                  |
| `maintenance`              | `upkeep`       | Work the scheduler runs for it, each task at its own interval.                                                                               |
| `audit`                    | `audit`        | How its actions read in the activity log: the areas they are listed under, the kinds of record they name, and today's names.                 |
| `demo`                     | `commands`     | What fills a new DSP with its demo data, for the development seed and Diagnostics' test DSPs.                                                |
| `commands`                 | `commands`     | Its operator commands of the binary, every one whose name begins with its prefix.                                                            |
| `dsp_connection`           | `mod.rs`       | Whether it adds one of the DSP's own accounts to Settings' DSP Connections, as a collector's connection is.                                  |
| `people`                   | `people/`      | Who its data names, for the feature that tells people apart: a `People` for each kind of data, with what else it knows of them.              |
| `integrations`             | `integrations` | What it does through another feature's connection, starting on or off in each of its accounts, and clears of one forgotten.                  |
| `exposes`                  | `exposes`      | What it offers every other feature, each through a contract; one feature at most offers each.                                                |
| `on`                       | `events`       | What it does when something happens for a DSP: a collection succeeding, the DSP's people changing, or another feature's event.               |

`collectors.rs` holds what a collector declares instead, `Collector`, and `agents.rs` the one
piece outside the features that lets outside agents use Dispatch, `Agents`.

### People

A feature whose data names people connects to the feature that tells them apart (Driver
Match) with one declaration for each kind of data, in its own folder:
`dispatchdev new feature <name> --tables dsp --people amazon` writes one to start from.

```rust
// backend/people.rs
pub const PEOPLE: People = People::new(
    KindInfo { id: "fuel", order: 100, label: "Fuel", icon: "Fuel" },
    dispatch_cortex::SOURCE,
    |store, dsp| {
        people::named(
            &*store.fuel_db(dsp)?,
            "SELECT transporter_id id,driver_name name,min(day) first,max(day) last \
             FROM fill_ups GROUP BY 1,2 ORDER BY last DESC",
        )
    },
)
.records("fill-up", |store, dsp| {
    people::records(
        &*store.fuel_db(dsp)?,
        "SELECT transporter_id id,count(*) count,max(day) last FROM fill_ups GROUP BY 1",
    )
})
.driving();

// feature.rs
people: &[&backend::people::PEOPLE],
```

- **What it is** (`KindInfo`): its id, permanent; its place among the kinds (`order`), which
  is also the order an ID's spellings are read in, the first being the one it is shown by, so
  a kind that writes names less fully goes later (`--people` picks the next free tens); its
  label, and its icon, by the name lucide-react exports it as.
- **Whose IDs** (`Source`): the source its people are named by, declared once by whoever owns
  those IDs, as a collector's `SOURCE` (`dispatch_cortex::SOURCE` for Amazon's transporter
  IDs), or a source of the feature's own (`Source::new(&SourceInfo { id, label, order })`).
  A source's `order` is the order sources are listed in, and whose name a person is shown by:
  the first source's that knows them. The registry refuses one id declared as two sources.
- **Who it names** (`read`): one `Named` for each ID and each way it writes their name, the
  newest first, from everything the data holds, since who someone is doesn't expire; what
  counts (`.records`, `.workdays`) reads only what the feature shows now. `people::named` reads them from a query by its columns' names: `id` and
  `name`, and any of `first` and `last` (the days it saw them), `listed` (1 while its source's
  roster lists them), `offboarded` (1 once its source says the DSP offboarded them: they have
  left, whatever else shows them, until another of their IDs there is listed), `email`,
  `phone`, `roles` (a JSON list of `Driver` and `Helper`),
  `department`, `position` and `photo` (the photo's version, read only from a kind with
  `.photos`). A query naming any other column is refused (`people_columns`), so a misspelt
  one fails the app's people test. Someone a source knows by several IDs, as an account in
  each of several workspaces, is named by each, with the others in `same_as`.
- **How much it holds** (`.records(unit, …)`): `id`, `count` and `last` for each ID
  (`people::records`), counted in what one record is, singular. Without it the kind is listed
  for nobody on a person's sheet.
- **Whether each record shows them driving** (`.driving()`): as a route or an inspection does.
  Someone such a kind counts drives, unless their source's departments say otherwise.
- **When they worked** (`.workdays`): each ID with each day it worked (`people::worked`, from
  columns `id` and `day`), and the days the data was collected for (`Workdays::daily` when
  every day it holds was collected), so two people who never worked the same days are less
  likely one.
- **Which departments drive** (`.driver_departments`): from the feature's settings; anyone it
  names outside them works in the office.
- **Photos** (`.photos`): serves the photo of the first of some IDs it holds one of, each
  named by its version in `read`'s `photo` column.
- **People it can reach, not employs** (`.contacts_only()`): being on its list never keeps
  someone from having left, and someone only such lists know works in the office. One person
  may hold several of its IDs, and one it no longer names sits idle.

A kind is read while its feature is on, and every ID it names gets a person after each
collection; a feature whose people may gain new IDs outside a collection, as a chat
workspace's members do, publishes `events::PeopleChanged` once they're kept. The app's people
test runs every connection's queries against a new DSP, so a connection is tried without a test
of its own. `dispatchdev generate` writes the kinds and sources to TypeScript in
`api/generated/`: each kind with its label, unit and icon, in their order, then the sources and
the titles a person may be given (`people-data.ts`, which imports the icons from lucide-react),
loaded only by the lists of people; and the features whose data names people, by their switches
(`people-features.ts`), small enough for a frontend manifest to ask whether a DSP has one on.

The other way round, any feature that shows people or uses what is known of them asks the
directory (`people/directory.rs`), a contract the feature that tells people apart exposes. Each
`Person` has their code, theirs for good and the same in every feature, so it is what a feature
keeps to name them; the name they are shown by and every other way their sources write it; their
role (driver, helper, office for management, former, or offboarded) and the title someone gave
them, as Dispatch (`PersonTitle`, which `people-data.ts` lists with its label); every ID they
hold, by source; their email addresses and phone numbers; whether someone confirmed who they
are; and the version of their photo.

```rust
let people = people::everyone(db, c.state, c.dsp_id())?; // everyone, by name
let person = people.person(code);                        // by code, a merged one too
let theirs = people.holding(dispatch_paycom::SOURCE, id); // by any source's ID
let having = people.by_phone("+1 555 010 2030");         // by phone number, its last ten digits
let found = people.find("cam hayes");                    // by name, ID, email or phone
```

`people::everyone` keeps what it reads until a collection, a decision or the DSP's settings
change it; `people::read` reads afresh, for work without the server's state, as an agent's
tool. `people::photo` serves a person's photo by their code. Each answers nothing in a build
without the directory. Every member of a DSP reads its people, whatever their role, from
`GET /api/dsp/people`, `…/people/{code}` and `…/people/{code}/photo` (`api/routes.rs`). The
dashboard draws a person with `PersonAvatar`, from their name, code and photo version.

### Exposes

What a feature offers every other feature, each through a contract, so one feature uses
another's work without depending on it or knowing which feature does it (`exposes.rs`). A
contract is a trait, owned by the feature offering it in a crate of its own beside it,
`features/<name>/contract/`, which holds the trait and the types it speaks in and depends on
core alone: any feature uses it without declaring the feature, and it stays in a build that
leaves the feature out, when nothing offers it. A contract core uses itself, as the directory
its people helpers ask, is core's. `dispatchdev add contract <name> --to <feature>` (or
`--contract <name>` on `new feature`) writes one that works as written: its crate, what fulfils
it, its exposure and their test. A feature offers one in its manifest's `exposes`:

```rust
exposes: &[dispatch_core::expose!(
    dyn dispatch_documents_contract::Files, // the contract
    &backend::service::FILES,               // what fulfils it
    "files",                                // its name, as `dispatchdev list exposes` lists it
    "Somewhere to keep a DSP's files, in folders, in its Google Drive.",
    shown,                                  // a page shows it
)],
```

One feature at most offers each contract and each name, as the registry checks. A feature using
one asks for it for a DSP, `exposes::for_dsp::<dyn Files>(db, dsp)` (or the contract crate's own
`for_dsp`), which answers only while the feature offering it is on there; `Registry::exposed`
finds it in the build. `dispatchdev list exposes` lists each, with its feature, its contract and
where that is. Core's own is `directory` (`people/directory.rs`), who an ID is, which the People
section above describes.

One a page shows ends its `expose!` with `shown`, and its contract extends `Shown`: what it's
called where members see it, and the permission that shows what it keeps. The app's export
writes each to TypeScript (`api/generated/exposes.ts`), and `offered(name, view)` in the
frontend's runtime (`exposes.ts`) answers a page using one whether the DSP has it, its name and
its permission, without naming the feature offering it. Its name is in `ExposedName` there, in
every build, which the build's export is checked against; `--shown` on `add contract` or
`new feature --contract` writes all of it.

### Events

What happened for a DSP, which any feature hears of without depending on where it happened or
knowing who hears (`events.rs`). An event is a type with a name (`Event::NAME`). Core's are
`CollectionSucceeded` (`collection.succeeded`), once a collection's outcome is recorded and
before the dashboard hears of it, and `PeopleChanged` (`people.changed`), when the people a
DSP's features name change outside a collection. A feature's own goes in its contract crate,
beside what it exposes, so a feature hearing it depends on neither.

Whoever an event happens to says so, `events::publish(&state, &dsp, PeopleChanged).await`, and
a feature hears it by naming it in its manifest's `on`:

```rust
on: &[dispatch_core::on!(CollectionSucceeded, backend::maintenance::collected)],

pub fn collected(state: Arc<State>, dsp: String, event: CollectionSucceeded) -> Upkeep {
    Box::pin(async move { … })
}
```

`publish` tells each feature hearing it, in the registry's order and one at a time, only while
that feature is on for the DSP, and returns once each has done its work; a feature logs its own
failures. The registry refuses two types under one name, and a feature hearing one twice.
`dispatchdev list events` lists every event a feature hears, with who hears it.

### Live readers

What a feature reads from a provider for each DSP every few minutes, on a schedule each DSP
sets (`live_readers.rs`). Jobs don't suit it: each starts a browser, a few run at once, and a DSP's schedule runs every half
hour at most. A live reader runs within the server instead, as `collection::readers` describes:
one light task for each DSP whose feature is on and whose schedule is set, which holds what the
reader keeps in memory between reads and starts no browser unless a hook opens one.

```rust
// backend/live.rs
pub struct Routes;
pub static READER: Routes = Routes;
impl LiveReader for Routes {
    type Held = Session; // in memory only, made anew each day: Default
    type Read = Day;     // one whole read, from `read` to `kept`
    fn id(&self) -> &'static str {
        "fuel.prices" // `<feature>.<what>`, permanent: each read's record is kept under it
    }
    fn schedule(&self, db: &Store, dsp: &str) -> Result<Option<Schedule>> {
        let set: Set = settings::read(db, dsp, &COLLECTION)?; // its days, times and interval
        let every = Duration::from_secs(set.minutes * 60); // whole seconds, a minute at least
        Schedule::new(&set.days, &set.start, &set.stop, every).map(Some)
    }
    fn read<'a>(&'a self, reading: &'a Reading, held: &'a mut Session) -> Pending<'a, Day> {
        Box::pin(async move { … }) // whole, or an error whose code says why not
    }
    fn kept<'a>(&'a self, reading: &'a Reading, day: Day) -> Pending<'a, Counts> {
        Box::pin(async move { …; Ok(vec![("stations", count)]) })
    }
    // `started` and `stopped` do nothing unless it says otherwise.
}

// feature.rs
live_readers: &[&backend::live::READER],
```

- **Its schedule** (`schedule`): the days of the week its window opens on, the window's start
  and stop, in the DSP's own time, and how often it reads, read from the feature's settings
  with one keyed read; none while it reads nothing there. `Schedule::new` refuses one on no day,
  or reading other than every whole number of seconds, a minute or more (`invalid_schedule`). A
  stop no later than the start is the next day's, and the same as the start reads all day. A
  day's window is the day it opens on, which the hooks are handed as `Reading::day`, today as
  the server reckons it.
- **Its reads** (`read`): one each interval, from the window's start until it closes, on each
  day it opens on; DSPs whose windows open together wait their turns to read. A read is whole,
  or an error whose code says why not; one slower than its interval is cut off
  (`live_read_timeout`). `held` is the DSP's memory, as a provider's session and key: never
  written, and made anew each day and whenever its reading starts again, as after a restart, a
  switch or a stall.
- **A whole read** goes to `kept`, which answers what it counted, under fixed names. A day's
  first whole read goes to `started` first, which starts the day, as clearing the last day's
  data; a restart within the window, or a window saved again, doesn't start it again.
- **A day ends** (`stopped`) as its window closes: once for each day it started, even when the
  server was down then or the day is no longer one it opens on, and before a later day starts.
  A day reads no more once its stop has begun, though its window is moved to end later or
  `stopped` fails part of the way: its stop is finished first, after a restart too, and its
  next reads are the next day's. A hook that fails is asked again: `started` with the next
  whole read, `stopped` a minute later.
- **A browser** (`Reading::browser(provider)`): a hook that needs one, to sign in or take a
  key from a page, opens the DSP's own for a provider, signed in as a job's is and within the
  server's limits on browsers and memory. It's the reader's alone until it closes, closing
  when dropped and as the reader stops for the DSP; what the hook takes from it, as its
  cookies (`http::Http::signed_in`), stays in memory. In fixture mode no browser runs
  (`ProviderBrowser::browser` is none).
- **Switched off**, or its window cleared, a DSP stops reading at once: a read under way is
  dropped, while what keeps a read or a day finishes, though without the DSP's browser, which
  closes. Switched on again within the window, it goes on with its day.
- **A demo DSP** (`enable_all_features`, a development or preview DSP's) has every feature on
  but those that declare a live reader, so nothing reads a provider for it unasked: a preview or
  a test switches one on.

The registry refuses two readers with one id, and an id outside its feature's name.

## Frontend slots

Every field of `FrontendFeature`, as `frontend/feature.ts` fills it. The runtime that reads
them is `core/shell/frontend/runtime/slots/`, one module per slot, gathered by `slots.ts`.

| Slot                     | Module         | What it adds                                                                                                                                                                                                                                                                                                                                      |
| ------------------------ | -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `name`                   | `installed`    | The owner's directory name, as its switch is.                                                                                                                                                                                                                                                                                                     |
| `routes`                 | `pages`        | Its pages, inside a DSP or outside, in sidebar order; which one a DSP opens on (`landing`) or draws Settings (`hostsSettings`); a count by a page's name (`badge`), the item it is under (`parent`): a page, which it highlights, or a sidebar of pages, which lists it, the addresses it was once at (`formerly`), filling the window (`fills`). |
| `sidebars`               | `pages`        | Its sidebars of pages: each an item of the app's sidebar, where its own page is or else its first, that slides in the list of the pages under it, any owner's, keeping the page open; the shell draws the list unless the sidebar draws its own (`draw`), as Settings does, listing a feature's page under Features (`parent: 'settings'`).       |
| `icon`                   | `pages`        | Its icon where none of its pages shows one, as for its page of a DSP's Settings when it has no page in the sidebar.                                                                                                                                                                                                                               |
| `dspSetup`               | `pages`        | Who may save a DSP's profile, and the route that saves it; opening a DSP that has none asks them for it.                                                                                                                                                                                                                                          |
| `movedToSettings`        | `pages`        | Its DSP pages that are pages of Settings now, each with the page of Settings it is: a link to one opens there, with its query.                                                                                                                                                                                                                    |
| `settingsSections`       | `settings`     | The components of its custom sections of a DSP's Settings, by the section's id, as its backend declares them `custom`.                                                                                                                                                                                                                            |
| `pageTabs`               | `page-tabs`    | Its tabs on another feature's page: a page with tabs draws them among its own, the app above any other.                                                                                                                                                                                                                                           |
| `connectionCard`         | `connections`  | A collector's card on a DSP's Connections page.                                                                                                                                                                                                                                                                                                   |
| `connectionPieces`       | `connections`  | Its cards on a DSP's Connections page, beside the collectors': one of the DSP's accounts, or the member's own.                                                                                                                                                                                                                                    |
| `connectionSections`     | `connections`  | Its sections of a connection's settings, its own or another owner's.                                                                                                                                                                                                                                                                              |
| `connectionIntegrations` | `integrations` | What it does through other owners' connections, as its card in the connection's settings says it, with its switch.                                                                                                                                                                                                                                |
| `accountActions`         | `account`      | Its actions on the account menu, beside Source code and Sign out.                                                                                                                                                                                                                                                                                 |
| `watchers`               | `watchers`     | What it keeps running on every DSP page for the person, drawing nothing: a component the shell mounts while the DSP has it and `visible` allows, whatever page is open, loading its code once the browser is online. One that fails stops alone, logged by its id, until the next DSP, or the next page load if its code failed to load.          |
| `cache`                  | `api`          | How the response cache treats its reads: which hold collected data, change with jobs or connections, or change with its writes.                                                                                                                                                                                                                   |
| `longPolls`              | `api`          | Its reads that wait for a change before they answer.                                                                                                                                                                                                                                                                                              |
| `replies`                | `api`          | How the API client checks the replies of its routes, loaded apart from the first page.                                                                                                                                                                                                                                                            |
| `errors`                 | `api`          | What its error codes say, for a code something outside its own screens raises.                                                                                                                                                                                                                                                                    |
| `scheduleIssues`         | `api`          | Why a schedule of its collections waits, by the code it last stopped on.                                                                                                                                                                                                                                                                          |
| `collected`              | `collected`    | What each collection it keeps collects, by the collection's name: its kinds of record and their fields, each with a made-up value, the count of the latest successful run that counts each kind, what a run reads, does, skips and doesn't keep. Settings' Collectors page loads it under the collection; `new feature --keeps` writes one.       |
| `platformSlots`          | `platform`     | What it puts in the platform owner's slots: its switch's icon, its audit wording, how Diagnostics words its runs, its mail kinds.                                                                                                                                                                                                                 |
