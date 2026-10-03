//! Uniform Inventory: a DSP's uniform catalog and its stock counts.
use crate::manifest::{
    DefaultRole::{Manager, Member},
    Feature, feature, perm,
};

pub const FEATURE: Feature = Feature {
    permissions: &[
        perm("uniforms.view", "View Uniform Inventory", 10).defaults(&[Manager, Member]),
        perm("uniforms.adjust", "Adjust Uniform Inventory", 11)
            .implies(&["uniforms.view"])
            .defaults(&[Manager]),
        perm("uniforms.manage", "Manage Uniform Inventory", 12).implies(&["uniforms.view"]),
    ],
    ..feature("uniforms")
};
