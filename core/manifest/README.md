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

Adding a slot:

1. Backend: its module here, its field in `Feature` with its empty value in `feature()`, its
   names in the `pub use` list in `backend/mod.rs`, and its `check` in `Registry::check`.
2. Frontend: its module under `slots/`, and its names re-exported from `slots.ts`.
3. Its row below, which `app/tests/rules/catalogue.test.ts` holds this file to.
4. The host that reads it: the page or runtime that draws what the slot holds.

## Backend slots

Every field of `Feature`, as `feature.rs` fills it.

| Slot                  | Module         | What it adds                                                                                                                       |
| --------------------- | -------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| `name`                | `mod.rs`       | Its directory's name, which its switch, permissions and audit actions begin with.                                                  |
| `place`               | `mod.rs`       | Where it sits among the features: its pages in the sidebar, its switch on the DSPs page, its section on the role sheet.            |
| `depends_on`          | `mod.rs`       | The features and collectors it uses beyond those whose collections it keeps; its code may reach only these, core and itself.       |
| `switch`              | `switches`     | Whether every DSP has it (`mandatory`) or the platform owner switches it per DSP (`optional`), with the connections it requires.   |
| `subfeatures`         | `switches`     | The parts of its page switched on their own: its tabs (`tab`) and any other part (`sub`), each with the permissions it owns.       |
| `permissions`         | `permissions`  | What a DSP's roles may grant, on the role sheet under its name: `perm`, with what each implies, sits under or lets its holder do.  |
| `routes`              | `mod.rs`       | Its endpoints, each with its path and access, which the server checks before a handler runs; listed in its `tests/api/routes.txt`. |
| `settings`            | `settings`     | The server's settings it reads with `config.setting`, in groups that go together; a server with part of a group refuses to start.  |
| `keeps`               | `keeping`      | The collections it keeps: for each, what a run needs, how what it brings is stored, and what a schedule of it queues (`Keeper`).   |
| `schedule_aliases`    | `keeping`      | A name a schedule runs several of its collections by at once.                                                                      |
| `live`                | `mod.rs`       | The permissions that may follow its collections' progress through the collection updates the dashboard waits on.                   |
| `migrations`          | `storage`      | What it adds to a database it keeps whole: that database's one list.                                                               |
| `own_migrations`      | `storage`      | What it adds to a database other owners add to, numbered by the feature from 1 and recorded under its name.                        |
| `retired_migrations`  | `storage`      | The ledger of a database it no longer opens, kept so its retired storage can be read during an import.                             |
| `retired_identifiers` | `retirement`   | Identifiers it renamed, migrated at startup and refused by current readers.                                                        |
| `upgrade_storage`     | `retirement`   | An import of its storage from before, run before a collector's storage is opened.                                                  |
| `tables`              | `storage`      | The tables it keeps, by database, which no other owner's code runs SQL on.                                                         |
| `domains`             | `caching`      | The kinds of data its reviewed writes change, for the read cache.                                                                  |
| `cached`              | `caching`      | What evicts the reads it caches, and any other owner's read its data feeds.                                                        |
| `maintenance`         | `upkeep`       | Work the scheduler runs for it, each task at its own interval.                                                                     |
| `after_collection`    | `upkeep`       | What runs for the DSP after each of its collections succeeds, before the dashboard hears of it.                                    |
| `audit`               | `audit`        | How its actions read in the activity log: the areas they are listed under, the kinds of record they name, and today's names.       |
| `demo`                | `commands`     | What fills a new DSP with its demo data, for the development seed and Diagnostics' test DSPs.                                      |
| `commands`            | `commands`     | Its operator commands of the binary, every one whose name begins with its prefix.                                                  |
| `dsp_connection`      | `mod.rs`       | Whether it adds one of the DSP's own accounts to Settings' DSP Connections, as a collector's connection is.                        |
| `people`              | `people/`      | Who its data names, for the feature that tells people apart: a `People` for each kind of data, with what else it knows of them.    |
| `integrations`        | `integrations` | What it does through another feature's connection, switched on and off in that connection's settings for each of its accounts.     |
| `services`            | `services/`    | What it offers every other feature through a contract core defines; one feature at most offers each.                               |

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
  newest first. `people::named` reads them from a query by its columns' names: `id` and
  `name`, and any of `first` and `last` (the days it saw them), `listed` (1 while its source's
  roster lists them), `email`, `phone`, `roles` (a JSON list of `Driver` and `Helper`),
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
workspace's members do, calls `people::changed(state, dsp)` once they're kept. The app's people
test runs every connection's queries against a new DSP, so a connection is tried without a test
of its own. `npm run contracts:generate` writes the kinds and sources to TypeScript in
`api/generated/`: each kind with its label, unit and icon, in their order (`people-data.ts`,
which imports the icons from lucide-react), loaded only by the lists of people; and the
features whose data names people, by their switches (`people-features.ts`), small enough for a
frontend manifest to ask whether a DSP has one on.

