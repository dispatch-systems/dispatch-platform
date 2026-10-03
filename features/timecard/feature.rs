//! Timecard: Paycom's punches and timecards, and the meal breaks Cortex reports.
use crate::manifest::{Feature, feature};

pub const FEATURE: Feature = Feature {
    keeps: &[
        &crate::workforce::keeper::Timecards,
        &crate::meals::keeper::MealBreaks,
    ],
    ..feature("timecard")
};
