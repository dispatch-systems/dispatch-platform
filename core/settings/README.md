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
    schedules("schedules", "Schedules", "/api/dsp/x/schedules", "x", ("Daily X", "18:00")),
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
  letters and digits, `.locked` once set; empty only until set, when its default is), `time`,
  `timezone`, and `schedules`, which holds no value: Settings draws core's schedule editor for
  the feature's `schedule_routes` there. Each field's id is the serde name its feature reads it
  by; every field reads as its default until saved, and as its default again if what was kept
  no longer fits it.
- **Pages:** a section is on its feature's own page (its switch's id, `-` for `_`, ordered by
  its `place`) unless `.on(..)` puts it on a shared page: `general`, `security` and `theme`, the
  member's own; `dsp`, `connections` and `data`, the DSP's.
- **Who:** `permission`, one its feature declares, or `roles::ACCESS` for every member; a write
  under a `.recently_verified()` permission asks for a recent verification. `.part(..)` makes it
  go with a part of its feature. `.member()` gives each member their own values, which the log
  doesn't record.
- **What else:** `.custom()` for one its owner's own component draws (the frontend's
  `settingsSections`), `.warn(..)` for words to confirm before a save that loses something,
  `.saved(..)` for what else a save does in the same write, `.kept(..)` for values kept in the
  feature's own tables. Each of these is handed only the DSP being read or saved.
- `Registry::check` refuses, at startup, a section not named `<feature>.<what>` with its
  `what` snake_case (a `:` would read as a member's own record), under a permission its
  feature doesn't declare, on a page or with a part that doesn't exist, with no fields unless
  it's custom, or with a field whose default its kind doesn't hold.

Core's own sections are custom ones its screens draw: accounts' `accounts.profile`,
`accounts.security` and `accounts.theme`, each member's own, and collection's
`collection.connections`, which every member is sent, its component saying what it shows them.

## Storage and saving

Each section's record is a row of the DSP's own `settings` table: `section:<id>`, or a member's
own `section:<id>:<user>`, holding `{revision, values, at, by}`. A section `kept` elsewhere keeps
only its revision here. `settings::read` is one keyed lookup; Settings reads all of a member's
records of a DSP in one query.

`settings::save` takes any of a section's fields, refuses one it hasn't or a value its kind
doesn't hold (`invalid_input`) and a revision another save passed
(`settings_changed_reload_before_saving`, 409), asks its `warn` unless confirmed, then in one
write keeps the values, runs `saved`, and records `settings.changed`: the section's label, each
changed field as its label with what it was and became as the field shows it, and the
reference `setting:<id>`, under the log's Settings area. Saving what's already kept changes
nothing. A save wakes the scheduler only when it retimes the DSP's schedules, as the DSP
profile's timezone does: retiming them marks the work, and the server wakes the scheduler once
the work is done.

`settings::adopt` is how a feature moves values it kept itself into its declared section,
once, from its `upgrade_storage`: checked as a save checks them, kept at revision 1 only while
nothing of the section was ever saved, and not recorded in the log.

## API

- `GET /api/dsp/settings`: every section the member may see, with its fields, values and
  revision, and the pages they're on. It's served from the read cache under the DSP's own name,
  for what the member's access makes of it; any write to that DSP's data but a collection's live
  pages evicts it. Each list a field offers (a `pick`'s, a listed `many`'s) is read once for an
  answer, however many sections offer it. A section that can't be read now, as when its keeper
  or a list it offers fails, is left out, with a page only it is on, and noted in the server's
  log as `settings.section_skipped` (its DSP, id and error code, never its values); the rest
  are answered as they are, and a save of it fails with that error before keeping anything.
- `POST /api/dsp/settings/{section}`: `{values, revision, confirmed?}`, answering the section as
  it is now, or `{warning}` with nothing kept. A section the DSP doesn't have, or hides from the
  member, is `not_found`; one they lack the permission of is `permission_denied`.

The DSP is always the one the member's view authorized: no request names one, so nothing of one
DSP is read, saved or evicted from another. `tests/backend/integration/isolation.rs` holds it
over HTTP.
