# Conduct

A DSP's writeups of its drivers, and where each driver stands. A writeup is a Corrective Action
Notice in a category (Safety, Excessive Tardies, POD, …), at a tier: Verbal, Written or Final, a
final one just that, with a suspension of a length someone types, or pending further
investigation. Each category climbs its tiers on its own, and steps back down in time: after a
set number of days, weeks or months it reduces one tier, or fully resets to a clean record, or
it never does. The wait runs from the driver's latest writeup in the category, and each tier it
reduces by starts the next wait. A driver's next writeup in it is suggested
one tier above where they stand, never below the tier the category starts at, and Final again
once they're at Final; written, it puts them at its tier, or keeps them higher, and starts the
wait again. The page prints the notice; the driver signs it on paper; someone marks it signed,
with a photo of the paper if they like.

- **Switch:** `conduct`, off for every DSP until the platform owner switches it on. It needs no
  connection.
- **Uses:** `files`, what a feature exposes as where a DSP keeps its files, to back signed
  copies up while the DSP has one and chose to. Conduct never learns which feature keeps them.
- **Permissions:** `conduct.use`, to see every writeup, write one up, change or delete one not yet
  signed, and mark one signed. `conduct.manage`, which includes it, to delete a signed one, and
  to add, change, retire and restore categories and how each steps down. Only owners hold them
  until a role grants them.
- **Page:** Conduct, in a DSP's sidebar: Drivers (each driver at a tier, their tier in each
  category) and Writeups (every writeup), a driver's own page (each category's tier and when it
  next steps down), a writeup's, and the form that
  writes or changes one beside its notice. Its search, category, tier and signature filters are in the address (`frontend/address.ts`),
  so Back and links restore them; page cursors and scroll return with the list.
- **API** (`api/routes.rs`): `GET /api/dsp/conduct` reads compact categories, people, current standings,
  driver totals and category counts. It uses the shared read cache, evicted by Conduct, DSP
  and directory changes, with the DSP-local date in its key. The page consumes prefetched
  data, revalidates while visible and at the DSP date boundary, and refreshes before starting
  a notice. `GET /api/dsp/conduct/writeups` returns a bounded page filtered by person, search,
  category, tier and signature state, with a cursor naming the complete date/creation/id
  ordering. Merged directory codes are resolved before filtering.
  `GET /api/dsp/conduct/context` supplies standing and previous notices at the selected
  notice date, preserving an edited notice's position within that day. A writeup mutation
  answers with that writeup; deleting one, and category mutations, with the compact overview.
  Deleting a signed one needs `conduct.manage` (`permission_denied` without it). Writeups under
  `/api/dsp/conduct/writeups[/{id}[/sign|/delete|/copy]]`, behind `conduct.use`; categories under
  `/api/dsp/conduct/categories[/{id}[/retire|/restore]]`, behind `conduct.manage`, as are the read-only
  `GET /api/dsp/conduct/categories/{id}/impact` preview of a proposed step-down rule,
  bound to the category revision, and `GET /api/dsp/conduct/backup`: where the DSP's files are kept, by the name members know that
  by; whether the backup is failing; why, in the words of what keeps them: why it can't keep
  them now, or why it didn't keep the copy tried latest; and how many signed copies aren't
  backed up yet. It answers only a member who may still read it once those are asked.
- **Data**, in each DSP's own database:
  - `conduct_categories`: each category's name, starting tier, step down (reduce or reset, and
    after how long, or none) and the wording of the three sections a new writeup starts with,
    `[…]` marking a blank. Every DSP starts with twelve (`migrations/dsp/0001_conduct.sql`), each
    starting at Verbal and reducing a tier every six months. A retired one stays for its
    writeups. Where everyone stands is worked out on every read, so a changed step down applies
    to everyone at once; writeups stay as they were written.
  - `conduct_writeups`: each writeup as it prints. It names its person by their code in core's
    directory (`people::everyone`) and keeps their name as printed; the page shows whoever the
    code names now, a merged person's code finding whoever kept their IDs. Signed, it never
    changes again, and only a member who manages Conduct deletes it, for good: where its person
    stands is worked out without it from then on. Edits name the revision they read.
  - `conduct_requests`: actor-bound receipts for copy uploads and signatures, committed with
    the mutation. Retrying the same request returns its result; another actor, payload or
    notice cannot reuse it. Receipts cascade with unsigned deletion so an older release can
    still delete unsigned notices.
  - `conduct_copies`: the photo of a writeup's signed paper, which the page shrinks to 2000
    pixels before sending. A signed writeup's photo, once kept, stays until the writeup is
    deleted.
  - `conduct_backups`: each signed copy as it's backed up: where it was saved, or how many tries
    failed, when the last did and why, by its error code, and when the next is due.
    `conduct_backup_folders`: the folders the copies go in, each by its ID where the DSP keeps
    its files (migration 2).
