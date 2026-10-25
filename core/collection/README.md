# Collection

The job engine, schedules, live results, live readers, checkpoints, metrics, the collection
browser and the Connections and Collectors pages. It runs what the registry's collectors declare and hands each
result to the one feature that keeps it, naming neither. A collection runs only while the
feature that keeps it is on, and each feature serves its own jobs under routes core builds for
it; its schedules are set on the Collectors page, through core's routes.
What a browser or a site sends is untrusted input, and a job's metrics keep only timings,
counts, memory and fixed labels: what failed, how a collector that reads in more than one
way read it, and why it fell back from its first way.

A page follows its collections live with `useCollectionUpdates()` (`frontend/live-collection.ts`):
one waiting request on `/api/dsp/collection-updates`, which the permissions features list in
`live` may make, wakes as a run ends or a collector shows results partway, and its feature's
`collected` and `collection` cache rules say which reads that refreshes. A member hears only of
the collections kept by features whose `live` permissions it holds, so a hint, such as the
employee code a change names, stays with its feature's readers. A job's status isn't
announced, so a page shows a run's progress by reading its status while it runs.

Live readers (`backend/readers/`) run what features declare in `live_readers` within the
server, beside the jobs rather than as them (`manifest/README.md`, "Live readers", says what a
feature declares). The server plans each served DSP's reading as it starts, each minute, and at
once when a write switches a feature or saves a DSP's settings; one light task for each DSP
whose reader's feature is on and whose window is set waits for each slot of its grid, reads,
hands a whole read to its reader to keep, and stops each day it started once its window ends.
At most `READS_AT_ONCE` reads run at once across the server; a read that can't start within
its interval misses its slot (`live_reads_busy`), and one whose turn comes once its window has
closed or its plan has changed doesn't start. Started again, a DSP reads at its next slot after
its last read, at once only when that read is over an interval old, so a restart reads no
burst. A task busy for three of its intervals without moving on, in a read, a hook or its turn
to read, is stopped, letting go of its turn, and started again alone, its memory made anew,
while its DSP still reads; it's logged as `live.stalled`.

What it keeps is in the environment's jobs database: `live_reads`, each read's day, when it
began, how long it took, and whether it was kept, with what it counted, or why not, by its
code, `live_read_stopped` for one dropped as its reader stopped; and `live_days`, the days each
reader started and stopped for each DSP, which a restart goes on from. A DSP's records from
before the last two weeks go as each of its days' reading starts. Its log has `live.started`
and `live.stopped` for each day, and `live.read_failed` with a failed read's code, never what
it read.

A hook's browser (`ProviderBrowser`) is opened as a job's is, through the same limits on
browsers and memory, under the reader's feature's authority: refused while that feature is
off for the DSP or the DSP is inactive, and closed as the reader stops for the DSP, so what a
hook still holds of it fails from then on. It never uses a browser another opened for the DSP
(`connection_busy`), holds its own as a collecting job does, so neither a job nor the idle
sweep takes it, and no job's end closes it; it closes rather than wait for an owner's
verification (`verification_required`), and when dropped. A start dropped partway, as a read
cut off, closes what it registered, as any start of a provider's browser now does. What a hook
takes from a sign-in to keep after the browser closes, as a provider's cookies, it binds to
`Store::connection_revision(dsp, provider)`: the connection's revision while it's enabled, none
while it's disabled. Replacing the credentials or disabling the connection moves it on, so what
was taken under another revision is stale and goes unused.

Its Connections page is a custom section of a DSP's Settings, `collection.connections`, on the
Connections page: the server shows it to every member, and its component in
`frontend/feature.ts` says whether they have anything there.

Its Collectors page is another, `collection.collectors`, on the shared page `collectors`
(`frontend/collectors/`): every collection whose schedule the member sets, under the collector
that reads it, with its schedule in words, when it ran last and runs next, or what a run under
way says it's doing, on or off, and Collect now; then the sections of Settings that go with it
(`.collection(..)`), which Settings draws there; and under it what the collection collects,
as the feature that keeps it says (the `collected` slot), with how many of each kind its latest
successful run brought, from that run's metrics (`counts`). It follows each collection's end as it comes,
and reads the collections every few seconds while one runs. Who sets a collection's schedule is its keeper's to say (`Keeper::schedules`), and
the page shows only to those holding one of those permissions (`schedulingPermissions`, which
the access catalog generates). No collection is scheduled until a member makes it a rule.

A collection's schedule is its rules (`backend/rules.rs`), each a row of `collection_schedules`:
the days of the week it runs on, then either up to 12 times, or every 30 minutes to 12 hours,
in half hours, from one time to another, the last included; an end before its start runs on
past midnight, and the same as its start all day. Times are where the DSP is: a local time a
clock change repeats runs once, at its first, and one it skips doesn't run. A rule may read
its collection another way when its keeper offers the choice, as Routes' rules read Yesterday
(`routes`) or Today so far (`routes_today`, a schedule alias). The rules are saved together,
against the revision they were read at, and the collection is on or off as a whole; its first
rules start it on once its collector is connected, and off till then, and coming on needs it.
Each row also keeps the one time, or the start and interval, the previous release runs it by,
beside what this release wrote of it, so a row that release wrote, or changed since, reads as
it runs it: one time every day, a repeat all day from its start, or, repeating less often than
a rule may, the times it falls at.

The scheduler queues every rule due in one batch, so the collections due together run
together, each collection once however many of its rules are due: the others it runs for move
on first, so a stop once the batch is queued finds only the first still due, and its run
queued. A rule that can't run yet keeps its run and says why, tried again in a minute. A run
queues its keeper's `scheduled` jobs, handed the name the rule collects by. Timecard's `both`,
which ran Paycom's timecards and Cortex's meal breaks at once, is retired: each of its
schedules becomes one rule of each, timed as it was, as a DSP opens
(`Feature::retired_schedule_aliases`).

Routes, under `/api/dsp/collectors` (`api/routes/collectors.rs`):

- `GET /api/dsp/collectors`: the collections the member sets, in the DSP's timezone.
- `POST …/{collection}/schedule` `{rules, revision}`: replaces its rules. A rule with an `id`
  keeps its row, and unchanged its pending run; one without is new; one left out is removed.
  A stale revision or an unknown id is `schedule_changed`; at most 50 rules a DSP.
- `POST …/{collection}/enabled` `{enabled, revision}`: turns every rule on or off.
- `POST …/{collection}/preview` `{rules}`: their next five runs, unsaved.
- `POST …/{collection}/collect` `{collects, requestId}`: queues now what one run of a rule
  collecting by `collects` queues, under the permission its collection runs under.

`{collection}` is a collection's own schedule name. The first four are open to a member holding
any collection's schedule permission, then refuse one who lacks that collection's; a save or
switch is logged as `schedule.updated` or `schedule.toggled`, Collect now as
`collection.started`.
