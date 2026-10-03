//! Scorecard: Amazon's weekly scorecard, collected from Cortex.
use crate::manifest::{Feature, feature};

pub const FEATURE: Feature = Feature {
    keeps: &[&crate::scorecard::keeper::Scorecard],
    ..feature("scorecard")
};
