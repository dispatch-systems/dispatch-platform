//! The app's integration tests, one program for all of them: each file is a module named for
//! it, and a module that needs features left out of a build is left out with them. One
//! program links the product once, where a program per file linked it a dozen times.
//! `browseros_host` and `browseros_worker` stay programs of their own, which the collector
//! shards run on a real browser.
mod agent_activity;
mod agent_tools;
mod collection_jobs;
mod contracts;
#[cfg(feature = "timecard")]
mod driver_match;
mod http_requests;
mod http_routes;
#[cfg(feature = "timecard")]
mod meal_cache;
mod network_policy;
mod oauth;
#[cfg(feature = "weekly_scorecard")]
mod weekly_scorecard_transition;

#[cfg(all(feature = "daily_performance", feature = "weekly_scorecard"))]
mod daily_performance;
