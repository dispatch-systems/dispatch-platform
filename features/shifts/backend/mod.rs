//! Schedules' logic: the shifts Amazon's Scheduling lists for the DSP's drivers, and every
//! change to them. It is private to the crate: `feature.rs` re-exports what the app or a
//! declared dependant uses.
pub(crate) mod keeper;
pub(crate) mod storage;

#[cfg(test)]
#[path = "../tests/backend/support.rs"]
pub(crate) mod support;

use dispatch_core::server::cache::DataDomain;

/// The shifts it keeps.
pub const DOMAIN: DataDomain = DataDomain::new("shifts");
