//! Uniform Inventory: a DSP's uniform catalog and its stock counts.
use crate::{
    db::{Kind, Migration, Migrations, migrations::Apply::Sql},
    manifest::{
        DefaultRole::{Manager, Member},
        Feature, Switch, feature, perm,
    },
};

pub const FEATURE: Feature = Feature {
    switch: Some(Switch {
        id: "uniforms",
        label: "Uniform Inventory",
        requires: &[],
    }),
    permissions: &[
        perm("uniforms.view", "View Uniform Inventory", 10).defaults(&[Manager, Member]),
        perm("uniforms.adjust", "Adjust Uniform Inventory", 11)
            .implies(&["uniforms.view"])
            .defaults(&[Manager]),
        perm("uniforms.manage", "Manage Uniform Inventory", 12).implies(&["uniforms.view"]),
    ],
    migrations: &[Migrations {
        kind: Kind::DSP,
        list: &[Migration {
            id: 2,
            name: "uniform_inventory",
            apply: Sql(include_str!("migrations/dsp/0002_uniform_inventory.sql")),
        }],
    }],
    ..feature("uniforms")
};
