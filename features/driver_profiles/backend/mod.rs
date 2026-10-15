//! Driver Profiles' logic: the roster Amazon lists for the DSP, the photos of whoever it
//! lists, and Driver Match, which tells the people every source names apart. It is private
//! to the crate: `feature.rs` re-exports what the app or a declared dependant uses.
pub mod driver_match;
pub(crate) mod keeper;
pub(crate) mod maintenance;
pub(crate) mod people;
pub(crate) mod photos;
pub(crate) mod storage;

use dispatch_core::server::cache::DataDomain;

/// The roster and photos it keeps.
pub const DOMAIN: DataDomain = DataDomain::new("driver_profiles");
