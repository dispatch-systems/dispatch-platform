//! Timecards: what a collection is asked for, how the site's pages are read into cards and
//! checked, and where an unfinished collection resumes from.
mod checkpoint;
pub(crate) mod collect;
pub(crate) mod extract;
mod types;
mod validation;

pub use checkpoint::{Checkpoint, PaycomStore};
pub use types::{EmployeeSync, EmployeeTimecardPeriod, JOB_KIND, PERIOD_DAYS};
pub use validation::{collection_date, sources, validate_workforce};
