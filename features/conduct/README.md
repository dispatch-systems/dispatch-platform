# Conduct

A DSP's writeups of its drivers, and where each driver stands. A writeup is a Corrective Action
Notice in a category (Safety, Excessive Tardies, POD, …), at a tier: Verbal, Written or Final, a
final one just that, with a suspension of a length someone types, or pending further
investigation. Each category climbs its tiers on its own, and steps back down in time: each of
its tiers waits a set number of days, weeks or months, or forever, then steps down to a lower
tier or to a clean record. The wait runs from the driver's latest writeup in the category, and
each step down starts the next tier's. A driver's next writeup in it is suggested one tier above
where they stand, never below the tier the category starts at, and Final again once they're at
Final; written, it puts them at its tier, or keeps them higher, and starts the wait again. The
page prints the notice; the driver signs it on paper; someone marks it signed, with a photo of
the paper if they like.

- **Switch:** `conduct`, off for every DSP until the platform owner switches it on. It needs no
  connection.
- **Permissions:** `conduct.use`, to see every writeup, write one up, change or delete one not yet
  signed, and mark one signed. `conduct.manage`, which includes it, to add, change, retire and restore
  categories, their step downs among them. Only owners hold them until a role
  grants them.
- **Page:** Conduct, in a DSP's sidebar: Drivers (each driver at a tier, their tier in each
  category) and Writeups (every writeup), a driver's own page (each category's tier and when it
  next steps down), a writeup's, and the form that
  writes or changes one beside its notice. What it shows is in the address (`frontend/address.ts`)
  so Back and links work.
- **API** (`api/routes.rs`): `GET /api/dsp/conduct` reads everything the page draws, and every
  write answers with it again, so the page shows a change at once. Writeups under
  `/api/dsp/conduct/writeups[/{id}[/sign|/delete|/copy]]`, behind `conduct.use`; categories under
  `/api/dsp/conduct/categories[/{id}[/retire|/restore]]`, behind `conduct.manage`.
- **Data**, in each DSP's own database:
  - `conduct_categories`: each category's name, starting tier and the wording of the three
    sections a new writeup starts with, `[…]` marking a blank. Every DSP starts with twelve
    (`migrations/dsp/0001_conduct.sql`), each starting at Verbal. A retired one stays for its
    writeups.
  - `conduct_step_downs`: each category's tiers' waits, and what each steps down to. A tier
    without one never steps down. The twelve start one tier at a time, six months at each.
    Where everyone stands is worked out on every read, so a changed wait applies to everyone at
    once; writeups stay as they were written.
  - `conduct_writeups`: each writeup as it prints. It names its person by their code in core's
    directory (`people::everyone`) and keeps their name as printed; the page shows whoever the
    code names now, a merged person's code finding whoever kept their IDs. Signed, it never
    changes again, nor can it be deleted. Edits name the revision they read.
  - `conduct_copies`: the photo of a writeup's signed paper, which the page shrinks to 2000
    pixels before sending. A signed writeup's photo, once kept, stays.
- **Settings** (`backend/settings.rs`), on its own page of Settings (`?tab=conduct`), behind
  `conduct.manage`: `conduct.categories`, the categories with their step downs, drawn by
  `frontend/settings/CategoriesSection.tsx` and saved through Conduct's own routes.
- **Activity log:** a category added, changed, retired or restored, and a writeup deleted, under
  Settings (`frontend/audit-wording.ts`). A writeup keeps who wrote it and who marked it signed
  itself.

## Where things are

- `backend/standing.rs`: the tier rules, climbing and stepping down: where each person stands in
  each category, when they next step down, and where they stood before each writeup.
- `backend/categories.rs`, `writeups.rs`, `copies.rs`: their storage and checks.
- `backend/overview.rs`: the page's one read, with each writeup's person as the directory knows
  them now.
- `frontend/tiers.ts`: how tiers read and climb on the page, from what the server works out,
  and what a writeup prints.
- `frontend/notice/`: the notice, its fixed wording in `wording.ts`, and printing it alone.
- `frontend/compose/`: the writeup form; `draft.ts` fills it as a person and a category are
  chosen, keeping whatever someone changed.
- `frontend/drivers/`, `frontend/writeups/`, `frontend/settings/`: each screen;
  `settings/StepDowns.tsx` sets a category's step downs.
