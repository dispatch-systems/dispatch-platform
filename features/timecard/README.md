# Timecard

Paycom's employees, punches and timecards and the meal breaks Cortex reports, on the Timecard
page (Daily, Meal Breaks and Employee Search) and its settings. Rust interprets punches and
assesses meals; the frontend only formats the results. Its addresses keep their old names
(`#dsp/<id>/paycom`, `/api/dsp/paycom/…`), and its jobs and schedules keep theirs from when
they were the only ones (`/api/dsp/jobs`, `/api/dsp/schedules`), though they serve only
Timecard's collections. It names drivers through Driver Match.

- **Switch:** `timecard`, off for every DSP until the platform owner switches it on. It needs
  timecards (Paycom), and its Meal Breaks tab alone needs meal breaks (Cortex); each switches
  on with what needs it. Its three tabs are parts of its own: Timecard, Meal Breaks and Employee
  Search. It depends on Driver Match, which a build leaving Driver Match out leaves it out with.
- **Permissions:** `timecard.view`, to read it; `timecard.manage`, which includes it, for its
  settings and schedules; and `collections.run`, to sync now or cancel a sync. The demo DSPs
  give Managers and Members View, and Managers Run Collections.
- **API:** behind `timecard.view`, `GET /api/dsp/timecards` (a day's), `…/employees` and
  `…/employees/{code}` (Employee Search's), `GET /api/dsp/paycom/meal-breaks` and
  `GET /api/dsp/cortex/meal-breaks` (the Meal Breaks tab's), `…/paycom/status`,
  `…/paycom/settings` and `GET /api/dsp/jobs/meal-breaks`. Behind `collections.run`:
  `GET` and `POST /api/dsp/jobs` (its jobs, and a Paycom collection), `…/jobs/{id}/cancel`,
  `POST …/jobs/meal-breaks` (a day's meal breaks from both sources), `…/employees/{code}/sync`
  and `…/cortex/meal-breaks/collect`. Behind `timecard.manage`, `POST …/paycom/settings` and
  its schedules under `/api/dsp/schedules`. A tab switched off answers its routes 404.
- **Schedules:** of Paycom's timecards, Cortex's meal breaks, or `both` at once.
- **Live results:** the page follows collections through core's live collection updates, one
  waiting request for every tab, and refreshes only the days and employees a result changed.
- **Errors** only its screens raise are worded in its API client, which loads with its page;
  its addresses are built in `api/urls.ts`, so its prefetch carries only those up front.
- **Log:** `collection.requested`, `cortex.collection.requested` and
  `meal_breaks.sync_requested`, under Collections, and `paycom.settings_updated`, under
  Settings. They keep the names they shipped with, which history holds, rather than
  `timecard.…`.
- **Agents:** `timecards` and `meal-breaks` under `/api/v1/`, read with the `timecards` and
  `meal_breaks` toggles. Its employees and its meal breaks' drivers are Driver Match's
  `timecards` and `meal_breaks` kinds of data.
- **Storage:** in the collectors' own databases, which it migrates: Paycom's (`paycom`) and
  Cortex's (`cortex`). A Cortex meal collection asks it for the scope's active publication, so
  finished routes are not read again. A route whose meal punches Cortex sent in a shape the
  meal rules can't read is kept in `meal_unreadable`, and its driver reads as
  `cortex_unreadable` (Check Cortex punches), linked to the route, rather than as no meal.
