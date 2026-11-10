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
  signed, and mark one signed. `conduct.manage`, which includes it, to add, change, retire and
  restore categories and how each steps down. Only owners hold them until a role grants them.
- **Page:** Conduct, in a DSP's sidebar: Drivers (each driver at a tier, their tier in each
  category) and Writeups (every writeup), a driver's own page (each category's tier and when it
  next steps down), a writeup's, and the form that
  writes or changes one beside its notice. What it shows is in the address (`frontend/address.ts`)
  so Back and links work.
- **API** (`api/routes.rs`): `GET /api/dsp/conduct` reads everything the page draws, and every
  write answers with it again, so the page shows a change at once. Writeups under
  `/api/dsp/conduct/writeups[/{id}[/sign|/delete|/copy]]`, behind `conduct.use`; categories under
  `/api/dsp/conduct/categories[/{id}[/retire|/restore]]`, behind `conduct.manage`, as is
  `GET /api/dsp/conduct/backup`: where the DSP's files are kept, by the name members know that
  by; whether the backup is failing; why where they're kept can't keep them now, in its words;
  and how many signed copies aren't backed up yet.
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
    changes again, nor can it be deleted. Edits name the revision they read.
  - `conduct_copies`: the photo of a writeup's signed paper, which the page shrinks to 2000
    pixels before sending. A signed writeup's photo, once kept, stays.
  - `conduct_backups`: each signed copy as it's backed up: where it was saved, or how many tries
    failed, why the last did, by its error code, and when the next is due.
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
  date and tier (`2026-11-02 Written.jpg`), from whoever marked it signed. A writeup signed
  without a photo has nothing to save. Marking one signed never waits on it. A folder is known
  by its ID, so one renamed or moved still takes its copies, and one no longer there is made
  again. A try that fails is reported (`conduct.backup_failed`) and waits a minute, five,
  fifteen, an hour, then six hours between tries, for as long as it isn't saved; the rest of
  the pass waits for the next. While what keeps the DSP's files says it can't, nothing is
  tried. What's saved is everyone's who sees those files, and is never changed or taken back:
  Conduct keeps its own copy. Switched off, the backup stops, and what was saved stays.
- **Activity log:** a category added, changed, retired or restored, and a writeup deleted, under
  Settings (`frontend/audit-wording.ts`). A writeup keeps who wrote it and who marked it signed
  itself.

## Where things are

- `backend/standing.rs`: the tier rules, climbing and stepping down: where each person stands in
  each category, when they next step down, and where they stood before each writeup.
- `backend/categories.rs`, `writeups.rs`, `copies.rs`: their storage and checks.
- `backend/backups.rs`: backing signed copies up, and how it's going.
- `backend/overview.rs`: the page's one read, with each writeup's person as the directory knows
  them now.
- `frontend/tiers.ts`: how tiers read and climb on the page, from what the server works out,
  and what a writeup prints.
- `frontend/notice/`: the notice, laid out in points as the DSP's own form so it prints on one
  letter page, a section too long for its four lines shrinking to fit; its fixed wording in
  `wording.ts`; printing it alone; and in `fonts/`, the free fonts it falls back on where a
  computer lacks Arial Narrow, Arial or Snell Roundhand, each with its license.
- `frontend/compose/`: the writeup form; `draft.ts` fills it as a person and a category are
  chosen, keeping whatever someone changed.
- `frontend/drivers/`, `frontend/writeups/`, `frontend/settings/`: each screen;
  `settings/StepDown.tsx` sets a category's step down.
