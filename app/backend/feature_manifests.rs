//! The manifests of the features the app still compiles, each its `feature.rs`: the root of
//! its crate once it is cut out of the app, as the other features' are.
#[path = "../../features/dvic/feature.rs"]
pub mod dvic;
#[path = "../../features/routes/feature.rs"]
pub mod routes;
#[path = "../../features/scorecard/feature.rs"]
pub mod scorecard;
#[path = "../../features/timecard/feature.rs"]
pub mod timecard;
