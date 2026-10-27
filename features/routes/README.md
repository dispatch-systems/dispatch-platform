# Routes

Each day's routes, itineraries, stops and packages from Cortex, kept for as
long as the DSP's retention window (Settings' Data page) says: historical route data. Every
response is stored whole and compressed for reprocessing, beside the rows reads filter on. Its
database keeps the name `routedata` on disk. Today as a feature reading routes live last read
it is live route data, in a database of its own, `routelive`.

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
  if collected later than the one it has, with the day's work phones it's given, which a later
  standard read keeps. A read in a mode its day didn't allow when it was read (its
  `finished_at`, where the DSP is), a final read of a day not yet ended or a snapshot of another
  day than it was then, is refused (`routes_day_not_available`). It tells Driver Match of new
  drivers at once (`people.changed`), and refuses while Routes is off.
- **Live route data:** a feature that depends on Routes and reads today live asks Cortex for
  what `live_request` names, a snapshot of today where the DSP is at its station, as a job asks
  for a day; and keeps each whole read with `keep_live` (`backend/live.rs`), counting for its read's record the itineraries
  listed, the rows written or deleted, the work phones held and the holds claimed, moved or
  ended (`LiveKept`). A read is checked whole as a snapshot of today where the DSP is, at its
  station, in the scope its days are read in, and kept only while the DSP's Cortex connection has
  the revision the read was taken under, checked in the transaction that keeps it
  (`connection_changed`, as after its credentials were replaced or it was disabled); then kept in
  place in one transaction, so a reader
  sees the last read or this one whole: a row new to it is inserted, one that changed updated,
  one it no longer holds deleted, one unchanged left unwritten. Its responses are kept in place
  too, so the last whole read can be read again as it came. A read that isn't whole changes
  nothing: an itinerary its list names missing, a list naming none once the day has had an
  itinerary, which is a lost read (`routes_capture_invalid`, `routes_scope_mismatch`,
  `routes_source_too_large`); so do another day than today (`routes_day_not_available`) and a
  read collected no later than the one kept. A day or scope other than the one live route data
  holds starts it afresh. The drivers its route list assigns are named before their itinerary
  is read, and Driver Match hears of each driver it names first (`people.changed`).
  `clear_live` empties it and gives the file's space back, as a day starts or once a stopped
  day is saved; `save_live_day` saves the last whole read it holds into historical route data
  through `save_day`, with every hold of the day, under the key `live:<day>:<collected ms>`, so
  saving it again saves nothing twice: a final read if it was read once its day had ended where
  the DSP is, a snapshot otherwise, so a day's later final read replaces it. It answers none
  when live route data holds no day, and leaves live route data as it was.
  `save_and_clear_live` saves the day so, then empties live route data, as a stopped day does:
  once it's saved, or once it holds no day, as after an earlier call emptied it but couldn't give
  its space back. All four refuse while Routes is off.
- **Each itinerary's work phone:** every itinerary keeps the company phone its driver's records
  show with it, by its last ten digits, in both route databases (`backend/work_phones.rs`): the
  day's list's record of them, else the itinerary's own response's, so a list record without a
  phone gives way to the itinerary's. Only the DSP's own drivers have an itinerary; one only
  assigned to a route has no phone. A day's read is shaped with them from adapter version 3; a
  saved day kept by the version before has them extracted from its stored responses by the
  minute's upkeep (`backend/phone_backfill.rs`), four days a DSP at a time, newest first, which
  writes only the phones and the version, so the drivers and addresses the days share are left as
  they are; an older day waits for a reprocess, which shapes it whole. A driver's latest of a day
  is the one on their itinerary that began last, by when it started, or else was planned to
  depart, of those showing one; of two no time tells apart, the one whose ID sorts last. Nothing
  is tracked over time: of two drivers who swapped phones, each has the one they swapped to.
- **Route data changed:** every feature that hears it is told a day's route data changed, as it's
  kept, with the day (`routes.data_changed`, after `people.changed` when a live read or a save
  names drivers anew): after a live read that changed a row of the day's itineraries, drivers or
  routes, or brought a day new to live route data, rather than only the responses it keeps whole,
  as the list's, whose `backendTime` changes read by read; each time live route data is emptied,
  of the day it held, or today once it holds none; once a read of a day saved becomes its read; and
  once a day's work phones are extracted. A collection, and a reprocess, change a day
  where nothing is at hand to tell of it, so the day is noted in the server's memory
  (`backend/changed.rs`) and told once Routes hears its collection succeeded
  (`collection.succeeded`), or by the next minute's upkeep, whichever is first; a restart before
  then tells of none.
- **Work phones:** who holds which of the company's work phones is settled from each whole live
  read, in the transaction that keeps it (`backend/phones.rs`), from what each driver's record
  shows: the list's phone, else their itinerary's, by its last ten digits. A driver claims a
  phone once signed in, when their record changes to it, never by showing it again: until then
  it shows the last phone they had. Signed in is a driver event today, or a record changed from
  the first phone it showed today, read for every driver the list names, assigned only or
  dispatched; a claim made earlier is withdrawn. A driver holding a phone switches to another
  once two reads in a row show it. A claim of a phone someone holds is a handover, ending theirs,
  and their record still showing it never takes it back. A hold ends only with a switch or a
  handover: a driver keeps the last phone they held once Cortex ends their session or they're
  gone from the day's list, until someone else claims it. Claims are settled oldest activity first, so the most
  recently active driver holds a phone two records show. Every hold is kept, from its claim
  until it ends or the day does, so a restart neither repeats a claim nor loses one; and saved
  with the day into historical route data, which keeps each day's for the retention window.
  Every feature that hears it is told once who held a phone on a day changed, as it's kept, with
  the day (`routes.work_phones_changed`, after `people.changed` when both are): after a live read
  that claimed, moved, handed over or withdrew a hold, of the read's day; each time live route
  data is emptied, of the day it held, or today once it holds none; and once a day saved becomes
  the day's read, its holds with it, of that day.
