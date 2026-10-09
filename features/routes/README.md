# Routes

Each day's routes, itineraries, stops and packages from Cortex, read by agents and kept for as
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
- **Agents:** `routes`, `routes/{route}`, `packages` and `packages/{tracking}` under `/api/v1/`,
  read with the `routes` toggle, and delivery addresses and GPS only with `locations` too.
  Its drivers and their days are Driver Match's `routes` kind of data.
