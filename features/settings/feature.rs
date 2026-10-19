//! Settings: the page every owner's sections of Settings are drawn on, and the DSP's profile,
//! its own section there.
mod api;
mod backend;

use dispatch_core::manifest::{Feature, feature, mandatory, perm};

pub const FEATURE: Feature = Feature {
    place: 110,
    switch: mandatory("settings", "Settings"),
    permissions: &[perm("settings.manage", "Manage DSP Settings", 90).group("DSP Settings")],
    routes: api::routes::routes,
    // The DSP's profile, on the DSP's page of Settings.
    settings: &[backend::PROFILE],
    ..feature("settings")
};
