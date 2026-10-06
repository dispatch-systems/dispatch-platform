# Daily Performance

Amazon Daily Quality and Day Safety, stored separately from posted Weekly Scorecards.
Cortex shares authentication, scope discovery and the performance HTTP reader. A daily job
reads exactly one date from 22 datasets with at most six concurrent requests.

The feature owns `daily_performance/daily_performance.sqlite`, publication history, indexed
rows and source coverage. Its switch, permissions (`view`, `collect`, `manage`), schedules,
jobs and agent allowances are independent of `weekly_scorecard`. Existing weekly grants do
not grant daily access. There is no dedicated frontend page.

Management routes use `/api/dsp/daily-performance`: the base lists collected days;
`/collect` accepts a date or an inclusive from/to backfill of up to 31 days; `/jobs`,
`/schedules` and `/policy` manage this collection alone. The policy defaults to rechecking
the previous seven dates after 20 hours, with configurable 1–30 lookback days and 1–168
refresh hours. Wall-clock times and cadence stay in the normal collection schedules.

Agents use `daily_performance` or `/api/v1/daily-performance`, choosing a dataset, a driver,
date/range/period, fields and optional groups. Summary counts recorded source rows. Full
detail pages project verified fields before loading rows; counts and filters run in SQL.
Group and detail cursors are independent. Ordinary period and response-size limits apply,
so longer requests can be split into adjacent ranges and every page retrieved.

When Weekly Scorecard is installed, the focused `customer_feedback`, `returns` and
`safety_events` tools also accept `source: daily_performance`. They require the daily_feedback, daily_returns or daily_safety
read allowance respectively. Omission keeps the weekly default.
Selection occurs before access checks; there is no fallback or mixing between sources.

Each capture is validated against its requested date and discovered company/station.
Publication is atomic and idempotent per job; recollection replaces the active publication
while retaining history.
Nonempty datasets have observed coverage. Empty responses stay unconfirmed and do not prove
zero events or completed work. Availability is reported for the selected dataset and date
range, independently of driver/event filters.

Daily feedback contains response counts, not individual package reviews. RTS fields retain
daily coaching and exemptions. Assessed and live safety datasets stay separate because
Amazon can report the same event ID in both. Contacts were empty during source inspection;
their field contract is not invented. Raw provider fields are retained for later validation,
while agent projections exclude location/video details.

The agent evaluation seed publishes daily rows through this same publisher for the existing
synthetic roster. Independent SQL expectations exercise recorded safety events, feedback
category counts and business-closed impact over ranges.
