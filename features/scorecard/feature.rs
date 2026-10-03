//! Scorecard: Amazon's weekly scorecard, collected from Cortex.
use crate::scorecard;
use dispatch_core::{
    db::{
        Migration, Migrations,
        migrations::Apply::{Code, Sql},
    },
    manifest::{Feature, Switch, feature, perm},
};

#[path = "api/routes.rs"]
mod api;
#[path = "mcp/mod.rs"]
pub mod mcp;

pub const FEATURE: Feature = Feature {
    // No page of its own yet, but its collection, its schedules and its weeks, apart from
    // the Timecard page.
    switch: Some(Switch {
        id: "scorecard",
        label: "Scorecard",
        requires: &["scorecard"],
    }),
    permissions: &[
        perm("scorecard.view", "View Scorecard", 50),
        perm("scorecard.collect", "Collect Scorecard", 51).implies(&["scorecard.view"]),
        perm("scorecard.manage", "Manage Scorecard", 52).implies(&["scorecard.view"]),
    ],
    routes: api::routes,
    keeps: &[&crate::scorecard::keeper::Scorecard],
    tables: &[(
        "scorecard",
        &[
            "scorecard_schema",
            "scorecard_publications",
            "scorecard_sources",
            "scorecard_weeks",
            "driver_scorecards",
            "driver_safety",
            "returns_to_station",
            "customer_feedback",
            "delivery_concessions",
            "pickup_failures",
            "safety_events",
            "dsp_quality",
            "dsp_team",
            "dsp_compliance",
            "dsp_safety",
            "dsp_working_device",
            "dsp_feedback",
            "dsp_pickups",
        ],
    )],
    migrations: &[Migrations {
        kind: scorecard::DATABASE,
        list: &[
            Migration {
                id: 1,
                name: "baseline",
                apply: Sql(include_str!("migrations/scorecard/0001_baseline.sql")),
            },
            Migration {
                id: 2,
                name: "sources",
                apply: Sql(include_str!("migrations/scorecard/0002_sources.sql")),
            },
            Migration {
                id: 3,
                name: "verified_scope",
                apply: Code(scorecard::add_verified_scope),
            },
        ],
    }],
    domains: &[scorecard::DOMAIN],
    mcp: mcp::MCP,
    people: &[&scorecard::people::Drivers],
    ..feature("scorecard")
};
