# Routes

Each day's routes, itineraries, stops and packages from Cortex, kept for as
long as the DSP's retention window (Settings' Data page) says. Every response is stored
whole and compressed for reprocessing, beside the rows reads filter on. Its database keeps the
name `routedata` on disk.

- **Switch:** `routes`, off for every DSP until the platform owner switches it on. It needs a
  route source (Cortex), which switches on with it.
- **Permissions:** `routes.view`, to read the collected days; `routes.collect`, which includes
  it, to start or cancel a collection; and `routes.manage`, which includes it too, for its
  schedule and the retention window.
- **Collecting:** a final read of a completed day (yesterday by default, or up to a run of
  days), or a snapshot of today so far. A day older than the retention window is refused
  (`routes_day_outside_retention`), since the next hour would delete it again. Whichever read
  of a day was collected later is the day's, whatever order they're published in: it replaces
  the day's read whole, and one collected earlier is left for the sweep.
- **Saving a day:** a feature that depends on Routes saves a whole read of a day it collected
  itself into historical route data, as the standard collection saves a job's, with `save_day`
  (`backend/saving.rs`): under a key of its own, checked whole against the DSP's station and
  the day before anything is written, staged one itinerary at a time, and the day's read only
  if collected later than the one it has, with the day's work phones it's given. It tells
  Driver Match of new drivers at once (`people.changed`), and refuses while Routes is off.
- **Schedule:** the DSP's own rules, none made for it, set on Settings' Collectors page behind
  `routes.manage` (its keeper's `schedules`, which offers the choice), through core's
  `/api/dsp/collectors/routes` routes. Each rule reads one day of its choosing, its Day:
  **Yesterday** (collection `routes`, every schedule made before the choice), a final read of
  yesterday and of the days in the week before it without one, newest first, at most four a run;
  or **Today so far** (`routes_today`, a schedule alias of the routes collection), a snapshot of
  the day it is where the DSP is. Collect now there offers the same two. A snapshot is never a day's final read, so the first Yesterday
  run after the day ends still reads it whole, and catches it up if missed.
- **API:** behind `routes.view`, `GET /api/dsp/routes/days`, `…/days/{day}`,
  `…/days/{day}/itineraries/{id}`, `…/packages/{tracking}` and `…/jobs`. Behind
  `routes.collect`, `POST /api/dsp/routes/collect` and `…/jobs/{id}/cancel`. The platform
  owner rebuilds a DSP's rows from its stored responses with
  `POST /api/platform/dsps/{id}/routes/reprocess`, for every day or one, after a release that
  reads fields the earlier one didn't.
- **Data:** historical route data, in `routedata`: each day's read with its route list,
  itineraries, stops, tasks, breaks and unknown stops, and every response compressed; the
  addresses and drivers the days share, as the last read to become its day's read sent them
  (each read's are staged with it until then); and
  `work_phones`, each driver's holds on the company's work phones a day, from when until when,
  which are the day's read's: a saved day's, or none from the standard collection.
- **Retention:** from 30 to 3,650 days, or every day. Hourly, data past the window is retired,
  with its days' work phones, and then deleted in small steps, so the lock is never held long.
- **Settings:** `routes.retention`, "Route data", on the shared Data page
  (`?tab=data`), behind `routes.manage` (`backend/settings.rs`). Its one field, `days`, is a
  number from 30 to 3,650 with presets, or none for every day, the default. A window that passes
  a day the route data holds asks to confirm first, naming the first day it keeps as the expiry
  within the hour counts it: back from the day it is then where the DSP is, so the next day's in
  the hour before midnight. Expiry and collection requests read it with `settings::read`; core
  keeps it in the DSP's settings. The window a DSP chose before, in its route data's
  `route_retention`, moves in once as the server starts (`upgrade_storage`, through
  `settings::carry_over`), unless the section was saved already; the table stays, read only by
  that move.
- **Log:** `routes.collection_requested` and `routes.data_expired`, under Collections; a change
  of the window is core's `settings.changed`, and of its schedule core's `schedule.updated` and
  `schedule.toggled`, each rule in words with its Day; one from before reads a schedule of today
  so far as collecting "Routes, today so far". `routes.retention_changed`, from before Settings
  kept it, still reads.
- **People:** its drivers and their days are Driver Match's `routes` kind of data.
- **Exposed:** `driver_routes`, through its contract in `contract/` (`dispatch-routes-contract`),
  for any feature or agent tool, while it's on for the DSP: the source whose IDs name its drivers,
  the days collected, a driver's itineraries over some days at the DSP's station, and every
  driver's on one day (`day`), each with its route, van, wave, times, stops, packages delivered,
  not delivered and taken off, and every package not delivered by tracking ID, with which of those
  days were collected and which were read while under way. Today, read by a Today so far schedule
  or a snapshot asked for, is there as its last read left it, not complete, until the day's final
  read replaces it. A driver without a route didn't drive only on a complete day: one read while
  under way may be read before dispatch. Never addresses, locations, order IDs or phone numbers.
  IDs are matched exactly, and none reads the days alone. It refuses a day that isn't YYYY-MM-DD
  (`invalid_date`); and `from` after `to`, more than 366 days, more than 16 IDs, or an ID that's
  empty or over 128 characters (`invalid_input`).
