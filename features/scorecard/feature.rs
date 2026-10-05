//! Scorecard: Amazon's weekly scorecard, collected from Cortex.
mod api;
mod backend;
mod mcp;

/// What its API answers with, which the app writes to TypeScript.
pub use api::types::{ScorecardDatasetCount, ScorecardPublication, ScorecardWeek, ScorecardWeeks};
/// What the app uses: its storage and its database.
pub use backend::{DATABASE, ScorecardStore};
/// What agents can ask of it, which the app's tests ask directly.
pub use mcp::scorecard::{feedback, returns, safety, weekly};

use dispatch_core::{
    db::{
        Migration, Migrations,
        migrations::Apply::{Code, Sql},
    },
    manifest::{Feature, Switch, feature, perm},
};

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
    routes: api::routes::routes,
    keeps: &[&backend::keeper::Scorecard],
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
        kind: DATABASE,
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
                apply: Code(backend::add_verified_scope),
            },
        ],
    }],
    domains: &[backend::DOMAIN],
    mcp: mcp::MCP,
    people: &[&backend::people::Drivers],
    ..feature("scorecard")
};
