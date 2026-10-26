# Live Route Data

A DSP's routes for today, read from Cortex every few minutes on the days and inside the window it
sets, and kept as live route data through Routes, which keeps them and offers them to other features
(`live_route_data`, Routes' `LiveRouteData`). As the window ends, the day is saved into historical
route data if the DSP chose so, and live route data is emptied once it is. A day left unsaved
stays in live route data until the next day's first read empties it. It reads within the server
through core's live readers (`core/collection/backend/readers/`), never as jobs.

- **Switch:** `live_route_data`, off for every DSP until the platform owner switches it on. It needs
  a route source (Cortex), which switches on with it. A development or preview DSP's demo leaves it
  off, as it reads a provider: switch it on to try it, with a station and Cortex connected. In
  fixture mode each read is Cortex's fixture day, its two drivers each on a work phone of their
  own, so Routes settles a hold on each. It depends on Routes, which keeps what it reads, and on
  Cortex, which it reads.
- **Permissions:** `live_route_data.manage`, for its settings. Only owners hold it until a role
  grants it.
- **Settings:** `live_route_data.collection`, "Collection", on its own page of Settings (Settings,
  Features, Live Route Data: `?tab=live-route-data`), behind `live_route_data.manage`
  (`backend/settings.rs`). `days`, the days of the week it reads on, every day by default; `start`
  and `stop`, times where the DSP is, from 08:00 until 21:00 by default; `every`, how often it
  reads: 5, 10, 15, 30 or 60 minutes, 5 by default; and `save_to_history`, a toggle, on by
  default. A save with no day is refused (`live_route_data_days_required`), and one whose stop is
  no later than its start (`live_route_data_window_invalid`), so a window never runs past midnight
  and each read is a read of today. A DSP that saved its settings before `days` and `every` were
  added reads every day, every five minutes. Core keeps the values and records each change as
  `settings.changed`; saving wakes the readers. The section names its reader (`.reader`), so
  Settings shows under its heading how the reader reads for the DSP, as core words it: Last read,
  when and whether it was kept, Next read, and Last error, its code worded where the platform
  words it; nothing while it reads nothing, as with no station.
- **Reading** (`backend/reader.rs`, the live reader `live_route_data.routes`): on each of its days,
  every `every` from the window's start until its stop, while Routes is on, the DSP has a station
  and its Cortex connection is enabled; at none else. Each read asks what Routes' `live_request`
  names, a snapshot of today at the DSP's station:
  - natively, with no browser, when the reader holds one the DSP's browser handed back, and it
    reads the scope today's request names. A read whole and checked is kept.
  - Otherwise, or when that reader finds no key on the page, has its key refused, finds its
    session ended or reads a day that fails the checks, it opens the DSP's Cortex browser
    (`Reading::browser`), reads the day as the routes collection does (`routes::Browser`): natively
    first, then with the application's signatures, then by moves. It keeps the native reader that
    browser hands back for the next read, then closes the browser.
  - Cortex briefly unavailable to the native reader fails the read with no browser, keeping the
    reader. The third such read in a row opens the browser anyway.
  - A reader fresh from a browser that fails its first read leaves the next reads to the browser:
    five reads open it, that one among them, before native is tried again.
  - It never opens a browser someone else has open for the DSP, such as a member's signing in or
    verifying: the read fails (`connection_busy`) and the next tries again.
  - The native reader serves only the connection revision it was taken under
    (`Store::connection_revision`): once the DSP's Cortex credentials are replaced, or the
    connection disabled, it goes unused, and a read taken under another revision is never kept,
    as `keep_live` checks as it writes (`connection_changed`). While the connection is disabled,
    the browser doesn't open (`connection_required`).
  - A read that isn't whole is never kept.
- **A day:** its first whole read empties live route data of the day before (`clear_live`) before
  it is kept (`keep_live`). Each read after is kept in place. Once the window ends, the day is
  saved into historical route data if `save_to_history` is on, and live route data is emptied
  once it is (`save_and_clear_live`), as when historical route data already has a read of the
  day collected later; that runs once, even after the server was down, and saving again saves
  nothing twice. A save Routes refuses for good (`routes_scope_mismatch`,
  `routes_station_required`, `routes_day_not_available`, `routes_capture_invalid`,
  `routes_source_too_large`, `invalid_input`, `routes_off`), as when the DSP's station changed
  since the day was read, ends the day unsaved, logged as `live_route_data.not_saved` with its day
  and code, so the next day reads; any other failure, as a busy database, fails the stop, which is
  asked again a minute later. A day unsaved, refused or with `save_to_history` off, stays in live
  route data until the next day starts. A stopped day reads no more, even with its Stop moved
  later: the next day starts afresh. A restart inside the window goes on with the day as it was,
  emptying nothing.
- **Records:** each read in the jobs database's `live_reads` (core's), with counts and codes only.
  A read the browsers' memory turned away is tried once more within its slot, which keeps the
  retry's record (core's).
  A kept read counts the itineraries listed, the rows changed, the work phones held and the holds
  changed (`listed`, `changed`, `holding`, `phoneEvents`). It also counts `browser` when it opened
  the DSP's browser, what read the day (`routes_native`, `routes_signed` or `routes_signed_moved`,
  as a job's metrics name them; none in fixture mode), and why native reading fell back
  (`routes_native_no_key`, `_refused`, `_signed_out`, `_unavailable` or `_invalid`), each as 1. A
  failed read keeps its error's code. The days started and stopped are in `live_days`.
- **Data:** none of its own: live route data and historical route data are Routes'.
- **API, upkeep, environment:** none. Beside core's `live.*` events, it logs only
  `live_route_data.not_saved`, a day's code when it can't be saved.
