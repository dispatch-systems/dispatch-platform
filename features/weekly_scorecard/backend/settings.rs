//! Weekly Scorecard's settings, with its collection on the Collectors page: which weeks a run
//! rechecks.
use dispatch_core::manifest::{Section, number, section};
use serde::Deserialize;

/// Which completed weeks a run of its collection rechecks, apart from when its rules run it:
/// the latest `lookback_weeks`, posted ones too, as disputes change them, each unless checked
/// within `refresh_hours`.
pub const COLLECTION: Section = section(
    "weekly_scorecard.collection",
    "Collection",
    "weekly_scorecard.manage",
    &[
        number("lookback_weeks", "Look back", 1, 8, Some(1)).unit("weeks"),
        number("refresh_hours", "Refresh after", 1, 168, Some(20)).unit("hours"),
    ],
)
.collection("weekly_scorecard");

/// `COLLECTION`'s values, as a run reads them.
#[derive(Deserialize)]
pub struct Policy {
    pub lookback_weeks: u32,
    pub refresh_hours: u32,
}
