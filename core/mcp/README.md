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

Focused `returns`, `safety_events` and `customer_feedback` offer an opt-in `view` contract:
`operational` reads Daily Performance (default yesterday), `posted_scorecard` reads Weekly
Scorecard (default last week), and `compare` reads both for the same days (default last week).
These are agent API/MCP queries only; frontend APIs, databases, collection jobs, history,
policies and schedules remain independent. Neither view falls back to another source.
Each requested source needs its own read grant and enabled feature; compare checks both
before reading either. Feature-owned adapters declare their views in the manifest.

View answers carry `contract_version: 2`, source, count units and coverage. Summaries are
the default; `detail: full` or `list: true` adds bounded pages. `groups_cursor` and `cursor`
always page separate tables in v2. Compare nests answers under `sources.operational` and
`sources.posted_scorecard`, with independent `operational_cursor`, `posted_cursor`,
`operational_groups_cursor` and `posted_groups_cursor`. Counts cover the complete filtered
range; page limits do not truncate totals. Comparison counts are never additive.
Omitting `view` preserves the existing source-specific contract and weekly default.

Operational returns reconcile active cumulative snapshots by tracking ID, planned delivery
date and transporter ID, keeping the latest annotation before filtering impact or contact.
The date filter applies to delivery attempts, while later snapshots can improve past detail.
Missing identity fields remain separate and are counted under `unidentified`.
Where available, `reported_returns` gives the independent daily aggregate; `reconciliation`
reports detail/aggregate mismatches explicitly. Daily annotations do not establish weekly
scorecard impact. Operational safety merges live and assessed copies by event ID, prefers
assessed outcomes and labels live-only events pending. Posted safety retains weekly dispute
outcomes. Daily feedback response counts and weekly package feedback records keep different
units. Empty daily responses remain unconfirmed, never assumed zero.
