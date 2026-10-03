//! Timecard: Paycom's punches and timecards, and the meal breaks Cortex reports.
use crate::{
    collectors::{cortex, paycom},
    db::{Migration, Migrations, migrations::Apply::Sql},
    manifest::{
        DefaultRole::{Manager, Member},
        Feature, Switch, feature, perm, tab,
    },
};

pub const FEATURE: Feature = Feature {
    switch: Some(Switch {
        id: "timecard",
        label: "Timecard",
        requires: &["timecards", "meal_breaks"],
    }),
    tabs: &[
        tab("timecard.daily", "Timecard"),
        tab("timecard.meal_breaks", "Meal Breaks"),
        tab("timecard.employees", "Employee Search"),
    ],
    schedules: true,
    permissions: &[
        perm("timecard.view", "View Timecard", 20).defaults(&[Manager, Member]),
        perm("timecard.manage", "Manage Timecard", 21).implies(&["timecard.view"]),
        perm("collections.run", "Run Collections", 22).defaults(&[Manager]),
    ],
    // Collection progress refreshes both the timecard pages and the collections page.
    live: &["timecard.view", "collections.run"],
    keeps: &[
        &crate::workforce::keeper::Timecards,
        &crate::meals::keeper::MealBreaks,
    ],
    // Its tables live in the collectors' databases, beside what each collection reads.
    migrations: &[
        Migrations {
            kind: paycom::DATABASE,
            list: &[
                Migration {
                    id: 2,
                    name: "employee_timecard_syncs",
                    apply: Sql(include_str!(
                        "migrations/paycom/0002_employee_timecard_syncs.sql"
                    )),
                },
                Migration {
                    id: 3,
                    name: "employee_history_index",
                    apply: Sql(include_str!(
                        "migrations/paycom/0003_employee_history_index.sql"
                    )),
                },
            ],
        },
        Migrations {
            kind: cortex::DATABASE,
            list: &[Migration {
                id: 2,
                name: "meal_stops",
                apply: Sql(include_str!("migrations/cortex/0002_meal_stops.sql")),
            }],
        },
    ],
    ..feature("timecard")
};
