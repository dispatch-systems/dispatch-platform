//! Driver Profiles' settings, on its own page: Amazon's roster as it stands, which its roster
//! bar draws, and when the roster is collected.
use dispatch_core::manifest::{Section, schedules, section};

/// When the roster was collected and is collected next, and Collect now: what its roster bar
/// shows and does, not a value.
pub const ROSTER: Section = section(
    "driver_profiles.roster",
    "Amazon roster",
    "driver_profiles.collect",
    &[],
)
.custom()
.order(10);

/// The schedules of its `associates` collection, which core's schedule routes keep.
pub const COLLECTION: Section = section(
    "driver_profiles.collection",
    "Collection",
    "driver_profiles.collect",
    &[schedules(
        "schedules",
        "Schedules",
        "/api/dsp/driver-profiles/schedules",
        "associates",
        ("Daily roster", "05:00"),
    )],
)
.order(20);
