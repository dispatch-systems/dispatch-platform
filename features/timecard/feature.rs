//! Timecard: Paycom's punches and timecards, and the meal breaks Cortex reports.
mod api;
mod backend;

/// What its API answers with, which the app writes to TypeScript.
pub use api::{
    assessment::{
        AssessedClock, DeliveryGap, DeliveryGaps, LateRule, Lunch, MealAssessment, MealPair,
        MealStatus, PaycomDay, PunchEvent,
    },
    employee_sync::{EmployeeSyncSource, EmployeeSyncStatus},
    meals::{
        CortexMeal, CortexPublication, CortexUnreadable, MealComparison, MealComparisonPage,
        MealCounts, MealDriver, MealEmployee, MealPaycom, MealPeriod, MealPeriodGaps, MealSource,
        PeriodMealGap,
    },
    settings::{NameOrder, PaycomSettings},
    types::{
        DailyTimecard, DailyTimecardPage, DailyTimecards, Employee, EmployeeTimecard,
        EmployeeTimecardResponse, EmployeesResponse, InPunchKind, OutPunchKind, Punch, Timecard,
    },
};
/// What its integration tests and its offline fixture builder use beside: how a meal break
/// is assessed, and the comparison row a Cortex meal becomes.
pub use backend::meals::{assessment::assess_meal, comparison_meal};
/// What the app uses: its storage, the cache domain of the Paycom data it keeps, and its
/// cached meal-break reads.
pub use backend::{TimecardStore, meals::CACHED, punches::DOMAIN};

/// Core's test support, for this crate's module tests.
#[cfg(test)]
use dispatch_core::testing;

use backend::{meals, punches};
use dispatch_core::{
    db::{Migration, Migrations, OwnMigrations, migrations::Apply::Sql},
    manifest::{
        Audit,
        DefaultRole::{Manager, Member},
        Feature, ScheduleAlias, feature, optional, perm, tab,
    },
    server::cache::{Cached, DataDomain, Evicted::By, LISTINGS},
    tenancy::api::audit::AuditArea::Collections,
};
use dispatch_cortex as cortex;
use dispatch_paycom as paycom;

pub const FEATURE: Feature = Feature {
    place: 20,
    switch: optional("timecard", "Timecard", &["timecards"]),
    subfeatures: &[
        tab("timecard.daily", "Timecard"),
        // Meal breaks come from Cortex: only this tab needs it.
        tab("timecard.meal_breaks", "Meal Breaks").requires(&["meal_breaks"]),
        tab("timecard.employees", "Employee Search"),
    ],
    // A schedule of `both` runs its two collections at once.
    schedule_aliases: &[ScheduleAlias {
        schedule: "both",
        runs: &[paycom::timecards::JOB_KIND, cortex::meals::JOB_KIND],
    }],
    permissions: &[
        perm("timecard.view", "View Timecard", 20).demo(&[Manager, Member]),
        perm("timecard.manage", "Manage Timecard", 21).implies(&["timecard.view"]),
        perm("collections.run", "Run Collections", 22)
            .demo(&[Manager])
            // The members who could run collections were the managers before roles held
            // permissions, which the memberships' legacy column still records.
            .legacy_role("manager"),
    ],
    routes: api::routes::routes,
    // Its schedules, which employees it lists, the departments that drive and when a DA is
    // late, on its page of Settings.
    settings: &[
        backend::settings::SCHEDULES,
        backend::settings::ROSTER,
        backend::settings::DRIVERS,
        backend::settings::LATE_DAS,
    ],
    upgrade_storage: Some(backend::settings::upgrade),
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
                "meal_publication_jobs",
                "meal_itineraries",
                "meal_delivery_events",
                "meal_breaks",
                "meal_record_schema",
                "meal_records",
                "meal_sources",
                "meal_stops",
                "meal_unreadable",
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
            list: &[
                Migration {
                    id: 2,
                    name: "meal_stops",
                    apply: Sql(include_str!("migrations/cortex/0002_meal_stops.sql")),
                },
                Migration {
                    id: 3,
                    name: "meal_unreadable",
                    apply: Sql(include_str!("migrations/cortex/0003_meal_unreadable.sql")),
                },
            ],
        },
    ],
    own_migrations: &[OwnMigrations {
        kind: cortex::DATABASE,
        list: &[Migration {
            id: 1,
            name: "meal_publication_jobs",
            apply: Sql(include_str!(
                "migrations/cortex/0001_meal_publication_jobs.sql"
            )),
        }],
    }],
    domains: &[punches::DOMAIN, meals::DOMAIN],
    cached: &[
        Cached {
            read: punches::daily::CACHED,
            evicted: By(&[punches::DOMAIN, DataDomain::LIVE, DataDomain::TENANT]),
        },
        // Who each employee is comes from the directory, which keeps its own.
        Cached {
            read: punches::employees::CACHED,
            evicted: By(&[punches::DOMAIN, DataDomain::TENANT]),
        },
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
                DataDomain::DRIVERS,
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
    people: &[&punches::people::PEOPLE, &meals::people::PEOPLE],
    // What it offers every other feature and agent tool, through its contract.
    exposes: &[dispatch_core::expose!(
        dyn dispatch_timecard_contract::Timecards,
        &backend::exposed::EXPOSED,
        "timecards",
        "Each driver's Paycom timecards by day: their punches, the meal breaks they punched, the time worked, and the days collected.",
    )],
    ..feature("timecard")
};

/// Its API types, which the app's export writes to TypeScript in its `api/generated/`.
#[cfg(feature = "ts")]
pub fn typescript(cfg: &ts_rs::Config) -> dispatch_core::Typescript {
    dispatch_core::typescript!(
        cfg,
        NameOrder,
        PaycomSettings,
        Employee,
        Punch,
        InPunchKind,
        OutPunchKind,
        Timecard,
        EmployeeTimecard,
        DailyTimecard,
        DailyTimecards,
        DailyTimecardPage,
        EmployeesResponse,
        AssessedClock,
        Lunch,
        PunchEvent,
        PaycomDay,
        DeliveryGap,
        DeliveryGaps,
        MealPair,
        MealAssessment,
        MealStatus,
        LateRule,
        CortexMeal,
        CortexUnreadable,
        MealPaycom,
        MealSource,
        MealEmployee,
        MealDriver,
        CortexPublication,
        MealComparison,
        MealComparisonPage,
        MealCounts,
        MealPeriod,
        MealPeriodGaps,
        PeriodMealGap,
        EmployeeTimecardResponse,
        EmployeeSyncSource,
        EmployeeSyncStatus,
    )
}
