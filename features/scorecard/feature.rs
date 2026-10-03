//! Scorecard: Amazon's weekly scorecard, collected from Cortex.
use crate::manifest::{Feature, Switch, feature, perm};

pub const FEATURE: Feature = Feature {
    // No page of its own yet, but its collection, its schedules and its weeks, apart from
    // the Timecard page.
    switch: Some(Switch {
        id: "scorecard",
        label: "Scorecard",
        requires: &["scorecard"],
    }),
    permissions: &[
        perm("scorecard.view", "View Scorecard", 50),
        perm("scorecard.collect", "Collect Scorecard", 51).implies(&["scorecard.view"]),
        perm("scorecard.manage", "Manage Scorecard", 52).implies(&["scorecard.view"]),
    ],
    keeps: &[&crate::scorecard::keeper::Scorecard],
    ..feature("scorecard")
};
