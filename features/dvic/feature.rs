//! DVIC: vehicle inspections, from the weekly reports Cortex publishes.
use crate::manifest::{Feature, Switch, feature, perm, tab};

pub const FEATURE: Feature = Feature {
    switch: Some(Switch {
        id: "dvic",
        label: "DVIC",
        requires: &["dvic"],
    }),
    tabs: &[tab("dvic.day", "Day"), tab("dvic.week", "Week")],
    permissions: &[
        perm("dvic.view", "View DVIC", 40),
        perm("dvic.collect", "Collect DVIC", 41).implies(&["dvic.view"]),
        perm("dvic.manage", "Manage DVIC", 42).implies(&["dvic.view"]),
    ],
    keeps: &[&crate::dvic::keeper::Dvic],
    ..feature("dvic")
};
