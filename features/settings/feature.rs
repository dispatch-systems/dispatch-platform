//! Settings: the profile, security and theme panels, and the features' settings tabs.
use crate::manifest::{Feature, feature, perm};

pub const FEATURE: Feature = Feature {
    permissions: &[perm("settings.manage", "Manage DSP Settings", 90).group("DSP")],
    ..feature("settings")
};
