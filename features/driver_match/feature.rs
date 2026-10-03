//! Driver Match: one code per driver across Paycom and every Amazon source.
use crate::manifest::{Feature, feature, perm};

pub const FEATURE: Feature = Feature {
    permissions: &[perm("driver_match.manage", "Manage Driver Match", 60)],
    ..feature("driver_match")
};
