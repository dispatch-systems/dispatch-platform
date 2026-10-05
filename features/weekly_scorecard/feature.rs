//! Weekly Scorecard: Amazon's weekly scorecard, collected from Cortex.
mod api;
mod backend;
mod mcp;

/// What its API answers with, which the app writes to TypeScript.
pub use api::types::{
    WeeklyScorecardDatasetCount, WeeklyScorecardPublication, WeeklyScorecardWeek,
    WeeklyScorecardWeeks,
};
/// What the app uses: its storage and its database.
pub use backend::{DATABASE, WeeklyScorecardStore};
/// What agents can ask of it, which the app's tests ask directly.
pub use mcp::weekly_scorecard::{feedback, returns, safety, weekly};

use dispatch_core::{
    db::{
        Migration, Migrations,
        migrations::Apply::{Code, Sql},
    },
    manifest::{Feature, Switch, feature, perm},
};

pub const FEATURE: Feature = Feature {
    identifiers: &[
        ("scorecard", "weekly_scorecard"),
        ("scorecard.view", "weekly_scorecard.view"),
        ("scorecard.collect", "weekly_scorecard.collect"),
        ("scorecard.manage", "weekly_scorecard.manage"),
        (
            "cortex.scorecard.collect",
            "cortex.weekly_scorecard.collect",
        ),
    ],
    // No page of its own yet, but its collection, its schedules and its weeks, apart from
    // the Timecard page.
    switch: Some(Switch {
        id: "weekly_scorecard",
        label: "Weekly Scorecard",
        requires: &["weekly_scorecard"],
    }),
    permissions: &[
        perm("weekly_scorecard.view", "View Weekly Scorecard", 50),
        perm("weekly_scorecard.collect", "Collect Weekly Scorecard", 51)
            .implies(&["weekly_scorecard.view"]),
        perm("weekly_scorecard.manage", "Manage Weekly Scorecard", 52)
            .implies(&["weekly_scorecard.view"]),
    ],
    routes: api::routes::routes,
    keeps: &[&backend::keeper::WeeklyScorecard],
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
    ..feature("weekly_scorecard")
};
