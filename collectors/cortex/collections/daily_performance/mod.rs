//! Daily Quality and Day Safety, independently collected from the weekly scorecard.
mod capture;
mod collect;
mod datasets;
pub use capture::{Capture, Collection, Days, JOB_KIND, MAX_DATES, Request, fixture};
pub use datasets::{DATASETS, dataset};
