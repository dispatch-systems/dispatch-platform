//! Workforce preferences, employees, timecards and collection publication.
#[path = "assessment.rs"]
pub mod assessment;
#[path = "daily.rs"]
mod daily;
#[path = "employees.rs"]
mod employees;
#[path = "keeper.rs"]
pub mod keeper;
#[path = "preferences.rs"]
mod preferences;
#[path = "publication.rs"]
mod publication;
#[path = "queue.rs"]
mod queue;
#[path = "range.rs"]
mod range;
#[path = "sync.rs"]
pub(crate) mod sync;
#[path = "timecards.rs"]
pub(crate) mod timecards;

pub(crate) use daily::cards;
pub use preferences::defaults;
pub(crate) use range::DailySource;
