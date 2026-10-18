//! What Daily Performance exposes to every other feature and agent tool: `daily_metrics`, through its
//! contract (`dispatch_daily_performance_contract`).
use dispatch_core::{Result, db::Store};
use dispatch_daily_performance_contract::DailyMetrics;

/// Daily Performance's Daily Metrics, as its contract says.
pub(crate) struct Exposed;
pub(crate) static EXPOSED: Exposed = Exposed;

impl DailyMetrics for Exposed {
    fn ready(&self, _: &Store, _: &str) -> Result<bool> {
        // TODO(scaffold): answer from what it keeps.
        Ok(true)
    }
}

#[cfg(test)]
#[path = "../tests/backend/exposed.rs"]
mod tests;
