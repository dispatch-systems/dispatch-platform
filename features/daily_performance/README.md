# Daily Performance

Amazon Daily Quality and Day Safety, stored separately from posted Weekly Scorecards.
Cortex shares authentication, scope discovery and the performance HTTP reader. A daily job
reads the dates it is asked for, oldest first, from 22 datasets, all at once: on its first
attempt each run of consecutive dates is one request per dataset, its rows given back to the
days they name, and a retry reads each date on its own.

The feature owns `daily_performance/daily_performance.sqlite`, publication history, indexed
rows and source coverage. Its switch, permissions (`view`, `collect`, `manage`), schedules
and jobs are independent of `weekly_scorecard`. Existing weekly grants do not grant daily
access. There is no page of its own: Settings draws its section.

Management routes use `/api/dsp/daily-performance`: the base lists collected days;
`/collect` accepts a date or an inclusive from/to backfill of up to 31 days; `/jobs` and
`/schedules` manage this collection alone.

Its Settings page (`?tab=daily-performance`) holds one section, `daily_performance.collection`
"Collection", behind `daily_performance.manage` (`backend/settings.rs`): `lookback_days`
(1–30, default 7) and `refresh_hours` (1–168, default 20), the dates a scheduled run rechecks
and how long one collected stays fresh, then its schedules, kept by core's schedule routes at
`/schedules`. Wall-clock times and cadence stay in those schedules. Scheduled runs read the
values with `settings::read`; core keeps them in the DSP's settings and records each change as
`settings.changed`. A policy kept before in its database's `settings` (`collection_policy`)
moves in once as the server starts (`upgrade_storage`, through `settings::carry_over`), unless
the section was saved already; the old value stays, read only by that move.
`daily_performance.policy_updated`, from before, still reads in the log.

Each day's capture is validated against the requested dates and discovered company/station.
A job publishes its days atomically, each idempotent per job and day; recollection replaces a
day's active publication while retaining history.
Nonempty datasets have observed coverage. Empty responses stay unconfirmed and do not prove
zero events or completed work.

A scheduled refresh queues one job for every date due, and a backfill of up to 31 days is
one job. Daily's keeper declares a bounded 31-job queue capacity: a job reads its days in as
many requests as one day takes. Worker/browser concurrency, idempotency and manual batch
exclusivity remain enforced; other collections retain their existing capacities.

Daily feedback contains response counts, not individual package reviews. RTS fields retain
daily coaching and exemptions. Assessed and live safety datasets stay separate because
Amazon can report the same event ID in both. Contacts were empty during source inspection;
their field contract is not invented. Raw provider fields are retained for later validation.

## People

Its drivers are Driver Match's `daily_performance` kind of data, counted in the days their
quality was reported, each showing them driving.

- **Exposed:** `daily_metrics`, through its contract in `contract/`
  (`dispatch-daily-performance-contract`), for any feature or agent tool, while it's on for the
  DSP: the days collected, and a driver's days under any of their Amazon IDs, from the active
  publications of the DSP's station. Each day has its quality, safety, feedback, delivery
  behavior and return numbers, its packages returned to the station, and its safety events, each
  once and as assessed when Amazon reports it both ways. A day counts as collected only once its
  driver quality was observed; any other is unknown. It leaves out raw rows, locations, videos
  and vehicles, matches IDs exactly, and refuses a day that isn't YYYY-MM-DD (`invalid_date`)
  and more than 31 days, more than 20 IDs, or an ID that's empty or over 128 characters
  (`invalid_input`).
