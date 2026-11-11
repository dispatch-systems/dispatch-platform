//! Daily Performance's settings, with its collection on the Collectors page: which dates a
//! run rechecks.
use dispatch_core::manifest::{Section, number, section};
use serde::Deserialize;

/// Which dates a run of its collection rechecks, apart from when its rules run it: the
/// `lookback_days` before today, each unless collected within `refresh_hours`.
pub const COLLECTION: Section = section(
    "daily_performance.collection",
    "Collection",
    "daily_performance.manage",
    &[
        number("lookback_days", "Look back", 1, 30, Some(7)).unit("days"),
        number("refresh_hours", "Refresh after", 1, 168, Some(20)).unit("hours"),
    ],
)
.collection("daily_performance");

/// `COLLECTION`'s values, as a run reads them.
#[derive(Deserialize)]
pub struct Policy {
    pub lookback_days: u32,
    pub refresh_hours: u32,
}
