//! What agents can know of Driver Match: who the people every source names are, by its
//! codes, and what a code is.
mod catalog;
mod synthetic;

use crate::driver_match::DriverMatchStore;
use dispatch_core::{
    Result,
    db::Store,
    mcp::{
        Mcp,
        data::scope::{Identity, Known},
    },
};

pub const MCP: Mcp = Mcp {
    terms: catalog::TERMS,
    identity: Some(identities),
    synthetic: synthetic::SYNTHETIC,
    ..Mcp::NONE
};

/// Everyone a DSP's sources name, each with their code and every ID they hold.
fn identities(db: &Store, dsp: &str) -> Result<Vec<Identity>> {
    Ok(db
        .driver_match(dsp)?
        .drivers
        .into_iter()
        .map(|driver| Identity {
            code: driver.code,
            name: driver.name,
            status: driver.status,
            ids: driver
                .ids
                .into_iter()
                .map(|id| Known {
                    source: id.source,
                    id: id.id,
                    name: id.name,
                })
                .collect(),
        })
        .collect())
}
