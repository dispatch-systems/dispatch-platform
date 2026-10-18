//! What Timecard exposes to every other feature and agent tool: `timecards`, through its
//! contract (`dispatch_timecard_contract`).
use dispatch_core::{Result, db::Store};
use dispatch_timecard_contract::Timecards;

/// Timecard's Timecards, as its contract says.
pub(crate) struct Exposed;
pub(crate) static EXPOSED: Exposed = Exposed;

impl Timecards for Exposed {
    fn ready(&self, _: &Store, _: &str) -> Result<bool> {
        // TODO(scaffold): answer from what it keeps.
        Ok(true)
    }
}

#[cfg(test)]
#[path = "../tests/backend/exposed.rs"]
mod tests;
