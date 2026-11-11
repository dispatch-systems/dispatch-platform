# Review rules

What a review of a Dispatch change holds it to, by the paths each rule covers. Skip
`package-lock.json`, `Cargo.lock` and anything under a `generated/` folder.

## Findings

- **Important**: the change breaks behavior, leaks data, blocks a rollback, breaks a contract
  another owner or caller relies on, or adds needless complexity, such as a workaround on a
  workaround or a copy of something that exists. Name the class of bug, and look for every
  other instance of it in the change.
- **Nit**: anything smaller worth saying. At most three; the author may decline one.
- **Pre-existing**: a real bug in code the change didn't write. It isn't the change's to fix.
- **Question**: a choice to justify, with what you'd do instead.
- **Out of scope**: what would change what was approved.

A finding that a rule here was broken quotes the rule and applies only where its paths cover
the file. A claim about behavior cites the `path:line` that shows it, not an inference from a
name. Don't report:

- what CI already enforces: formatting, lint, types, the rule tests;
- code that looks wrong but is correct;
- nitpicks a senior engineer wouldn't raise;
- general quality concerns, such as test coverage, that no rule here names.

## Every change

- A finding is about what the changed lines introduce. Older code is Pre-existing at most; do
  not ask for cleanup outside the change's scope.
- Flag a change that leaves `AGENTS.md`, a skill, a README or this file saying something
  untrue.
- Never accept a change that weakens a test, budget or check to get a pass: a deleted or
  loosened assertion, a skipped or removed test, a raised limit or budget, or a new suppression
  such as `#[allow]`, `eslint-disable`, `@ts-ignore` or `@ts-expect-error`. Flag each and ask
  why it is needed.
- Use synthetic people, businesses, example emails and deployment addresses in fixtures.
- Keep credentials, runtime state and host-specific configuration out of source and logs.
- Preserve required copyright attribution and narrowly reviewed legacy compatibility
  identifiers. Do not broadly allowlist scanner findings or approve changed binary assets
  without reviewing their contents.
- Local exports also need an export audit: inspect screenshots and archive members, run OCR,
  and bind asset approval to exact content hashes. Never generate review approvals merely to
  silence findings; local identity hashes and review manifests stay private.

## `**/migrations/**`

Production rolls back one release, so each release must open, and leave usable, the previous
release's data:

- Migrations are additive only: new tables, nullable or defaulted columns, indexes. Flag any
  edit to or renumbering of an existing migration, any change to `PRAGMA user_version`, and any
  drop, rename or rewrite of what the previous release still uses; that takes two releases.
- Adding a column is a `Code` step with `add_column`, not a plain `ALTER TABLE` in a `.sql`
  file.
- A `CHECK` that lists values is widened by rebuilding the table under a `_v2` name, copying
  rows with an explicit column list, rebuilding referencing tables before their parent, and
  recreating the indexes, as `jobs/0002_scorecard_kind.sql` does. The release that rebuilds
  must not write the new values yet.

## `{app,core,collectors,features,mcp}/**/*.rs`

- Logs and request events never carry request bodies, raw emails or IP addresses, cookies,
  tokens or employee records. Session identifiers shown to users are HMAC identifiers, never
  bearer tokens or token hashes. Browser data and provider profiles are credentials.
- MFA challenge state and authenticator secrets stay encrypted at rest; recovery codes are
  high-entropy, hashed and one-use. Factor changes revoke other sessions, and sensitive writes
  require recent verification. Existing retired passkeys must not enroll or lock an account
  implicitly.
- Compare errors with `error.is(Code::X)`, never by their text. Never hard-code job titles;
  customizable roles stay in the role system.

## `{**/api/routes.rs,**/api/routes/**}`

A route's access (`Public`, `Session`, `PlatformOwner` or `Dsp("permission")`) must be no
broader than what it reads or changes. Work that waits outside the database uses
`async_get`/`async_post` and re-authorizes after each wait. A new route needs its row in
`app/tests/backend/integration/http_routes.rs`. A password change verifies the current
password. Security routes are exempt from the normal MFA gate only so verification can
complete; enrollment, factor removal, recovery-code replacement and session revocation must
enforce their own recent-authentication requirement.

## `{features/*/feature.rs,features/custom/*/*/feature.rs,collectors/*/collector.rs}`

A switch, tab, permission, read toggle or collection is declared once, in its owner's manifest,
with its label; the catalog, the role sheet and the DSPs page are built from it. Flag one
copied by hand anywhere else.

## `core/**`

Core names no feature or collector: flag a feature's or collector's name, id, table, host or
wording in core outside its tests. What a feature adds reaches core through a slot. Anything
for the platform owner's dashboard belongs in `core/platform_owner/`, but the Agents page,
which is the MCP's, in `mcp/frontend/`.

## `collectors/**`

A collector names no feature and none of a feature's tables. It returns typed data; the feature
that keeps the collection writes it.

## `features/**`

A custom feature belongs only to the DSP code in `features/custom/<code>/<name>/`; its ids
start with `<code>_`, its switch is custom and optional, and it exposes no contracts or MCP
tools. Nothing it runs or shows reaches another DSP.

A feature uses only its own tables, core, and the collectors and features its manifest
declares. Flag SQL against another owner's tables and imports beyond those dependencies.

## `mcp/**`

The MCP uses its own tables, and reads people, DSPs and features' data only through their
owners' functions, never their SQL. A tool never decides who may use it: the connection's
grants do, and every call is checked again as the connection stands.

## `collectors/paycom/{collections/timecards/extract.rs,scripts/timecard.js}`

`extract.rs` ports `timecard.js`'s detached reading. A change to one must be matched in the
other and proved with `http_extraction_parity`.

## `{app,core,features,collectors,mcp}/**/frontend/**`

- No Tailwind, shadcn, Motion or TanStack Virtual. TanStack Table is used only through
  `core/shell/frontend/ui/useDataTable.ts`. Styles use the tokens in
  `core/shell/frontend/styles/tokens.css` and live with the component that owns them.
  Animation is hand-written CSS or a small rAF loop, and is off under
  `prefers-reduced-motion`.
- Use the existing primitives before new markup: `Header`, `Tabs`, `Modal`, `ConfirmDialog`,
  `Popover`, `DataState` (for loading and errors), `SearchInput`, `Pagination`, `SortHeader`,
  `DetailList`, `Badge`, `DspAvatar`, `Empty`, `ErrorBox`, `Loading`, `useDataTable`,
  `useFocusTrap`. Mutations use `useAction`; formatting comes from
  `core/shell/frontend/lib/format.ts`.
- An editor with unsaved changes never just closes: Escape, the backdrop and the close button
  ask "Save changes?" with only Save and Discard, as `features/team/frontend/RoleSheet.tsx`
  does. Closing or refreshing the browser tab discards silently, with no `beforeunload` prompt.
- No subtitles, taglines or descriptive text beneath page titles, section headings or card
  titles.
- Links use `dspHash`/`platformHash` and `navigate`, never hand-written hash strings.
