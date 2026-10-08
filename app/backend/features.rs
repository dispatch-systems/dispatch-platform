//! Every feature in features/, written by `npm run contracts:generate`: the registry's,
//! each while the app's Cargo feature of its name is on. The registry puts them in their
//! places.
use dispatch_core::manifest::Feature;

pub const FEATURES: &[&Feature] = &[
    #[cfg(feature = "daily_performance")]
    &dispatch_daily_performance::FEATURE,
    #[cfg(feature = "documents")]
    &dispatch_documents::FEATURE,
    #[cfg(feature = "driver_match")]
    &dispatch_driver_match::FEATURE,
    #[cfg(feature = "dvic")]
    &dispatch_dvic::FEATURE,
    #[cfg(feature = "home")]
    &dispatch_home::FEATURE,
    #[cfg(feature = "routes")]
    &dispatch_routes::FEATURE,
    #[cfg(feature = "settings")]
    &dispatch_settings::FEATURE,
    #[cfg(feature = "team")]
    &dispatch_team::FEATURE,
    #[cfg(feature = "timecard")]
    &dispatch_timecard::FEATURE,
    #[cfg(feature = "uniforms")]
    &dispatch_uniforms::FEATURE,
    #[cfg(feature = "weekly_scorecard")]
    &dispatch_weekly_scorecard::FEATURE,
];

/// Every feature's API types in TypeScript, as each lists them, for the app's export test.
#[cfg(test)]
pub fn typescript(cfg: &ts_rs::Config) -> dispatch_core::Typescript {
    #[allow(unused_mut)]
    let mut all = dispatch_core::Typescript::new();
    #[cfg(feature = "daily_performance")]
    all.extend(dispatch_daily_performance::typescript(cfg));
    #[cfg(feature = "documents")]
    all.extend(dispatch_documents::typescript(cfg));
    #[cfg(feature = "driver_match")]
    all.extend(dispatch_driver_match::typescript(cfg));
    #[cfg(feature = "dvic")]
    all.extend(dispatch_dvic::typescript(cfg));
    #[cfg(feature = "routes")]
    all.extend(dispatch_routes::typescript(cfg));
    #[cfg(feature = "timecard")]
    all.extend(dispatch_timecard::typescript(cfg));
    #[cfg(feature = "uniforms")]
    all.extend(dispatch_uniforms::typescript(cfg));
    #[cfg(feature = "weekly_scorecard")]
    all.extend(dispatch_weekly_scorecard::typescript(cfg));
    all
}
