//! Shifts: every shift assignment Amazon's Scheduling lists at the DSP's station for the
//! current week, as the week view and its "Shifts & Availability" sheet show them.
mod capture;
pub(crate) mod collect;
mod fixture;

pub use capture::{Assignment, Capture, Collection, JOB_KIND, MAX_ASSIGNMENTS, Request, week};
pub use fixture::fixture;
