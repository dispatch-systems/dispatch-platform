//! Settings: the profile, security and theme panels, and the features' settings tabs.
mod api;

use dispatch_core::manifest::{Feature, feature, mandatory, perm};

pub const FEATURE: Feature = Feature {
    place: 110,
    switch: mandatory("settings", "Settings"),
    permissions: &[perm("settings.manage", "Manage DSP Settings", 90).group("DSP Settings")],
    routes: api::routes::routes,
    ..feature("settings")
};
