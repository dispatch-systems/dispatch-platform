//! Daily Quality and Day Safety, independently collected from the weekly scorecard.
mod capture;
mod collect;
mod datasets;
pub use capture::{Capture, Collection, JOB_KIND, Request, fixture};
pub use datasets::{DATASETS, dataset};
