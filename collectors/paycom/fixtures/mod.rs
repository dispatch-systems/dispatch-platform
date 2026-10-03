//! What fixture mode returns instead of reading Paycom: development, demo data and tests
//! run without the site.
mod timecards;

pub use timecards::{fixture, fixture_date};
