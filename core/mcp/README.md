# MCP

The MCP server and the agent API under `/api/v1/`, with its OpenAPI document and the Agent
Skill, all built from one catalog so they can't drift apart; agent keys and connected apps,
Sign in with Dispatch, usage limits and activity. Features bring their endpoints, facts, read
toggles and the skill's example questions in their `mcp/`. Tool names, paths and answers are what agents rely on, and nothing an
agent does appears in a DSP's activity log.

Collection queries accept a driver or everyone, a date, `from`/`to`, or periods such as
`past 14 days`. Routes and meal breaks default to yesterday; timecards default to yesterday
for everyone or 30 days for one driver. Other period tools default to 30 days. Periods span
up to 366 days, with a 92-day limit for payroll and meal assessments; longer requests use
nonoverlapping ranges. Weekly Scorecards select a posted week, defaulting to the latest.
DVIC holds short-inspection exceptions, including their durations; an empty exception list
does not establish that an inspection was completed.

Counts and totals cover the matching collected range, independently of `limit`. Details
remain available in pages of up to 500 rows within the 24 KB answer budget. Keep filters
unchanged when following `next_cursor`. With groups and details together, `groups_cursor`
advances groups and `cursor` advances the list; with groups alone, `cursor` retains its
original behavior. `driver_report(metrics: "short_inspections,packages_delivered")` reads
only the selected daily sources, reports unknown days as null and leaves day-only metrics'
period totals null. Source counts and detail pages use SQL where possible; payroll and
meal comparisons still assess bounded batches before paging their derived results.

Features can register an opt-in `view` contract for their agent queries. Adapters declare
their view name, cursor prefix, default period, query normalization and source grant. One
adapter can declare the shared default period for `compare`, which reads all registered
views for the same range. Discovery derives its choices and cursor parameters from these
declarations. These are agent API/MCP queries only; frontend APIs, databases, collection
jobs, history, policies and schedules remain independent. No view falls back to another
source. Each requested source needs its own read grant and enabled feature; compare checks
every grant before reading any source.

View answers carry `contract_version: 2`, source, count units and coverage. Summaries are
the default; `detail: full` or `list: true` adds bounded pages. `groups_cursor` and `cursor`
always page separate tables in v2. Compare nests answers under `sources`, keyed by the
registered view names. Each view has independent `<prefix>_cursor` and
`<prefix>_groups_cursor` parameters. Counts cover the complete filtered
range; page limits do not truncate totals. Comparison counts are never additive.
Omitting `view` preserves the existing source-specific contract and its default period.
Reconciliation and source-specific count semantics stay in their feature adapters and docs.
