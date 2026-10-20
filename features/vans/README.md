# Vans

Every vehicle Amazon's fleet lists for the DSP's company, with the name the DSP gives it,
collected from Cortex's fleet management Vehicles page. A route names its van by VIN alone; Vans
turns that VIN into the DSP's own name for it ("EDV 214"). It has no page of its own: its fleet
bar and its schedules are on its Settings page.

- **Switch:** `vans`, off for every DSP until the platform owner switches it on. It needs the
  `vans` connection (Cortex), which switches on with it.
- **Permission:** `vans.collect`, to see when the fleet was collected, collect it on demand and
  change its schedules.
- **API**, all behind `vans.collect`: `GET /api/dsp/vans/status` (when it was collected, how
  many vans the DSP runs and how many are inactive, the next scheduled run and the recent
  jobs), `POST …/collect`, `POST …/jobs/{id}/cancel`, and core's schedule routes under
  `…/schedules` for the `vans` collection.
- **Collection:** Cortex's `vans` reads the fleet as Amazon's fleet management page lists it,
  `/fleet-management/api/vehicles`, over plain HTTP with the session the job's browser started
  from, which closes once it is copied: every status the list accepts (active, maintenance,
  pending and inactive, those the DSP returned, sold or retired, which older routes still name),
  page after page while it answers a continuation key. The list answers for the signed-in
  account and takes no company to scope it, so it is read only for an account that sees one
  company, the one whose short code is the DSP's abbreviation. A fleet listing no van is
  refused rather than taken to mean every van left.
- **Measured** on a copy of a live DSP's session (318 vans, 302 named; medians of three):
  this way took 2.3 s, 2.0 CPU-seconds and 199 MB, with the browser open 1.0 s and two
  requests. The fleet page's own request took 4.2 s and 5.7 CPU-seconds (3.7 s headless), and
  a `fetch` inside a tab 3.0 s and 3.0. Asking for inactive vans costs nothing: the list takes
  1.3 s for 64 vans or 318. The list ignores page sizes and company IDs, refuses
  `OUT_OF_SERVICE`, sends no ETag, and its Excel export is built in the page from this same
  list. All 52 VINs that Routes held for that DSP were in it and named, 8 of them inactive.
- **Schedules:** only the DSP's own, added on its Settings page; Vans makes none.
- **Data:** `vans` and `fleet_runs`, in a database of its own beside Cortex's. Each
  van by VIN: the name the DSP gives it, plate, status, kind (`service_tier`), station, and the
  row as Amazon sent it. A name is reused: a returned rental's may be the next one's, so a van is
  named by its VIN and never the other way round. A van the fleet stops listing keeps its row,
  with `listed` 0, so a route that names its VIN still finds its name.
- **Settings:** its own page (`?tab=vans`), for those who hold `vans.collect`
  (`backend/settings.rs`): `vans.fleet` "Amazon fleet", a custom section its fleet bar draws
  (when the fleet was collected and is collected next, and Collect now), then `vans.collection`
  "Collection", the `vans` schedules, kept by core's schedule routes under `…/schedules`. It
  stores no values of its own there.
- **Log:** `vans.collection_requested`, under Collections.
- **Exposed:** `amazon_fleet`, through its contract in `contract/` (`dispatch-vans-contract`),
  for any feature or agent tool, while it's on for the DSP: when the fleet was last collected, or
  none before the first, and for the VINs asked, in their order, the name the DSP gives each van,
  its status, whether the fleet still lists it and when it last did. A VIN is matched in
  capitals, without the spaces around it. It leaves out a VIN it never listed, and refuses an
  empty VIN, one over 128 characters or more than 100 VINs with `invalid_input`. It never offers
  the row as Amazon sent it or the company.
