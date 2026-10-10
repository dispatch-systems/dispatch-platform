# Weekly Scorecard

Amazon's posted weekly scorecards, one week per Cortex job. The same Cortex connection and
performance reader serve Daily Performance; weekly publications remain a separate source,
with their own permissions, schedules and storage.

The current names are `weekly_scorecard`, `cortex.weekly_scorecard.collect`,
`weekly_scorecard.view/collect/manage` and `/api/dsp/weekly-scorecard`. There is no
dedicated page.

Storage is `weekly_scorecard/weekly_scorecard.sqlite`. Historical publications and source
rows are retained when a week is collected again. Provider daily datasets inside a weekly
capture are details of that week's publication; they do not become Daily Performance data.

Weekly schedules refresh recent completed weeks, including already posted weeks whose
dispute outcomes can change. `/policy` defaults to one recent week and a 20-hour refresh,
configurable from 1–8 weeks and 1–168 hours. Schedule times and cadence are independent of
daily schedules. Manual recollection can target older weeks.

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
