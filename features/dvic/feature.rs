//! DVIC: vehicle inspections, from the weekly reports Cortex publishes.
use crate::manifest::{Feature, feature, perm};

pub const FEATURE: Feature = Feature {
    permissions: &[
        perm("dvic.view", "View DVIC", 40),
        perm("dvic.collect", "Collect DVIC", 41).implies(&["dvic.view"]),
        perm("dvic.manage", "Manage DVIC", 42).implies(&["dvic.view"]),
    ],
    keeps: &[&crate::dvic::keeper::Dvic],
    ..feature("dvic")
};
