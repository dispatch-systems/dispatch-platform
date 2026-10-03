//! Workforce preferences, employees, timecards and collection publication.
#[path = "assessment.rs"]
pub mod assessment;
#[path = "daily.rs"]
mod daily;
#[path = "employees.rs"]
mod employees;
#[path = "fixtures.rs"]
mod fixtures;
#[path = "preferences.rs"]
mod preferences;
#[path = "publication.rs"]
mod publication;
#[path = "range.rs"]
mod range;
#[path = "sync.rs"]
pub(crate) mod sync;
#[path = "timecards.rs"]
pub(crate) mod timecards;
#[path = "validation.rs"]
mod validation;

pub(crate) use daily::cards;
pub use fixtures::{fixture, fixture_date};
pub use preferences::defaults;
pub(crate) use range::DailySource;
pub use validation::{collection_date, validate_workforce};
