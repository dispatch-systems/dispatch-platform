# Daily Performance

Amazon Daily Quality and Day Safety, stored separately from posted Weekly Scorecards.
Cortex shares authentication, scope discovery and the performance HTTP reader. A daily job
reads exactly one date from 22 datasets with at most six concurrent requests.

The feature owns `daily_performance/daily_performance.sqlite`, publication history, indexed
rows and source coverage. Its switch, permissions (`view`, `collect`, `manage`), schedules
and jobs are independent of `weekly_scorecard`. Existing weekly grants do not grant daily
access. There is no dedicated frontend page.

Management routes use `/api/dsp/daily-performance`: the base lists collected days;
`/collect` accepts a date or an inclusive from/to backfill of up to 31 days; `/jobs`,
`/schedules` and `/policy` manage this collection alone. The policy defaults to rechecking
the previous seven dates after 20 hours, with configurable 1–30 lookback days and 1–168
refresh hours. Wall-clock times and cadence stay in the normal collection schedules.

Each capture is validated against its requested date and discovered company/station.
Publication is atomic and idempotent per job; recollection replaces the active publication
while retaining history.
Nonempty datasets have observed coverage. Empty responses stay unconfirmed and do not prove
zero events or completed work.

Daily's keeper declares a bounded 31-job queue capacity so the supported 31-day backfill and
default seven-day scheduled refresh can be admitted atomically. Worker/browser concurrency,
idempotency and manual batch exclusivity remain enforced; other collections retain their
existing capacities.

Daily feedback contains response counts, not individual package reviews. RTS fields retain
daily coaching and exemptions. Assessed and live safety datasets stay separate because
Amazon can report the same event ID in both. Contacts were empty during source inspection;
their field contract is not invented. Raw provider fields are retained for later validation.
