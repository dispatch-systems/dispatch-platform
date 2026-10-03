//! Driver Match: one code per driver across Paycom and every Amazon source.
use crate::{
    db::{Kind, Migration, Migrations, migrations::Apply::Sql},
    driver_match,
    manifest::{Audit, Feature, Switch, feature, perm},
};

#[path = "mcp/mod.rs"]
pub mod mcp;

pub const FEATURE: Feature = Feature {
    // A tab of Settings, not a page of its own: it matches Paycom's employees to the
    // drivers Amazon's routes and other collections name.
    switch: Some(Switch {
        id: "driver_match",
        label: "Driver Match",
        requires: &["timecards", "routes"],
    }),
    permissions: &[perm("driver_match.manage", "Manage Driver Match", 60)],
    migrations: &[
        Migrations {
            kind: Kind::PLATFORM,
            list: &[Migration {
                id: 10,
                name: "driver_codes",
                apply: Sql(include_str!("migrations/platform/0010_driver_codes.sql")),
            }],
        },
        Migrations {
            kind: Kind::DSP,
            list: &[Migration {
                id: 7,
                name: "driver_match",
                apply: Sql(include_str!("migrations/dsp/0007_driver_match.sql")),
            }],
        },
    ],
    domains: &[driver_match::DOMAIN],
    maintenance: &[driver_match::maintenance::MAINTENANCE],
    after_collection: Some(driver_match::maintenance::after_collection),
    // Its events name drivers by their codes, and the log puts today's names to them.
    audit: Audit {
        subjects: &["driver"],
        names: Some(driver_match::name_driver_events),
        ..Audit::NONE
    },
    mcp: mcp::MCP,
    ..feature("driver_match")
};
