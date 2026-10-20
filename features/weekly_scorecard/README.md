# Weekly Scorecard

Amazon's posted weekly scorecards, one week per Cortex job. The same Cortex connection and
performance reader serve Daily Performance; weekly publications remain a separate source,
with their own permissions, schedules and storage.

The current names are `weekly_scorecard`, `cortex.weekly_scorecard.collect`,
`weekly_scorecard.view/collect/manage` and `/api/dsp/weekly-scorecard`. There is no
page of its own: Settings draws its section.

Storage is `weekly_scorecard/weekly_scorecard.sqlite`. Historical publications and source
rows are retained when a week is collected again. A week recollected with fewer rows in any
dataset than its active publication read, as Cortex answers while it republishes a week, is
kept beside it, with its counts alone, until a read at least half an hour later (twelve hours
for a dataset gone empty) sees the same counts, as a dispute that removed rows settles. Provider daily datasets inside a weekly
capture are details of that week's publication; they do not become Daily Performance data.

Weekly schedules refresh recent completed weeks, including already posted weeks whose
dispute outcomes can change. Schedule times and cadence are independent of daily schedules.
Manual recollection can target older weeks.

Its Settings page (`?tab=weekly-scorecard`) holds one section, `weekly_scorecard.collection`
"Collection", behind `weekly_scorecard.manage` (`backend/settings.rs`): `lookback_weeks` (1–8,
default 1) and `refresh_hours` (1–168, default 20), the weeks a scheduled run rechecks and how
long one checked stays fresh, then its schedules, kept by core's schedule routes at
`/api/dsp/weekly-scorecard/schedules`. Scheduled runs read the values with `settings::read`;
core keeps them in the DSP's settings and records each change as `settings.changed`. A policy
kept before in its database's `settings` (`collection_policy`) moves in once as the server
starts, after the import below (`upgrade_storage`, through `settings::carry_over`), unless the
section was saved already; the old value stays, read only by that move.
`weekly_scorecard.policy_updated`, from before, still reads in the log.

Startup retires old Dispatch identifiers: saved switches (including disabled states), role
permissions, collection schedules, job kinds/requests and audit action prefixes migrate to
their weekly names. IDs, restrictions and schedule timing are preserved. New requests reject
old permissions, collection names and routes.

A marked prior `scorecard/scorecard.sqlite` is imported into the new database in a transaction.
All publications, rows and source metadata keep their IDs and verified-scope flags. A durable
receipt and count/foreign-key checks make retries safe. Unverified historical scope remains
quarantined. Missing or conflicting marked state fails closed.

After import the prior directory moves to the DSP's
`state/weekly_scorecard_migration_backup`; it is an inactive recovery archive.
Matching existing archives are reused after verifying every remaining source file. Moves
across filesystems use a private staged copy, verify and sync it, then remove the source.
Conflicting archives stop before the new marker is committed and retain both directories.
Historical shipped SQL files remain unchanged for migration evidence. Automatic rollback to a binary
that only understands retired identifiers is not supported after this transition; recovery
requires restoring a consistent pre-transition snapshot of the platform, jobs and DSP data.

## People

Its scorecards' drivers are Driver Match's `weekly_scorecard` kind of data, counted in the
weeks they were scored, each showing them driving.

- **Exposed:** `driver_scorecards`, through its contract in `contract/`
  (`dispatch-weekly-scorecard-contract`), for any feature or agent tool, while it's on for the
  DSP: the weeks collected and, for drivers' Amazon IDs, their scorecards and the packages they
  list (concessions, returns and customer feedback). A scorecard or package counts in the days
  asked when its Sunday-to-Saturday week overlaps them. It reads only the active, verified
  publications at the DSP's station, as its weeks do, and never gives raw rows, locations,
  addresses or coaching. It refuses a day that isn't YYYY-MM-DD (`invalid_date`), and
  more than 366 days, more than 32 IDs or an ID that's empty or over 128 characters
  (`invalid_input`).
