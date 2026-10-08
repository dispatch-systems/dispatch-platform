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
        Feature, feature, optional, perm,
    },
};

/// Core's test support, for this crate's module tests.
#[cfg(test)]
use dispatch_core::testing;

pub const FEATURE: Feature = Feature {
    place: 30,
    switch: optional("uniforms", "Uniform Inventory", &[]),
    permissions: &[
        perm("uniforms.view", "View Uniform Inventory", 10).demo(&[Manager, Member]),
        // Finer parts of viewing, apart from each other: counting stock, and shaping the
        // catalog.
        perm("uniforms.adjust", "Adjust Uniform Inventory", 11)
            .under("uniforms.view")
            .demo(&[Manager]),
        perm("uniforms.manage", "Manage Uniform Inventory", 12).under("uniforms.view"),
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

/// Its API types, which the app's export writes to TypeScript in its `api/generated/`.
#[cfg(feature = "ts")]
pub fn typescript(cfg: &ts_rs::Config) -> dispatch_core::Typescript {
    dispatch_core::typescript!(
        cfg,
        UniformFit,
        UniformEventKind,
        UniformVariant,
        Uniform,
        UniformInventory,
        UniformAdjustment,
        UniformUpdates,
        UniformEvent,
        UniformHistory,
    )
}
