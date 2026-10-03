//! Routes: each day's routes, itineraries and packages from Cortex.
use crate::{
    db::{
        Migration, Migrations,
        migrations::Apply::{Code, Sql},
    },
    manifest::{Feature, Switch, feature, perm},
    routedata,
};

#[path = "api/routes.rs"]
mod api;
#[path = "mcp/mod.rs"]
pub mod mcp;

pub const FEATURE: Feature = Feature {
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
    routes: api::routes,
    keeps: &[&crate::routedata::keeper::Routes],
    migrations: &[Migrations {
        kind: routedata::DATABASE,
        list: &[
            Migration {
                id: 1,
                name: "baseline",
                apply: Sql(include_str!("migrations/routedata/0001_baseline.sql")),
            },
            Migration {
                id: 2,
                name: "details",
                apply: Code(routedata::add_details),
            },
            Migration {
                id: 3,
                name: "task_keys",
                apply: Sql(include_str!("migrations/routedata/0003_task_keys.sql")),
            },
        ],
    }],
    domains: &[routedata::DOMAIN],
    maintenance: &[routedata::maintenance::MAINTENANCE],
    mcp: mcp::MCP,
    people: &[&routedata::people::Drivers],
    ..feature("routes")
};
