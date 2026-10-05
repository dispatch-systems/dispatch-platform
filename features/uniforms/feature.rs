//! Uniform Inventory: a DSP's uniform catalog and its stock counts.
mod api;
mod backend;

/// What its API answers with, which the app writes to TypeScript.
pub use api::types::{
    Uniform, UniformAdjustment, UniformEvent, UniformEventKind, UniformFit, UniformHistory,
    UniformInventory, UniformUpdates, UniformVariant,
};

use dispatch_core::{
    db::{Kind, Migration, Migrations, migrations::Apply::Sql},
    manifest::{
        DefaultRole::{Manager, Member},
        Feature, Switch, feature, perm,
    },
};

/// Core's test support, for this crate's module tests.
#[cfg(test)]
use dispatch_core::testing;

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
    routes: api::routes::routes,
    tables: &[(
        "dsp",
        &[
            "uniform_inventory",
            "uniforms",
            "uniform_variants",
            "uniform_events",
        ],
    )],
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
