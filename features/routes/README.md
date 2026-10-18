# Routes

Each day's routes, itineraries, stops and packages from Cortex, kept for as
long as the DSP's retention setting (its Data tab in Settings) says. Every response is stored
whole and compressed for reprocessing, beside the rows reads filter on. Its database keeps the
name `routedata` on disk.

- **Switch:** `routes`, off for every DSP until the platform owner switches it on. It needs a
  route source (Cortex), which switches on with it.
- **Permissions:** `routes.view`, to read the collected days; `routes.collect`, which includes
  it, to start or cancel a collection; and `routes.manage`, which includes it too, for the
  schedules and the retention window.
- **Collecting:** a final read of a completed day (yesterday by default, or up to a run of
  days), or a snapshot of today so far. A day older than the retention window is refused
  (`routes_day_outside_retention`), since the next hour would delete it again. Schedules run
  through core's schedule routes under `/api/dsp/routes/schedules`, behind `routes.manage`.
- **API:** behind `routes.view`, `GET /api/dsp/routes/days`, `…/days/{day}`,
  `…/days/{day}/itineraries/{id}`, `…/packages/{tracking}` and `…/jobs`. Behind
  `routes.collect`, `POST /api/dsp/routes/collect` and `…/jobs/{id}/cancel`. Behind
  `routes.manage`, `GET` and `POST /api/dsp/routes/retention`. The platform owner rebuilds a
  DSP's rows from its stored responses with `POST /api/platform/dsps/{id}/routes/reprocess`,
  for every day or one, after a release that reads fields the earlier one didn't.
- **Retention:** from 30 to 3,650 days, or forever. Hourly, data past the window is retired,
  and then deleted in small steps, so the lock is never held long.
- **Its Settings tab** (`?tab=data`) shows the window and what it holds now, to those who
  manage routes.
- **Log:** `routes.collection_requested`, `routes.retention_changed` and `routes.data_expired`,
  under Collections.
- **People:** its drivers and their days are Driver Match's `routes` kind of data.
- **Exposed:** `driver_routes`, through its contract in `contract/` (`dispatch-routes-contract`),
  for any feature or agent tool, while it's on for the DSP: the source whose IDs name its
  drivers, the days collected, and a driver's itineraries over some days at the DSP's station,
  each with its route, van, times, stops, packages delivered, not delivered and taken off, and
  every package not delivered by tracking ID, with which of those days were collected and
  which were read while under way. A driver without a route didn't drive only on a complete
  day: one read while under way may be read before dispatch. Never addresses, locations, order
  IDs or phone numbers. IDs are matched exactly, and none reads the days alone. It refuses a
  day that isn't YYYY-MM-DD (`invalid_date`); and `from` after `to`, more than 366 days, more
  than 16 IDs, or an ID that's empty or over 128 characters (`invalid_input`).
