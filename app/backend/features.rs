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
