# Timecard

Paycom's employees, punches and timecards and the meal breaks Cortex reports, on the Timecard
page (Daily, Meal Breaks and Employee Search) and its page of Settings. Rust interprets punches and
assesses meals; the frontend only formats the results. Its addresses keep their old names
(`#dsp/<id>/paycom`, `/api/dsp/paycom/…`), and its jobs and schedules keep theirs from when
they were the only ones (`/api/dsp/jobs`, `/api/dsp/schedules`), though they serve only
Timecard's collections. It names drivers through Driver Match.

- **Switch:** `timecard`, off for every DSP until the platform owner switches it on. It needs
  timecards (Paycom), and its Meal Breaks tab alone needs meal breaks (Cortex); each switches
  on with what needs it. Its three tabs are parts of its own: Timecard, Meal Breaks and Employee
  Search.
- **Drivers and employees:** the Meal Breaks tab pairs each driver with their employee through
  core's directory, which Driver Match answers; it matches no names itself. A driver Driver
  Match hasn't reached yet, as while a collection runs, waits unmatched, as does every driver
  in a build without it. Employee Search names each employee's person the same way, and shows
  their photo from core's `/api/dsp/people/{code}/photo`; without one, their initials.
- **Permissions:** `timecard.view`, to read it; `timecard.manage`, which includes it, for its
  settings and schedules; and `collections.run`, to sync now or cancel a sync. The demo DSPs
  give Managers and Members View, and Managers Run Collections.
- **API:** behind `timecard.view`, `GET /api/dsp/timecards` (a day's), `…/employees`,
  `…/employees/{code}` (Employee Search's), `GET /api/dsp/paycom/meal-breaks`, its
  `…/period` (a person's gaps over their timecard period) and `…/period-gaps` (everyone's
  count of them), and `GET /api/dsp/cortex/meal-breaks` (the Meal Breaks tab's),
  `…/paycom/status`, `…/paycom/settings` (what the page shows by, of its settings) and
  `GET /api/dsp/jobs/meal-breaks`. Behind `collections.run`:
  `GET` and `POST /api/dsp/jobs` (its jobs, and a Paycom collection), `…/jobs/{id}/cancel`,
  `POST …/jobs/meal-breaks` (a day's meal breaks from both sources), `…/employees/{code}/sync`
  and `…/cortex/meal-breaks/collect`. Behind `timecard.manage`, its schedules under
  `/api/dsp/schedules`. A tab switched off answers its routes 404.
- **Settings:** its own page of Settings (`?tab=timecard`), which the page's Settings button
  opens and old links to `#dsp/<id>/paycom-settings` land on, holds its sections, each behind
  `timecard.manage` and saved through core's settings: `timecard.schedules`, Sync schedules,
  drawn by its own component, as one schedule may run both collections, which core's
  schedule editor doesn't offer, though its sheet asks Save or Discard for unsaved edits as
  core's does (`useUnsavedSchedule`) before the page moves on; `timecard.roster`, Employees:
  the department and delivery station the Daily tab and Employee Search list (none for all)
  and the name order Meal Breaks writes and sorts names in; `timecard.drivers`, Driver
  departments: all, or those chosen, whom alone the Daily tab lists and Driver Match counts as
  drivers; and `timecard.late_das`,
  Late DAs, with the Meal Breaks tab: the IN DAY time at or after which a DA is late, in the
  departments chosen or any. Their departments and stations are the newest Paycom roster's,
  listed as Settings is read. Its backend reads them with `settings::read`
  (`backend/settings.rs`).
- **Employee workspace:** the directory scrolls independently of the selected detail.
  Changing a person, filter or detail view preserves the displayed period. Meal comparisons
  are flat rows, including additional meals. Regular and overtime minutes come from complete
  on-clock punch segments in the DSP's timezone, excluding every off-clock meal. Each day's
  first 480 minutes are regular and its remainder is overtime; period totals sum those daily
  splits. Missing punches, paid leave without worked punches, and ambiguous or nonexistent
  daylight-saving clocks have unknown worked totals and count as incomplete days.
- **Employee sync:** `GET /api/dsp/jobs/employees` behind `timecard.view` reports the displayed
  `from`/`to`, optionally pinned to `requestId`. `POST` behind `collections.run` accepts those
  bounds and a request ID, independent of the selected employee and directory filters. It
  atomically queues one Paycom job for every known employee plus one Cortex bundle for each
  elapsed DSP-local date through today: at most 15 jobs for a 14-day period. Bundles capture
  every proven tenant station/area/provider identity, preferring period evidence and filling
  missing identities from that tenant's history; initial collection uses tenant discovery.
  Both connections and a valid scope are required before any jobs are queued. Source status
  follows the exact immutable accepted jobs, including failures and cancellations. Retries
  preserve their bounds, dates and scopes. Historical Paycom refreshes preserve the newest
  directory roster; employees retained from history are collected even if absent from the
  active source roster. An inaccessible known employee fails the job rather than becoming
  zero hours; inactive employees never seen by the application cannot be inferred.
- **Schedules:** of Paycom's timecards, Cortex's meal breaks, or `both` at once.
- **Live results:** the page follows collections through core's live collection updates, one
  waiting request for every tab, and refreshes only the days and employees a result changed.
- **Errors** only its screens raise are worded in its API client, which loads with its page;
  its addresses are built in `api/urls.ts`, so its prefetch carries only those up front.
- **Log:** `collection.requested`, `cortex.collection.requested` and
  `meal_breaks.sync_requested`, under Collections. They keep the names they shipped with,
  which history holds, rather than `timecard.…`. A change of its settings is Settings'
  `settings.changed`; `paycom.settings_updated`, which it logged before, keeps its wording for
  the entries history holds.
- **People:** its employees and its meal breaks' drivers are Driver Match's `timecards` and
  `meal_breaks` kinds of data.
- **Storage:** in the collectors' own databases, which it migrates: Paycom's (`paycom`) and
  Cortex's (`cortex`). Its settings are core's, in the DSP's own database; the preferences it
  kept in Paycom's `settings` before (`paycom.preferences`) move into them once, as the DSP
  opens (`settings::carry_over`), into each section never saved, and are read nowhere else.
  A Cortex meal collection asks it for the scope's active publication, so
  finished routes are not read again. A route whose meal punches Cortex sent in a shape the
  meal rules can't read is kept in `meal_unreadable`, and its driver reads as
  `cortex_unreadable` (Check Cortex punches), linked to the route, rather than as no meal.
  Cortex migration `0001_meal_publication_jobs` adds only the mapping from bundled scope
  publications to their actual collection job. Each day validates and publishes all scopes
  in one transaction. Existing meal fields and privacy cleanup remain unchanged. The additive
  table can remain during a code rollback; do not remove it or rewrite shipped migrations.
- **Exposed:** `timecards`, through its contract in `contract/` (`dispatch-timecard-contract`),
  for any feature or agent tool, while it's on for the DSP: the days its collections of
  everyone reached, and a driver's timecards by their Paycom codes, matched exactly, over up
  to 31 days, each day's punches, meal breaks as punched, Paycom's hours and the minutes
  worked, with the employee syncs and running collections its pages read, but without the
  Daily tab's department filters. For the codes asked, a day after the collection covering it,
  or outside every collection, is unknown rather than a day off, and the collection's own day
  is partial: only what was punched by then. It refuses a day that isn't YYYY-MM-DD
  (`invalid_date`), and more than 31 days, more than 16 codes, or a code that's empty or over
  128 characters (`invalid_input`); no codes read no timecards. It leaves out departments,
  positions, Paycom links and the Cortex meal comparison.
