//! Vans' settings, on its own page: Amazon's fleet as it stands, which its fleet bar draws, and
//! when the fleet is collected.
use dispatch_core::manifest::{Section, schedules, section};

/// When the fleet was collected and is collected next, and Collect now: what its fleet bar
/// shows and does, not a value.
pub const FLEET: Section = section("vans.fleet", "Amazon fleet", "vans.collect", &[])
    .custom()
    .order(10);

/// The schedules of its `vans` collection, which core's schedule routes keep.
pub const COLLECTION: Section = section(
    "vans.collection",
    "Collection",
    "vans.collect",
    &[schedules(
        "schedules",
        "Schedules",
        "/api/dsp/vans/schedules",
        "vans",
        ("Daily vans", "04:30"),
    )],
)
.order(20);
