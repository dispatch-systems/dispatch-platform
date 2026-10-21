//! Vans' logic: the fleet Amazon lists for the DSP. It is private to the crate: `feature.rs`
//! re-exports what the app or a declared dependant uses.
pub(crate) mod exposed;
pub(crate) mod keeper;
pub(crate) mod storage;

#[cfg(test)]
#[path = "../tests/backend/support.rs"]
pub(crate) mod support;

use dispatch_core::server::cache::DataDomain;

/// The fleet it keeps.
pub const DOMAIN: DataDomain = DataDomain::new("vans");
