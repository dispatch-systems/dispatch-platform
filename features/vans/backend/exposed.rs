//! What Vans exposes to every other feature and agent tool: `amazon_fleet`, the fleet as it
//! keeps it, through its contract (`dispatch_vans_contract`).
use super::keeper::Fleet;
use crate::VansStore;
use dispatch_core::{Result, db::Store, manifest::Keeper};
use dispatch_vans_contract::{AmazonFleet, Van};

/// Vans' fleet, as `AmazonFleet` reads it.
pub(crate) struct Exposed;
pub(crate) static EXPOSED: Exposed = Exposed;

impl AmazonFleet for Exposed {
    fn collected(&self, db: &Store, dsp: &str) -> Result<Option<String>> {
        Fleet.last_collected(db, dsp)
    }
    fn vans(&self, db: &Store, dsp: &str, vins: &[String]) -> Result<Vec<Van>> {
        db.vans(dsp, vins)
    }
}

#[cfg(test)]
#[path = "../tests/backend/exposed.rs"]
mod tests;
