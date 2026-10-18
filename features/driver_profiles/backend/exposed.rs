//! What Driver Profiles exposes to every other feature and agent tool: `amazon_roster`, the
//! roster as it keeps it, through its contract (`dispatch_driver_profiles_contract`).
use super::keeper::Roster;
use crate::DriverProfilesStore;
use dispatch_core::{
    Result,
    db::Store,
    manifest::{Keeper, people::Source},
};
use dispatch_driver_profiles_contract::{AmazonRoster, Associate};

/// Driver Profiles' roster, as `AmazonRoster` reads it.
pub(crate) struct Exposed;
pub(crate) static EXPOSED: Exposed = Exposed;

impl AmazonRoster for Exposed {
    fn source(&self) -> Source {
        dispatch_cortex::SOURCE
    }
    fn collected(&self, db: &Store, dsp: &str) -> Result<Option<String>> {
        Roster.last_collected(db, dsp)
    }
    fn associates(&self, db: &Store, dsp: &str, ids: &[String]) -> Result<Vec<Associate>> {
        db.associates(dsp, ids)
    }
}

#[cfg(test)]
#[path = "../tests/backend/exposed.rs"]
mod tests;
