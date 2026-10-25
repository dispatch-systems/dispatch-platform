//! Routes: each day's routes, itineraries and packages from Cortex.
mod api;
mod backend;

/// What its API answers with, which the app writes to TypeScript.
pub use api::types::{
    RouteAddress, RouteBreak, RouteDayView, RouteDays, RouteItinerary, RouteItineraryDetail,
    RoutePackage, RoutePackageEvent, RoutePublication, RouteReprocess, RouteStop, RouteTask,
    RouteUnknownStop,
};
/// What the app uses: its storage and its database, and how it shapes a day's capture, which
/// the app's Cortex route probe measures.
pub use backend::{DATABASE, RoutesStore, prepare};
/// What its integration tests check beside the storage: the most jobs a scheduled run queues,
/// how a capture is shaped and how kept responses are read back.
pub use backend::{MAX_JOBS_PER_RUN, Shaper, gunzip};
/// What a feature that depends on it saves a whole read of a day into historical route data
/// with, as the standard collection saves its own, the day's work phones with it.
pub use backend::{WorkPhone, save_day};

/// Core's test support, for this crate's module tests.
#[cfg(test)]
use dispatch_core::testing;

use dispatch_core::{
    db::{
        Migration, Migrations,
        migrations::Apply::{Code, Sql},
    },
    manifest::{Audit, Feature, ScheduleAlias, feature, optional, perm},
    tenancy::api::audit::AuditArea::Collections,
};

pub const FEATURE: Feature = Feature {
    place: 40,
    switch: optional("routes", "Routes", &["routes"]),
    // A schedule of `routes_today` reads today so far; one of `routes`, yesterday.
    schedule_aliases: &[ScheduleAlias {
        schedule: backend::TODAY,
        runs: &[dispatch_cortex::routes::JOB_KIND],
    }],
    permissions: &[
        perm("routes.view", "View Routes", 30),
        perm("routes.collect", "Collect Routes", 31).implies(&["routes.view"]),
        perm("routes.manage", "Manage Routes", 32).implies(&["routes.view"]),
    ],
    routes: api::routes::routes,
    // How long its data is kept, on Settings' Data page.
    settings: &[backend::settings::RETENTION],
    upgrade_storage: Some(backend::settings::upgrade),
    keeps: &[&backend::keeper::Routes],
    tables: &[(
        "routedata",
        &[
            "routedata_schema",
            "route_publications",
            "routes",
            "itineraries",
            "stops",
            "tasks",
            "addresses",
            "drivers",
            "driver_days",
            "route_raw",
            "breaks",
            "unknown_stops",
            "work_phones",
            // The window chosen before Settings kept it, read once to move it there.
            "route_retention",
        ],
    )],
    migrations: &[Migrations {
        kind: DATABASE,
        list: &[
            Migration {
                id: 1,
                name: "baseline",
                apply: Sql(include_str!("migrations/routedata/0001_baseline.sql")),
            },
            Migration {
                id: 2,
                name: "details",
                apply: Code(backend::add_details),
            },
            Migration {
                id: 3,
                name: "task_keys",
                apply: Sql(include_str!("migrations/routedata/0003_task_keys.sql")),
            },
            Migration {
                id: 4,
                name: "driver_day_index",
                apply: Sql(include_str!(
                    "migrations/routedata/0004_driver_day_index.sql"
                )),
            },
            Migration {
                id: 5,
                name: "package_flags_index",
                apply: Sql(include_str!(
                    "migrations/routedata/0005_package_flags_index.sql"
                )),
            },
            Migration {
                id: 6,
                name: "work_phones",
                apply: Sql(include_str!("migrations/routedata/0006_work_phones.sql")),
            },
            Migration {
                id: 7,
                name: "shared_rows_collected",
                apply: Code(backend::add_read_times),
            },
        ],
    }],
    domains: &[backend::DOMAIN],
    maintenance: &[backend::maintenance::MAINTENANCE],
    people: &[&backend::people::PEOPLE],
    // Its events are its collections' and their data's, which its settings keep.
    audit: Audit {
        areas: &[("routes.", Collections)],
        ..Audit::NONE
    },
    // What it offers every other feature and agent tool, through its contract.
    exposes: &[dispatch_core::expose!(
        dyn dispatch_routes_contract::DriverRoutes,
        &backend::exposed::EXPOSED,
        "driver_routes",
        "Each driver's routes day by day, with their stops and packages, and the days collected.",
    )],
    ..feature("routes")
};

/// Its API types, which the app's export writes to TypeScript in its `api/generated/`.
#[cfg(feature = "ts")]
pub fn typescript(cfg: &ts_rs::Config) -> dispatch_core::Typescript {
    dispatch_core::typescript!(
        cfg,
        RouteAddress,
        RouteBreak,
        RouteDayView,
        RouteDays,
        RouteItinerary,
        RouteItineraryDetail,
        RoutePackage,
        RoutePackageEvent,
        RoutePublication,
        RouteReprocess,
        RouteStop,
        RouteTask,
        RouteUnknownStop,
    )
}
