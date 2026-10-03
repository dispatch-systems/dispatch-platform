//! DVIC: vehicle inspections, from the weekly reports Cortex publishes.
use crate::manifest::{Feature, feature};

pub const FEATURE: Feature = Feature {
    keeps: &[&crate::dvic::keeper::Dvic],
    ..feature("dvic")
};
