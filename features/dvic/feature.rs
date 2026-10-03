//! DVIC: vehicle inspections, from the weekly reports Cortex publishes.
use crate::{
    contracts::AuditArea::Collections,
    db::{
        Migration, Migrations,
        migrations::Apply::{Code, Sql},
    },
    dvic,
    manifest::{Audit, Feature, Switch, feature, perm, tab},
};

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
    migrations: &[Migrations {
        kind: dvic::DATABASE,
        list: &[
            Migration {
                id: 1,
                name: "baseline",
                apply: Sql(include_str!("migrations/dvic/0001_baseline.sql")),
            },
            Migration {
                id: 2,
                name: "hidden_drivers",
                apply: Sql(include_str!("migrations/dvic/0002_hidden_drivers.sql")),
            },
            Migration {
                id: 3,
                name: "verified_scope",
                apply: Code(dvic::add_verified_scope),
            },
        ],
    }],
    domains: &[dvic::DOMAIN],
    audit: Audit {
        areas: &[("dvic.", Collections)],
        ..Audit::NONE
    },
    ..feature("dvic")
};
