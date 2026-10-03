//! Scorecard: Amazon's weekly scorecard, collected from Cortex.
use crate::manifest::{Feature, feature, perm};

pub const FEATURE: Feature = Feature {
    permissions: &[
        perm("scorecard.view", "View Scorecard", 50),
        perm("scorecard.collect", "Collect Scorecard", 51).implies(&["scorecard.view"]),
        perm("scorecard.manage", "Manage Scorecard", 52).implies(&["scorecard.view"]),
    ],
    keeps: &[&crate::scorecard::keeper::Scorecard],
    ..feature("scorecard")
};
