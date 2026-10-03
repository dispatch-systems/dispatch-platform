//! What Timecard keeps: Paycom's punches and timecards, and the meal breaks Cortex reports,
//! read and written through one extension of the store.
pub(crate) mod meals;
pub(crate) mod punches;
mod storage;

pub use storage::TimecardStore;
