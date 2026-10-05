//! Workforce preferences, employees, timecards and collection publication.
pub(crate) mod assessment;
pub(crate) mod daily;
pub(crate) mod employees;
pub(crate) mod keeper;
pub(crate) mod people;
pub(crate) mod preferences;
pub(crate) mod publication;
pub(crate) mod queue;
mod range;
pub(crate) mod sync;
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

use super::TimecardStore;
pub(crate) use daily::cards;
pub use preferences::defaults;
pub(crate) use range::{DailySource, daily_sources};
