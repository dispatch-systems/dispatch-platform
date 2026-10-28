//! The app's integration tests, one program for all of them: each file is a module named for
//! it, and a module that needs features left out of a build is left out with them. One
//! program links the product once, where a program per file linked it a dozen times.
//! `browseros_host` and `browseros_worker` stay programs of their own, which the collector
//! shards run on a real browser. `support` is what they share: the real router served
//! in-process, sessions and requests.
mod support;

mod agent_activity;
#[cfg(all(feature = "dvic", feature = "routes"))]
mod agent_tool_checks;
mod agent_tools;
mod collection_jobs;
mod contracts;
#[cfg(all(
    feature = "timecard",
    feature = "driver_match",
    feature = "driver_profiles"
))]
mod driver_match;
mod http_requests;
mod http_routes;
#[cfg(feature = "timecard")]
mod meal_cache;
#[cfg(all(feature = "timecard", feature = "driver_match"))]
mod meal_driver_match;
mod network_policy;
mod oauth;
#[cfg(all(feature = "routes", feature = "communications"))]
mod todays_routes;
#[cfg(feature = "weekly_scorecard")]
mod weekly_scorecard_transition;

#[cfg(all(feature = "daily_performance", feature = "weekly_scorecard"))]
mod daily_performance;

/// One Driver Match pass over a DSP's data, as after a collection: reading, then writing.
#[cfg(feature = "driver_match")]
fn pass(db: &dispatch_core::db::Store, dsp: &str) -> dispatch_core::Result<usize> {
    use dispatch_driver_match::DriverMatchStore;
    db.write_pass(dsp, db.read_pass(dsp)?)
}
