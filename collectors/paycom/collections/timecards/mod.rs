//! Timecards: what a collection is asked for, how the site's pages are read into cards and
//! checked, where an unfinished collection resumes from, and how the same cards are read
//! through Paycom's API instead.
pub(crate) mod api;
mod checkpoint;
pub(crate) mod collect;
pub(crate) mod extract;
pub(crate) mod projection;
mod punches;
mod roster;
mod types;
mod validation;

pub use checkpoint::{Checkpoint, PaycomStore};
pub use types::{EmployeeSync, EmployeeTimecardPeriod, JOB_KIND, PERIOD_DAYS, PeriodSync};
pub use validation::{collection_date, sources, validate_workforce};
