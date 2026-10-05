# Weekly Scorecard

Amazon's weekly scorecard from Cortex's performance API, one week per job, with every row
Amazon sends kept as JSON beside the keys reads filter on. It has no page: agents read it, and
its frontend fills the platform owner's slots.

`weekly_scorecard` owns this feature, its permissions, schedule collection, agent read
area and source. Its collection jobs are `cortex.weekly_scorecard.collect`; agents call
`weekly_scorecard` or read `/api/v1/weekly-scorecard`. Management routes live under
`/api/dsp/weekly-scorecard`.

Schedules check for the latest completed week's publication. They may check daily or at
an interval; a published week is skipped, and an unpublished week is checked again after
the existing cooldown. The provider's daily event datasets are details of that weekly
publication, rather than a separate daily collection.

During the transition, the old MCP tool and `/api/v1/scorecard` and `/api/dsp/scorecard`
routes call the same handlers with the same permissions. Discovery advertises the new
name once. The feature's identifier mappings read either spelling and keep durable
switches, role permissions, agent reads, schedule collections, job kinds and requests in
the spelling the previous release understands. Historical audit actions remain readable.

The existing `scorecard/scorecard.sqlite` file, storage marker, storage identity, table
names and migration ledger remain unchanged, preserving publications and rollback.
Retiring the compatibility mappings and legacy routes requires a later release. A future
daily feature must declare its own feature, collector job, permissions, schedules, agent
source and storage; weekly permissions and agent access grant nothing to it automatically.
