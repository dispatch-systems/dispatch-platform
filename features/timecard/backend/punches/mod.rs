//! Workforce preferences, employees, timecards and collection publication.
#[path = "assessment.rs"]
pub mod assessment;
#[path = "daily.rs"]
mod daily;
#[path = "employees.rs"]
mod employees;
#[path = "keeper.rs"]
pub mod keeper;
#[path = "people.rs"]
pub mod people;
#[path = "preferences.rs"]
mod preferences;
#[path = "publication.rs"]
mod publication;
#[path = "queue.rs"]
mod queue;
#[path = "range.rs"]
mod range;
#[path = "../storage.rs"]
mod storage;
#[path = "sync.rs"]
pub(crate) mod sync;
#[path = "timecards.rs"]
pub(crate) mod timecards;

/// A new DSP's demo timecards: Paycom's fixture, published as a collection would be.
pub fn demo(
    store: &dispatch_core::db::Store,
    dsp: &str,
    timezone: &str,
) -> dispatch_core::Result<()> {
    store.publish_timecards(dsp, &dispatch_paycom::fixtures::fixture(timezone)?)?;
    Ok(())
}
/// Paycom's employees, timecards and preferences, as Timecard keeps them.
pub const DOMAIN: dispatch_core::server::cache::DataDomain =
    dispatch_core::server::cache::DataDomain::new("paycom");

pub(crate) use daily::cards;
pub use preferences::defaults;
pub(crate) use range::{DailySource, daily_sources};
pub use storage::TimecardStore;