The other way round, a feature showing people asks the directory, a service
(`services/directory.rs`) the feature that tells people apart offers: `people::who` says who
holds each ID of a source, with every ID they hold and the version of their photo,
`people::photo` gives that photo for the feature to serve behind its own permission, and both
answer nothing in a build without it. The dashboard draws a person with `PersonAvatar`.

### Services

What a feature offers every other feature through a contract core defines, so one feature uses
another's work without depending on it or knowing which feature does it. Each service is a
trait in a module of its own in `backend/services/`, named for what it does; a feature offers
it in its manifest's `services`, one feature at most offers each, and a feature using it asks
for it for a DSP (`files::for_dsp`), which answers only while the feature offering it is on
there. The first is `files`, somewhere to keep a DSP's files in folders: its name and the
permission that shows its folders, the largest file it takes, why it can't keep files yet,
browsing its folders, a folder by ID, making a folder, saving a file streamed as it arrives
(`Upload::streamed`) with who it came from, a member or someone known only by name, and writing
a page of HTML as a document members open and edit, then over it as the page changes. The
second is `directory`, who an ID is, which the People section above describes. The
services offered are written to TypeScript too (`api/generated/services.ts`): `offered(name,
view)` in the frontend's runtime answers a page using one whether the DSP has it, what it's
called and the permission that shows what it keeps, without naming the feature offering it.

## Frontend slots

Every field of `FrontendFeature`, as `frontend/feature.ts` fills it. The runtime that reads
them is `core/shell/frontend/runtime/slots/`, one module per slot, gathered by `slots.ts`.

| Slot                     | Module         | What it adds                                                                                                                      |
| ------------------------ | -------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| `name`                   | `installed`    | The owner's directory name, as its switch is.                                                                                     |
| `routes`                 | `pages`        | Its pages, inside a DSP or outside, in sidebar order; which one a DSP opens on (`landing`) or draws the Settings tabs.            |
| `dspSetup`               | `pages`        | Who may save a DSP's profile, and the route that saves it; opening a DSP that has none asks them for it.                          |
| `settingsTabs`           | `settings`     | Its tabs on a DSP's Settings page.                                                                                                |
| `settingsPieces`         | `settings`     | Its pieces of other owners' Settings tabs, drawn under the tab's own panel or above it.                                           |
| `pageTabs`               | `page-tabs`    | Its tabs on another feature's page, which that page draws among its own.                                                          |
| `connectionCard`         | `connections`  | A collector's card on a DSP's Connections tab.                                                                                    |
| `connectionPieces`       | `connections`  | Its cards on a DSP's Connections tab, beside the collectors': one of the DSP's accounts, or the member's own.                     |
| `connectionSections`     | `connections`  | Its sections of a connection's settings, its own or another owner's.                                                              |
| `connectionIntegrations` | `integrations` | What it does through other owners' connections, as its card in the connection's settings says it, with its switch.                |
| `accountActions`         | `account`      | Its actions on the account menu, beside Source code and Sign out.                                                                 |
| `cache`                  | `api`          | How the response cache treats its reads: which hold collected data, change with jobs or connections, or change with its writes.   |
| `longPolls`              | `api`          | Its reads that wait for a change before they answer.                                                                              |
| `replies`                | `api`          | How the API client checks the replies of its routes, loaded apart from the first page.                                            |
| `errors`                 | `api`          | What its error codes say, for a code something outside its own screens raises.                                                    |
| `scheduleIssues`         | `api`          | Why a schedule of its collections waits, by the code it last stopped on.                                                          |
| `platformSlots`          | `platform`     | What it puts in the platform owner's slots: its switch's icon, its audit wording, how Diagnostics words its runs, its mail kinds. |
