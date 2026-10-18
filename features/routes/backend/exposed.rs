//! What Routes exposes to every other feature and agent tool: `driver_routes`, through its
//! contract (`dispatch_routes_contract`).
use dispatch_core::{Result, db::Store};
use dispatch_routes_contract::DriverRoutes;

/// Routes's Driver Routes, as its contract says.
pub(crate) struct Exposed;
pub(crate) static EXPOSED: Exposed = Exposed;

impl DriverRoutes for Exposed {
    fn ready(&self, _: &Store, _: &str) -> Result<bool> {
        // TODO(scaffold): answer from what it keeps.
        Ok(true)
    }
}

#[cfg(test)]
#[path = "../tests/backend/exposed.rs"]
mod tests;
