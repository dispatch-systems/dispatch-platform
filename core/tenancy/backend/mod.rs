//! DSPs, roles, the feature switches' catalog, audit recording and deleting a DSP.
#[path = "../api/mod.rs"]
pub mod api;
pub(crate) mod custom;
pub mod audit;
pub mod beta;
pub mod catalog;
pub mod deletion;
pub mod dsps;
pub mod roles;
pub mod switches;
pub mod visibility;
