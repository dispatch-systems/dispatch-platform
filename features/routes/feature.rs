//! Routes: each day's routes, itineraries and packages from Cortex.
mod api;
mod backend;

/// What its API answers with, which the app writes to TypeScript.
pub use api::types::{
    RouteAddress, RouteBreak, RouteDayView, RouteDays, RouteItinerary, RouteItineraryDetail,
    RoutePackage, RoutePackageEvent, RoutePublication, RouteReprocess, RouteStop, RouteTask,
    RouteUnknownStop,
};
/// What a feature that depends on it saves a whole read of a day into historical route data
/// with, as the standard collection saves its own.
pub use backend::save_day;
/// What the app uses: its storage and its database, and how it shapes a day's capture, which
/// the app's Cortex route probe measures.
pub use backend::{DATABASE, RoutesStore, prepare};
/// What a feature that reads routes live asks Cortex for, keeps each whole read of today in live
/// route data with, empties it with as a day starts, and saves its day into historical route data
/// with.
pub use backend::{
    LiveKept, clear_live, keep_live, live_request, save_and_clear_live, save_live_day,
};
/// What its integration tests check beside the storage: the most jobs a scheduled run queues,
/// how a capture is shaped and how kept responses are read back.
pub use backend::{MAX_JOBS_PER_RUN, Shaper, gunzip};

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
    tables: &[
        (
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
                "staged_addresses",
                "staged_drivers",
                // The window chosen before Settings kept it, read once to move it there.
                "route_retention",
            ],
        ),
        (
            "routelive",
            &[
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
            ],
        ),
    ],
    migrations: &[
        Migrations {
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
                    name: "staged_shared_rows",
                    apply: Sql(include_str!(
                        "migrations/routedata/0007_staged_shared_rows.sql"
                    )),
                },
                Migration {
                    id: 8,
                    name: "drop_tasks_driver_day",
                    apply: Sql(include_str!(
                        "migrations/routedata/0008_drop_tasks_driver_day.sql"
                    )),
                },
                Migration {
                    id: 9,
                    name: "work_phones_phone",
                    apply: Sql(include_str!(
                        "migrations/routedata/0009_work_phones_phone.sql"
                    )),
                },
                Migration {
                    id: 10,
                    name: "itinerary_work_phone",
                    apply: Code(backend::add_work_phone),
                },
                Migration {
                    id: 11,
                    name: "drop_work_phones",
                    apply: Sql(include_str!(
                        "migrations/routedata/0011_drop_work_phones.sql"
                    )),
                },
            ],
        },
        Migrations {
            kind: backend::LIVE_DATABASE,
            list: &[
                Migration {
                    id: 1,
                    name: "baseline",
                    apply: Sql(include_str!("migrations/routelive/0001_baseline.sql")),
                },
                Migration {
                    id: 2,
                    name: "itinerary_work_phone",
                    apply: Code(backend::add_live_work_phone),
                },
                Migration {
                    id: 3,
                    name: "drop_work_phones",
                    apply: Sql(include_str!(
                        "migrations/routelive/0003_drop_work_phones.sql"
                    )),
                },
            ],
        },
    ],
    domains: &[backend::DOMAIN],
    maintenance: &[backend::maintenance::MAINTENANCE],
    // The days its collections change, told of once they succeed.
    on: &[dispatch_core::on!(
        dispatch_core::manifest::events::CollectionSucceeded,
        backend::changed::collected,
    )],
    people: &[&backend::people::PEOPLE, &backend::people::LIVE_PEOPLE],
    // Its events are its collections' and their data's, which its settings keep.
    audit: Audit {
        areas: &[("routes.", Collections)],
        ..Audit::NONE
    },
    // What it offers every other feature and agent tool, through its contracts.
    exposes: &[
        dispatch_core::expose!(
            dyn dispatch_routes_contract::DriverRoutes,
            &backend::exposed::EXPOSED,
            "driver_routes",
            "Each driver's routes day by day, with their stops and packages, and the days collected.",
        ),
        dispatch_core::expose!(
            dyn dispatch_routes_contract::LiveRouteData,
            &backend::exposed::LIVE,
            "live_route_data",
            "Today's routes as live route data last read them, and each driver's latest work phone.",
        ),
    ],
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
