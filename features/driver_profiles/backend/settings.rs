//! Driver Profiles' settings, on its own page: Amazon's roster as it stands, which its roster
//! bar draws.
use dispatch_core::manifest::{Section, section};

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
