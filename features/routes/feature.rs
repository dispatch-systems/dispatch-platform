//! Routes: each day's routes, itineraries and packages from Cortex.
mod api;
mod backend;
mod mcp;

/// What its API answers with, which the app writes to TypeScript.
pub use api::types::{
    RouteAddress, RouteBreak, RouteDayView, RouteDays, RouteItinerary, RouteItineraryDetail,
    RoutePackage, RoutePackageEvent, RoutePublication, RouteReprocess, RouteRetention, RouteStop,
    RouteTask, RouteUnknownStop,
};
/// What the app uses: its storage and its database, and how it shapes a day's capture, which
/// the app's Cortex route probe measures.
pub use backend::{DATABASE, RoutesStore, prepare};
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
    manifest::{Feature, Switch, feature, perm},
};

pub const FEATURE: Feature = Feature {
    place: 40,
    switch: Some(Switch {
        id: "routes",
        label: "Routes",
        requires: &["routes"],
    }),
    permissions: &[
        perm("routes.view", "View Routes", 30),
        perm("routes.collect", "Collect Routes", 31).implies(&["routes.view"]),
        perm("routes.manage", "Manage Routes", 32).implies(&["routes.view"]),
    ],
    routes: api::routes::routes,
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
        ],
    }],
    domains: &[backend::DOMAIN],
    maintenance: &[backend::maintenance::MAINTENANCE],
    mcp: mcp::MCP,
    people: &[&backend::people::Drivers],
    ..feature("routes")
};