- **Settings** (`backend/settings.rs`), on its own page of Settings (`?tab=conduct`), behind
  `conduct.manage`: `conduct.categories`, the categories with their step down, drawn by
  `frontend/settings/CategoriesSection.tsx` and saved through Conduct's own routes; and
  `conduct.backup`, whose one field, `signed_copies`, off until someone turns it on, backs signed
  copies up. `frontend/settings/BackupSection.tsx` draws it only while something keeps the
  DSP's files: a switch that names what keeps them and where, as `files` says, and saves once
  it's flipped; and, while the backup is failing, why and how many copies wait.
- **Backup** (`backend/backups.rs`): while the DSP chose it, every minute the photos of signed
  writeups not yet saved go where the DSP keeps its files, 20 a pass for each DSP, those signed
  before it chose to as well: into `Writeups`, the person's folder inside it, named as the
  directory knows them now, and the category's inside theirs, each file named for the writeup's
  date and tier (`2026-11-02 Written.jpg`), from whoever marked it signed. The activity log
  is given "a driver's folder" for a person's folder, never their name: it keeps no one's
  disciplinary record. A writeup signed
  without a photo has nothing to save. Marking one signed never waits on it. A folder is known
  by its ID, so one renamed or moved still takes its copies, and one no longer there is made
  again. A try that fails is reported (`conduct.backup_failed`) and waits a minute, five,
  fifteen, an hour, then six hours between tries, for as long as it isn't saved; the rest of
  the pass waits for the next. While what keeps the DSP's files says it can't, nothing is
  tried. What's saved is everyone's who sees those files, and is never changed: Conduct keeps
  its own copy. It's taken back only as its writeup is deleted, while the DSP still backs its
  copies up: moved to the trash of where the files are kept, as the member deleting it, before
  the writeup goes. While it can't be, the deletion is refused (`conduct_copy_not_removed`) and
  nothing is deleted; one already gone from the DSP's files doesn't keep its writeup. With the
  backup off, the copy saved stays where it is. A copy saved in the moment its writeup was
  deleted goes with it. Switched off, as Conduct itself or the DSP disabled, the backup
  stops before the next thing it would send, in a pass under way too, and what was saved stays.
- **Activity log:** a category added, changed, retired or restored, and a writeup deleted, under
  Settings (`frontend/audit-wording.ts`). A writeup keeps who wrote it and who marked it signed
  itself.

## Where things are

- `backend/standing.rs`: the tier rules, climbing and stepping down: where each person stands in
  each category, when they next step down, and where they stood before each writeup.
- `backend/categories.rs`, `writeups.rs`, `copies.rs`, `requests.rs`: their storage, revision
  checks and retry receipts.
- `backend/history.rs`: paginated history, date-specific drafting and category policy impact.
- `backend/backups.rs`: backing signed copies up, and how it's going.
- `backend/overview.rs`: compact summaries, with people as the directory knows them now.
- `frontend/writeups/`: photo preview, rotation and retake before signing; cancellation during
  preparation and a non-dismissible commit phase; signed photos with explicit retry and enlargement.
- `frontend/tiers.ts`: how tiers read and climb on the page, from what the server works out,
  and what a writeup prints.
- `frontend/notice/`: the notice, laid out in points as the DSP's own form so it prints on one
  letter page, a section too long for its four lines shrinking to fit, with printing refused below 7pt
  and all saved text preserved; responsive width fitting and an enlarged reading view; its fixed wording in
  `wording.ts`; printing it alone; and in `fonts/`, the free fonts it falls back on where a
  computer lacks Arial Narrow, Arial or Snell Roundhand, each with its license.
- `frontend/compose/`: the writeup form, searchable driver picker and directly editable
  template blanks, with full-text editing and client-side section limits; `draft.ts` fills it as a person and a category are
  chosen, keeping whatever someone changed.
- `frontend/drivers/`, `frontend/writeups/`, `frontend/settings/`: each screen;
  `settings/StepDown.tsx` sets a category's step down.
