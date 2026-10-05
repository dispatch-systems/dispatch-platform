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
