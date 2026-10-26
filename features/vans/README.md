# Vans

Every vehicle Amazon's fleet lists for the DSP's company, with the name the DSP gives it,
collected from Cortex's fleet management Vehicles page. A route names its van by VIN alone; Vans
turns that VIN into the DSP's own name for it ("EDV 214"). It has no page of its own: the fleet
is collected, on demand and by its rules, from Settings' Collectors page.

- **Switch:** `vans`, off for every DSP until the platform owner switches it on. It needs the
  `vans` connection (Cortex), which switches on with it.
- **Permission:** `vans.collect`, to see when the fleet was collected, collect it on demand and
  set when it's collected, on the Collectors page (its keeper's `permission` and `schedules`).
- **API:** `POST /api/dsp/vans/jobs/{id}/cancel`, behind `vans.collect`. The fleet is
  collected, and its schedule set, as the `vans` collection on core's Collectors routes; what it
  collects, as the Collectors page shows under it, is `frontend/collected.ts` (the `collected`
  slot).
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
- **Schedules:** only the DSP's own, set on the Collectors page; Vans makes none.
- **Data:** `vans` and `fleet_runs`, in a database of its own beside Cortex's. Each van by
  VIN: the name the DSP gives it, plate, status, kind (`service_tier`), station, and the row as
  Amazon sent it. A name is reused: a returned rental's may be the next one's, so a van is named
  by its VIN and never the other way round. A van the fleet stops listing keeps its row, with
  `listed` 0, so a route that names its VIN still finds its name.
- **Log:** `vans.collection_requested`, under Collections, which its own Collect now wrote
  before the Collectors page collected the fleet; it's worded still, for those entries.
- **Exposed:** `amazon_fleet`, through its contract in `contract/` (`dispatch-vans-contract`),
  for any feature or agent tool, while it's on for the DSP: when the fleet was last collected, or
  none before the first, and for the VINs asked, in their order, the name the DSP gives each van,
  its status, whether the fleet still lists it and when it last did. A VIN is matched in
  capitals, without the spaces around it. It leaves out a VIN it never listed, and refuses an
  empty VIN, one over 128 characters or more than 100 VINs (`MOST_VINS`) with `invalid_input`. It
  never offers the row as Amazon sent it or the company.
