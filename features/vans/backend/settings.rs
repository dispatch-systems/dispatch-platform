//! Vans' settings, on its own page: Amazon's fleet as it stands, which its fleet bar draws.
use dispatch_core::manifest::{Section, section};

/// When the fleet was collected and is collected next, and Collect now: what its fleet bar
/// shows and does, not a value.
pub const FLEET: Section = section("vans.fleet", "Amazon fleet", "vans.collect", &[])
    .custom()
    .order(10);
