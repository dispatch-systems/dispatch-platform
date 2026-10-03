//! Driver Match: one code per driver across Paycom and every Amazon source.
use crate::{
    db::{Kind, Migration, Migrations, migrations::Apply::Sql},
    manifest::{Feature, Switch, feature, perm},
};

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
    ..feature("driver_match")
};
