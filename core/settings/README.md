# Settings

A DSP's settings: what its members set for each feature, in sections the features declare in
their manifests (the `settings` slot, `core/manifest/backend/settings.rs`) and the Settings page
draws from those declarations alone. Core keeps the values, checks every save against what each
field holds, and records each change of the DSP's in its log. A feature switched off, or hidden
from a DSP, has no sections there; switched on, they're back as they were.

## Declaring a section and reading it

```rust
// features/x/backend/settings.rs
pub const POLICY: Section = section("x.policy", "Collection", "x.manage", &[
    toggle("enabled", "Collects", true),
    number("days", "Keep", 30, 3650, Some(90)).unit("days").none("Every day"),
]);
#[derive(serde::Deserialize)]
pub struct Policy { pub enabled: bool, pub days: Option<i64> }

// features/x/feature.rs: `settings: &[backend::settings::POLICY],`
// anywhere in its backend:
let policy: Policy = dispatch_core::settings::read(db, dsp, &POLICY)?;
```

- **Fields** are `toggle`, `choice` (fixed options), `pick` (options the feature lists as it's
  read: one indexed query), `many` (several, `.ordered()` when their order is the member's),
  `number` (`.unit`, `.presets`, `.none` for a labelled none), `text` (`.code` for uppercase
  letters and digits, `.locked` once set; empty only until set, when its default is), `days`
  (days of the week, kept `mon` to `sun`, read as `Vec<Weekday>`), `time` and `timezone`. Each field's id is the serde name its feature reads it by; every field reads
  as its default until saved, and as its default again if what was kept no longer fits it. When
  a collection runs isn't a field: its keeper says who sets it (`Keeper::schedules`), on the
  Collectors page.
- **Pages:** a section is on its feature's own page (its switch's id, `-` for `_`, ordered by
  its `place`) unless `.on(..)` puts it on a shared page: `general`, `security`, `theme` and `display`,
  the member's own; `dsp`, `connections`, `collectors` and `data`, the DSP's. `.collection(..)` puts
  it with a collection its feature keeps, on `collectors`, under the permission that sets when
  that collection runs: the Collectors page draws it below the collection's schedule, as Daily
  Performance's `daily_performance.collection` is. `.reader(..)` names the live reader of its
  feature's whose schedule it sets: Settings shows under its heading how that reader reads for
  the DSP, as Live Route Data's `live_route_data.collection` does.
- **Who:** `permission`, one its feature declares, or `roles::ACCESS` for every member.
  `.part(..)` makes it go with a part of its feature. `.member()` gives each member their own
  values, which the log doesn't record.
- **What else:** `.custom()` for one its owner's own component draws (the frontend's
  `settingsSections`), `.warn(..)` for words to confirm before a save that loses something,
  `.saved(..)` for what else a save does in the same write, `.kept(..)` for values kept in the
  feature's own tables. Each of these is handed only the DSP being read or saved.
- `Registry::check` refuses, at startup, a section not named `<feature>.<what>` with its
  `what` snake_case (a `:` would read as a member's own record), under a permission its
  feature doesn't declare, on a page or with a part that doesn't exist, with no fields unless
  it's custom, with a field whose default its kind doesn't hold, or with a collection its
  feature doesn't keep under the section's permission, or custom, or moved off `collectors`.

Core's own sections are custom ones its screens draw: accounts' `accounts.profile`,
`accounts.security` and `accounts.theme`, each member's own, and collection's
`collection.connections` and `collection.collectors`, which every member is sent, each
component saying what it shows them.

## Storage and saving

Each section's record is a row of the DSP's own `settings` table: `section:<id>`, or a member's
own `section:<id>:<user>`, holding `{revision, values, at, by}`. A section `kept` elsewhere keeps
only its revision here. One at revision 0 with no values is one a release before kept where it
had nothing to move from before Settings: its defaults hold, as for one with no record.
`settings::read` is one keyed lookup; Settings reads all of a member's records of a DSP in one
query.

A section may declare `.aliases(&[...])` for an identity-only transition. Typed reads, the
bulk listing and saves accept one record at either spelling. A save uses the running
declaration's key; even a no-op may move that key, preserving the original JSON and revision.
Two records in a family fail with `settings_identity_conflict` without choosing or merging
their values. Startup normalizes all members' records, including disabled features, in one
DSP transaction before owner carry-over. Custom feature aliases apply only to their owning DSP.

A feature's raw DSP setting uses `StoredKey::new(id, aliases)`, declared in `settings_keys`,
with `settings::read_key` and `write_key`. This is a separate namespace from section revision
envelopes. Startup and writes move keys with their payload intact; callers retain their own
revision and audit contracts. An absent row remains absent until an actual save.

The generated section identity families also normalize known reply IDs to the receiving
browser bundle's spelling before validation results enter any cache. Request scope is captured
at admission, so a response completing after a DSP change never resolves against the new
DSP's custom aliases. This compatibility does not rename feature IDs, Settings page addresses,
physical tables or browser storage, and does not retrofit untouched pre-bridge browser tabs.

`settings::save` takes any of a section's fields, refuses one it hasn't or a value its kind
doesn't hold (`invalid_input`) and a revision another save passed
(`settings_changed_reload_before_saving`, 409), asks its `warn` unless confirmed, then in one
write keeps the values, runs `saved`, and records `settings.changed`: the section's label, each
changed field as its label with what it was and became as the field shows it, and the
reference `setting:<id>`, under the log's Settings area. Saving what's already kept changes
nothing. A save wakes the scheduler only when it retimes the DSP's schedules, as the DSP
profile's timezone does: retiming them marks the work, and the server wakes the scheduler once
the work is done.

## API

- `GET /api/dsp/settings`: every section the member may see, with its fields, values and
  revision, and the pages they're on. It's served from the read cache under the DSP's own name,
  for what the member's access makes of it (their role, permissions and the DSP's features),
  one answer for every member with the same access unless a section they see has fields each
  member keeps their own of, when it's theirs alone; any write to that DSP's data but a
  collection's live pages evicts it. Each list a field offers (a `pick`'s, a listed `many`'s)
  is read once for an answer, however many sections offer it. A section that can't be read now, as when its keeper
  or a list it offers fails, is left out, with a page only it is on, and noted in the server's
  log as `settings.section_skipped` (its DSP, id and error code, never its values); the rest
  are answered as they are, and a save of it fails with that error before keeping anything.
- `POST /api/dsp/settings/{section}`: `{values, revision, confirmed?}`, answering the section as
  it is now, its options as the save listed them to check it, or `{warning}` with nothing kept.
  A section the DSP doesn't have, or hides from the member, is `not_found`; one they lack the
  permission of is `permission_denied`.

The DSP is always the one the member's view authorized: no request names one, so nothing of one
DSP is read, saved or evicted from another. `tests/backend/integration/isolation.rs` holds it
over HTTP.
