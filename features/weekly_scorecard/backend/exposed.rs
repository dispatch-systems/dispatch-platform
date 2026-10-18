//! What Weekly Scorecard exposes to every other feature and agent tool: `driver_scorecards`, through its
//! contract (`dispatch_weekly_scorecard_contract`).
use dispatch_core::{Result, db::Store};
use dispatch_weekly_scorecard_contract::DriverScorecards;

/// Weekly Scorecard's Driver Scorecards, as its contract says.
pub(crate) struct Exposed;
pub(crate) static EXPOSED: Exposed = Exposed;

impl DriverScorecards for Exposed {
    fn ready(&self, _: &Store, _: &str) -> Result<bool> {
        // TODO(scaffold): answer from what it keeps.
        Ok(true)
    }
}

#[cfg(test)]
#[path = "../tests/backend/exposed.rs"]
mod tests;
