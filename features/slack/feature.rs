//! Slack: Dispatch's app in a DSP's Slack workspaces, each of which belongs to that DSP alone.
//! Its README says more.
mod api;
mod backend;

/// What its API answers with, which the app writes to TypeScript.
pub use api::types::{
    SlackAccount, SlackChannel, SlackChannels, SlackInstall, SlackLink, SlackNewLink,
    SlackWorkspace, WorkspaceStatus,
};

use dispatch_core::{
    db::{
        Kind, Migration, OwnMigrations,
        migrations::Apply::{Code, Sql},
    },
    manifest::{Audit, Commands, Feature, feature, optional, perm},
    tenancy::api::audit::AuditArea::Connections,
};

pub const FEATURE: Feature = Feature {
    place: 120,
    switch: optional("slack", "Slack", &[]),
    // What Dispatch does in each of the DSP's workspaces, apart from adding them and taking them
    // out, which those who manage the DSP's connections do.
    permissions: &[perm("slack.manage", "Manage Slack", 120)],
    // Its workspaces are among the DSP's own accounts, on Settings' DSP Connections.
    dsp_connection: true,
    routes: api::routes::routes,
    tables: &[
        ("platform", &["slack_teams"]),
        (
            "dsp",
            &[
                "slack_workspaces",
                "slack_connect_requests",
                "slack_install_links",
            ],
        ),
    ],
    // Numbered by Slack itself: core keeps both databases, and every owner adds to them.
    own_migrations: &[
        OwnMigrations {
            kind: Kind::PLATFORM,
            list: &[Migration {
                id: 1,
                name: "slack_teams",
                apply: Sql(include_str!("migrations/platform/0001_slack_teams.sql")),
            }],
        },
        OwnMigrations {
            kind: Kind::DSP,
            list: &[
                Migration {
                    id: 1,
                    name: "slack",
                    apply: Sql(include_str!("migrations/dsp/0001_slack.sql")),
                },
                Migration {
                    id: 2,
                    name: "slack_new_connections",
                    apply: Code(backend::storage::add_new_connections),
                },
            ],
        },
    ],
    // Slack's app: its ID and secret, and the secret Slack signs its requests with.
    settings: &[&[
        "SLACK_CLIENT_ID",
        "SLACK_CLIENT_SECRET",
        "SLACK_SIGNING_SECRET",
    ]],
    maintenance: &[backend::maintenance::MAINTENANCE],
    audit: Audit {
        areas: &[("slack.", Connections)],
        ..Audit::NONE
    },
    commands: Some(Commands {
        prefix: "slack-",
        run: backend::app::run,
    }),
    ..feature("slack")
};

/// Its API types, which the app's export writes to TypeScript in its `api/generated/`.
#[cfg(feature = "ts")]
pub fn typescript(cfg: &ts_rs::Config) -> dispatch_core::Typescript {
    dispatch_core::typescript!(
        cfg,
        SlackAccount,
        SlackWorkspace,
        WorkspaceStatus,
        SlackLink,
        SlackNewLink,
        SlackInstall,
        SlackChannels,
        SlackChannel,
    )
}
