//! Routes: each day's routes, itineraries and packages from Cortex.
use crate::manifest::{Feature, Switch, feature, perm};

pub const FEATURE: Feature = Feature {
    switch: Some(Switch {
        id: "routes",
        label: "Routes",
        requires: &["routes"],
    }),
    permissions: &[
        perm("routes.view", "View Routes", 30),
        perm("routes.collect", "Collect Routes", 31).implies(&["routes.view"]),
        perm("routes.manage", "Manage Routes", 32).implies(&["routes.view"]),
    ],
    keeps: &[&crate::routedata::keeper::Routes],
    ..feature("routes")
};