- **Schedule:** the DSP's own rules, none made for it, set on Settings' Collectors page behind
  `routes.manage` (its keeper's `schedules`, which offers the choice), through core's
  `/api/dsp/collectors/routes` routes. Each rule reads one day of its choosing, its Day:
  **Yesterday** (collection `routes`, every schedule made before the choice), a final read of
  yesterday and of the days in the week before it without one, newest first, at most four a run;
  or **Today so far** (`routes_today`, a schedule alias of the routes collection), a snapshot of
  the day it is where the DSP is. Collect now there offers the same two. A snapshot is never a day's final read, so the first Yesterday
  run after the day ends still reads it whole, and catches it up if missed. What it collects,
  each Day's period among it, as the Collectors page shows under it, is `frontend/collected.ts`
  (the `collected` slot).
- **API:** behind `routes.view`, `GET /api/dsp/routes/days`, `…/days/{day}`,
  `…/days/{day}/itineraries/{id}`, `…/packages/{tracking}` and `…/jobs`. Behind
  `routes.collect`, `POST /api/dsp/routes/collect` and `…/jobs/{id}/cancel`. The platform
  owner rebuilds a DSP's rows from its stored responses with
  `POST /api/platform/dsps/{id}/routes/reprocess`, for every day or one, after a release that
  reads fields the earlier one didn't.
- **Data:** each itinerary's work phone in its row, in both; in historical route data found by
  the phone too, for whether a number was ever one. Live route data, in `routelive`: today's one
  read, in historical route data's tables
  but its staged rows, with only the indexes a day's reads need; the drivers it names, those only
  assigned too; every hold of the day in `work_phones`; and in `work_phones_shown`, what each
  driver's record showed of a phone, which holds are settled from: the first phone today, the
  one at the last read listing them, and the one they last claimed.
  Historical route data, in `routedata`: each day's read with its route list,
  itineraries, stops, tasks, breaks and unknown stops, and every response compressed; the
  addresses and drivers the days share, as the last read to become its day's read sent them
  (each read's are staged with it until then); and
  `work_phones`, each driver's holds on the company's work phones a day, from when until when,
  which are the last saved day's: the standard collection reads none, so its read keeps them;
  found by phone too, for whether a number was ever one.
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
- **People:** its drivers and their days are Driver Match's `routes` kind of data; those live
  route data names, `routes_live` ("Live routes"), whose phones are never a contact: a work
  phone passes from driver to driver.
- **Exposed:** `driver_routes`, through its contract in `contract/` (`dispatch-routes-contract`),
  for any feature or agent tool, while it's on for the DSP: the source whose IDs name its drivers,
  the days collected, a driver's itineraries over some days at the DSP's station, and every
  driver's on one day (`day`), each with its route, van, wave, times, stops, packages delivered,
  not delivered and taken off, and every package not delivered by tracking ID, with which of those
  days were collected and which were read while under way. Today, read by a Today so far schedule
  or a snapshot asked for, is there as its last read left it, not complete, until the day's final
  read replaces it. A driver without a route didn't drive only on a complete day: one read while
  under way may be read before dispatch. Never addresses, locations or order IDs, nor a phone
  number but a work phone's. IDs are matched exactly, and none reads the days alone. It refuses a day that isn't YYYY-MM-DD
  (`invalid_date`); and `from` after `to`, more than 366 days (`MOST_DAYS`), more than 16 IDs
  (`MOST_IDS`), or an ID that's empty or over 128 characters (`MOST_ID_LENGTH`,
  `invalid_input`). And
  `live_route_data` (`LiveRouteData`), while it's on: today as live route data holds it, read the
  same way, without the days collected: the live day, every driver's routes, or those of up to 16
  IDs, each a route they're assigned to and haven't started too, as planned; the work phone each
  driver holds now, since when; and every hold of the day, ended ones too, each driver's phone by
  its ten digits from when until when (`work_phone_holds`). Nothing of a day that isn't today;
  never complete. It refuses IDs as `driver_routes` does. `driver_routes` reads a saved day's
  holds the same way (`work_phone_holds`, refusing a day as `day` does), and whether a number, by
  its last ten digits, was ever held as a work phone in live or historical route data
  (`was_work_phone`): a work phone nobody holds now is still one. Each driver's latest work phone
  of a day reads from both, a saved day's (`DriverRoutes::day_phones`, refusing a day as `day`
  does) and today's (`LiveRouteData::day_phones`): each driver ID once with their phone by its ten
  digits, and whether it's another driver ID's latest that day too, which names no one driver; a
  day not collected, or not live, and one whose phones aren't extracted yet, are told apart from a
  day without phones. And whether a number, by its last ten digits, is an itinerary's work phone
  on any day kept, live or historical (`ever_work_phone`).
