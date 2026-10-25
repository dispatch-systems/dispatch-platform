# Live Route Data

TODO(scaffold): what Live Route Data is for a DSP, and the data it shows, in a few lines. Write down any rule that
isn't obvious from the code.

- **Switch:** `live_route_data`, off for every DSP until the platform owner switches it on.
- **Permissions:** `live_route_data.view`, `live_route_data.manage`. Only owners hold them until a role grants them.
- **Settings:** `live_route_data.general`, "General", on its own page of Settings (`?tab=live-route-data`), behind `live_route_data.manage` (`backend/settings.rs`). Its one field, `example`, is a toggle, off until the DSP turns it on. Live Route Data reads it with `backend::settings::general`; core keeps it in the DSP's settings and records each change as `settings.changed`.
