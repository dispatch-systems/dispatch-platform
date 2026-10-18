//! What Driver Profiles exposes to every other feature and agent tool: `amazon_roster`, through its
//! contract (`dispatch_driver_profiles_contract`).
use dispatch_core::{Result, db::Store};
use dispatch_driver_profiles_contract::AmazonRoster;

/// Driver Profiles's Amazon Roster, as its contract says.
pub(crate) struct Exposed;
pub(crate) static EXPOSED: Exposed = Exposed;

impl AmazonRoster for Exposed {
    fn ready(&self, _: &Store, _: &str) -> Result<bool> {
        // TODO(scaffold): answer from what it keeps.
        Ok(true)
    }
}

#[cfg(test)]
#[path = "../tests/backend/exposed.rs"]
mod tests;
