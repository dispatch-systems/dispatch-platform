//! The scorecard: a station's weekly datasets, from the scorecard's own API.
mod capture;
pub(crate) mod collect;

pub use capture::{Capture, Collection, DATASETS, Dataset, JOB_KIND, Request, dataset, fixture};
