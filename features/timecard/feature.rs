//! Timecard: Paycom's punches and timecards, and the meal breaks Cortex reports.
mod api;
mod backend;
mod mcp;

/// What its API answers with, which the app writes to TypeScript.
pub use api::{
    assessment::{
        AssessedClock, DeliveryGap, DeliveryGaps, LateRule, Lunch, MealAssessment, MealPair,
        MealStatus, PaycomDay, PunchEvent,
    },
    meals::{
        CortexMeal, CortexPublication, MatchType, MealComparison, MealDriver, MealEmployee,
        MealPaycom, MealSource,
    },
    settings::{
        DepartmentOption, NameOrder, PaycomColumn, PaycomOptions, PaycomPage, PaycomPreferences,
        PaycomSettings, PaycomSort, PreferenceRevision,
    },
    types::{
        DailyTimecard, DailyTimecards, Employee, EmployeeTimecard, EmployeeTimecardResponse,
        EmployeesResponse, InPunchKind, OutPunchKind, Punch, Timecard,
    },
};
/// What its integration tests and its offline fixture builder use beside: how a meal break
/// is assessed, and the comparison row a Cortex meal becomes.
pub use backend::meals::{assessment::assess_meal, comparison_meal};
/// What the app uses: its storage, the cache domain of the Paycom data it keeps, its cached
/// meal-break reads, and the preferences a DSP starts with.
pub use backend::{
    TimecardStore,
    meals::CACHED,
    punches::{DOMAIN, defaults},
};

/// Core's test support, for this crate's module tests.
#[cfg(test)]
use dispatch_core::testing;

use backend::{meals, punches};
use dispatch_core::{
    db::{Migration, Migrations, migrations::Apply::Sql},
    manifest::{
        Audit,
        DefaultRole::{Manager, Member},
        Feature, ScheduleAlias, Switch, feature, perm, tab,
    },
    server::cache::{Cached, DataDomain, Evicted::By, LISTINGS},
    tenancy::api::audit::AuditArea::Collections,
};
use dispatch_cortex as cortex;
use dispatch_driver_match as driver_match;
use dispatch_paycom as paycom;

pub const FEATURE: Feature = Feature {
    place: 20,
    // Its meal-break comparison joins drivers to employees by Driver Match's codes.
    depends_on: &["driver_match"],
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
    // A schedule of `both` runs its two collections at once.
    schedule_aliases: &[ScheduleAlias {
        schedule: "both",
        runs: &[paycom::timecards::JOB_KIND, cortex::meals::JOB_KIND],
    }],
    permissions: &[
        perm("timecard.view", "View Timecard", 20).demo(&[Manager, Member]),
        perm("timecard.manage", "Manage Timecard", 21).implies(&["timecard.view"]),
        perm("collections.run", "Run Collections", 22).demo(&[Manager]),
    ],
    routes: api::routes::routes,
    // Collection progress refreshes both the timecard pages and the collections page.
    live: &["timecard.view", "collections.run"],
    keeps: &[&punches::keeper::Timecards, &meals::keeper::MealBreaks],
    // Its tables live in the collectors' databases, beside what each collection reads.
    tables: &[
        (
            "paycom",
            &[
                "publications",
                "employees",
                "timecards",
                "timecard_sources",
                "employee_timecard_syncs",
                "settings",
            ],
        ),
        (
            "cortex",
            &[
                "meal_schema",
                "meal_publications",
                "meal_itineraries",
                "meal_delivery_events",
                "meal_breaks",
                "meal_record_schema",
                "meal_records",
                "meal_sources",
                "meal_stops",
            ],
        ),
    ],
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
    domains: &[punches::DOMAIN, meals::DOMAIN],
    cached: &[
        // The DSP listings' collection dates are its timecards'.
        Cached {
            read: LISTINGS,
            evicted: By(&[punches::DOMAIN]),
        },
        Cached {
            read: meals::CACHED,
            evicted: By(&[
                punches::DOMAIN,
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
    demo: Some(punches::demo),
    mcp: mcp::MCP,
    people: &[&punches::people::Employees, &meals::people::MealDrivers],
    ..feature("timecard")
};
