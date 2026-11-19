//! Legacy Tracker: a link on Conduct's tab strip to the tracker one DSP kept its writeups in
//! before, at the address the DSP sets. Its README says more.
mod api;
mod backend;

/// What its API answers with, which the app writes to TypeScript.
pub use api::types::DspCjf87gqdLegacyTrackerSummary;

use dispatch_core::manifest::{Feature, custom, feature};

pub const FEATURE: Feature = Feature {
    place: 170,
    switch: custom("cjf87gqd_legacy_tracker", "Legacy Tracker", &[]),
    // Its link is on Conduct's page, for those who use Conduct.
    depends_on: &["conduct"],
    routes: api::routes::routes,
    // The tracker's address, on Conduct's page of Settings, for every member.
    settings: &[backend::settings::GENERAL],
    ..feature("cjf87gqd_legacy_tracker")
};

/// Its API types, which the app's export writes to TypeScript in its `api/generated/`.
#[cfg(feature = "ts")]
pub fn typescript(cfg: &ts_rs::Config) -> dispatch_core::Typescript {
    dispatch_core::typescript!(cfg, DspCjf87gqdLegacyTrackerSummary)
}
