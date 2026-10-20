//! Vans: every vehicle Amazon's fleet management lists for the DSP's company, with the name
//! the DSP gives it, its VIN and plate, as the Vehicles page reads them.
mod capture;
pub(crate) mod collect;
mod fixture;

pub use capture::{Capture, Collection, JOB_KIND, MAX_VANS, Request, Van};
pub use fixture::fixture;
