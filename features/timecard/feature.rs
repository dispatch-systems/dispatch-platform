//! Timecard: Paycom's punches and timecards, and the meal breaks Cortex reports.
use crate::manifest::{
    DefaultRole::{Manager, Member},
    Feature, feature, perm,
};

pub const FEATURE: Feature = Feature {
    permissions: &[
        perm("timecard.view", "View Timecard", 20).defaults(&[Manager, Member]),
        perm("timecard.manage", "Manage Timecard", 21).implies(&["timecard.view"]),
        perm("collections.run", "Run Collections", 22).defaults(&[Manager]),
    ],
    keeps: &[
        &crate::workforce::keeper::Timecards,
        &crate::meals::keeper::MealBreaks,
    ],
    ..feature("timecard")
};
