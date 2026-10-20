# Vans

Every vehicle Amazon's fleet lists for the DSP's company, with the name the DSP gives it,
collected from Cortex's fleet management Vehicles page. A route names its van by VIN alone; Vans
turns that VIN into the DSP's own name for it ("214"). It has no page of its own: its fleet bar
and its schedules are on its Settings page.

- **Switch:** `vans`, off for every DSP until the platform owner switches it on. It needs the
  `vans` connection (Cortex), which switches on with it.
- **Permission:** `vans.collect`, to see when the fleet was collected, collect it on demand and
  change its schedules.
- **API**, all behind `vans.collect`: `GET /api/dsp/vans/status` (when it was collected, how
  many vans it lists, the next scheduled run and the recent jobs), `POST …/collect`,
  `POST …/jobs/{id}/cancel`, and core's schedule routes under `…/schedules` for the `vans`
  collection.
- **Collection:** Cortex's `vans` reads the company whose short code is the DSP's abbreviation,
  then its whole fleet. A fleet listing no van is refused rather than taken to mean every van
  left.
- **Schedule:** each DSP that has it gets one schedule, `Daily vans` at 04:30 its time, as soon
  as Cortex is connected and the DSP has an abbreviation, apart from the roster's 05:00 since a
  DSP's collections run one at a time. It is made once: changed or removed, it stays as the DSP
  left it.
- **Data:** `vans`, `fleet_runs` and `settings`, in a database of its own beside Cortex's. A van
  the fleet stops listing keeps its row, with `listed` 0, so a route that names its VIN still
  finds its name.
- **Settings:** its own page (`?tab=vans`), for those who hold `vans.collect`
  (`backend/settings.rs`): `vans.fleet` "Amazon fleet", a custom section its fleet bar draws
  (when the fleet was collected and is collected next, and Collect now), then `vans.collection`
  "Collection", the `vans` schedules, kept by core's schedule routes under `…/schedules`. It
  stores no values of its own there.
- **Log:** `vans.collection_requested`, under Collections.
- **Exposed:** `amazon_fleet`, through its contract in `contract/` (`dispatch-vans-contract`),
  for any feature or agent tool, while it's on for the DSP: when the fleet was last collected, or
  none before the first, and for the VINs asked, in their order, the name the DSP gives each van,
  whether the fleet still lists it and when it last did. A VIN is matched in capitals, without
  the spaces around it. It leaves out a VIN it never listed, and refuses an empty VIN, one over
  128 characters or more than 100 VINs with `invalid_input`. It never offers the row as Amazon
  sent it or the company.
