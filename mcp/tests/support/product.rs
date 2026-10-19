//! The product's features the MCP's tools need, with the features those use and the
//! collectors whose collections they keep, written by `npm run contracts:generate`: its tests
//! install them, each while the MCP's Cargo feature of a feature that brings it is on.
use dispatch_core::manifest::{Collector, Feature};

pub const COLLECTORS: &[&dyn Collector] = &[
    #[cfg(feature = "dvic")]
    &dispatch_cortex::COLLECTOR,
];
pub const FEATURES: &[&Feature] = &[
    #[cfg(feature = "driver_match")]
    &dispatch_driver_match::FEATURE,
    #[cfg(feature = "dvic")]
    &dispatch_dvic::FEATURE,
];
