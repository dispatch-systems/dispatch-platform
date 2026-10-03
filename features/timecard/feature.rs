//! Timecard: Paycom's punches and timecards, and the meal breaks Cortex reports.
use crate::{
    collectors::{cortex, paycom},
    contracts::AuditArea::Collections,
    db::{Migration, Migrations, migrations::Apply::Sql},
    driver_match,
    manifest::{
        Audit,
        DefaultRole::{Manager, Member},
        Feature, Switch, feature, perm, tab,
    },
    meals,
    read_cache::{Cached, DataDomain, Evicted::By, LISTINGS},
    workforce,
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
    keeps: &[&workforce::keeper::Timecards, &meals::keeper::MealBreaks],
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
    domains: &[workforce::DOMAIN, meals::DOMAIN],
    cached: &[
        // The DSP listings' collection dates are its timecards'.
        Cached {
            read: LISTINGS,
            evicted: By(&[workforce::DOMAIN]),
        },
        Cached {
            read: meals::CACHED,
            evicted: By(&[
                workforce::DOMAIN,
                meals::DOMAIN,
                DataDomain::LIVE,
                driver_match::DOMAIN,
                DataDomain::TENANT,
            ]),
        },
    ],
    // Asking Cortex for meal breaks, and syncing them, read as collections.
    audit: Audit {
        areas: &[
            ("cortex.collection.", Collections),
            ("meal_breaks.", Collections),
        ],
        ..Audit::NONE
    },
    demo: Some(workforce::demo),
    ..feature("timecard")
};
