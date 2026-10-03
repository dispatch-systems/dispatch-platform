//! Driver Match: one code per driver across Paycom and every Amazon source.
use crate::manifest::{Feature, Switch, feature, perm};

pub const FEATURE: Feature = Feature {
    // A tab of Settings, not a page of its own: it matches Paycom's employees to the
    // drivers Amazon's routes and other collections name.
    switch: Some(Switch {
        id: "driver_match",
        label: "Driver Match",
        requires: &["timecards", "routes"],
    }),
    permissions: &[perm("driver_match.manage", "Manage Driver Match", 60)],
    ..feature("driver_match")
};
