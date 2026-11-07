# Backups

Each DSP's backups, which only the platform owner sees and sets, on the DSP's Backups tab of the
platform owner's DSPs page. A DSP has a schedule of its own: rules made as a collection's are,
each on chosen days at set times or every so often between two times, where the DSP is, with how
long what it takes is kept (days, weeks, months or years) and what of the DSP it holds. Every
DSP starts with one rule: every day at 2:00 AM, kept 30 days, all but its pictures. Back up now
takes one by hand, kept 30 days, holding everything.

## Where backups are kept

Backups go to a bucket of their own, never the pictures' bucket, with a token of its own, and are
sealed with a key of their own, which the platform owner keeps off the server too: with the bucket
and the key alone, every backup reads back. The environment names them
(`DISPATCH_PRODUCTION_` or `DISPATCH_DEV_` before each):

- `BACKUPS_R2_ACCOUNT_ID`, `BACKUPS_R2_BUCKET`, `BACKUPS_R2_ACCESS_KEY_ID`,
  `BACKUPS_R2_SECRET_ACCESS_KEY`: the bucket and its token.
- `BACKUPS_KEY`: 64 hexadecimal digits, 32 random bytes (`openssl rand -hex 32`).

None set, no backup is taken and the tab says so. Only some set, the server refuses to start.

## What a backup holds

What a backup can hold comes from the registry, never a list here: the DSP's own data
(`dispatch`, always), each collector's database and each feature's added databases under their
own names, its saved logins (its connections' sealed credentials and the key beside them, and the
browsers' signed-in profiles), and its pictures (files its features keep beside their databases,
and those kept in the object store). `cache`, what its owners collect again, is never held.

Each file is copied while the server runs, a database as SQLite copies one in use, then
compressed and sealed with AES-256-GCM in pieces bound to their place and the DSP
(`backend/seal.rs`). It's kept by a keyed hash of what it holds and of the DSP's generation, a
random name kept with its schedule and new with each platform database, so a file that hasn't
changed isn't sent again, and a server started over with an empty database never shares, nor
lets go of, a file of an older one's backups. Each file is read back once sent, and opened, before
the backup counts as taken. Then the backup's description is kept beside them, sealed the same
way: the DSP as the platform knows it, its features, the release that took it, its generation
and each file with what it belongs to. A restore needs nothing else.

## Ownership

- `backend/` owns the backups' SQL, their schedule, taking, keeping and letting go of them, and
  the zip a backup is downloaded as. What takes them (`keeper.rs`) runs beside the server,
  outside requests, once a minute or at once when one is asked for by hand, one at a time.
- `api/` owns the routes, Rust wire types, generated TypeScript and the browser's calls.
- `frontend/` owns the Backups tab. It edits rules with the Collectors page's editor and week,
  which `core/collection/frontend/schedule.ts` shares.
- `migrations/platform/0026_dsp_backups.sql` adds the schedules, rules and backups to the
  platform database, beside each DSP's folder, so a restore of that folder neither forgets the
  backups taken since nor moves when they run.

## Keeping and letting go

Each backup expires as its rule says, a month as the calendar has it. Once it has, its
description goes, then each file it held that no backup kept longer holds, then its row: a pass
cut short leaves what's left to the next. Only files a backup the platform knows held are let go
of. A backup that fails holds what it sent until it expires, and its rule runs again within the
hour. Deleting a DSP forgets its schedule but keeps its backups until each expires.

## Not yet

Restoring a backup. Each backup's description is what a restore reads; the platform database
keeps the history a restore of a DSP's folder won't change.
