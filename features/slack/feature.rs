//! Slack: Dispatch's app in a DSP's Slack workspaces, each of which belongs to that DSP alone.
//! Its README says more.
mod api;
mod backend;
mod integrations;

/// What its API answers with, which the app writes to TypeScript.
pub use api::types::{
    SlackAccount, SlackCanvas, SlackCanvasLink, SlackCanvasStatus, SlackCanvases, SlackChannel,
    SlackChannelCanvas, SlackChannelCanvases, SlackChannels, SlackFolder, SlackFolders,
    SlackImportLink, SlackImportStatus, SlackImportedFile, SlackImports, SlackInstall, SlackLink,
    SlackNewLink, SlackWorkspace, WorkspaceStatus,
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
                "slack_members",
                "slack_integrations",
                "slack_import_links",
                "slack_imports",
                "slack_canvas_links",
                "slack_canvas_picks",
                "slack_canvases",
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
                Migration {
                    id: 3,
                    name: "slack_members",
                    apply: Sql(include_str!("migrations/dsp/0003_slack_members.sql")),
                },
                Migration {
                    id: 4,
                    name: "slack_integrations",
                    apply: Sql(include_str!("migrations/dsp/0004_slack_integrations.sql")),
                },
                Migration {
                    id: 5,
                    name: "slack_imports",
                    apply: Sql(include_str!(
                        "integrations/import_files/migrations/dsp/0005_slack_imports.sql"
                    )),
                },
                Migration {
                    id: 6,
                    name: "slack_canvases",
                    apply: Sql(include_str!(
                        "integrations/sync_canvases/migrations/dsp/0006_slack_canvases.sql"
                    )),
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
    // The hourly check; Import Files' minute's saving and hourly catching up; and Sync
    // Canvases' minute's syncing and its check every ten minutes.
    maintenance: &[
        backend::maintenance::MAINTENANCE,
        integrations::import_files::queue::MAINTENANCE,
        integrations::import_files::catch_up::MAINTENANCE,
        integrations::sync_canvases::queue::MAINTENANCE,
        integrations::sync_canvases::check::MAINTENANCE,
    ],
    // Its own: Import Files and Sync Canvases, switched on in each workspace's Integrations.
    integrations: &[
        integrations::import_files::INTEGRATION,
        integrations::sync_canvases::INTEGRATION,
    ],
    audit: Audit {
        areas: &[("slack.", Connections)],
        ..Audit::NONE
    },
    commands: Some(Commands {
        prefix: "slack-",
        run: backend::app::run,
    }),
    // Its workspaces' members, for Driver Match: whom the DSP can reach, not whom it employs.
    people: &[&backend::people::PEOPLE],
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
        SlackImports,
        SlackImportLink,
        SlackImportedFile,
        SlackImportStatus,
        SlackFolder,
        SlackFolders,
        SlackCanvases,
        SlackCanvasLink,
        SlackCanvas,
        SlackCanvasStatus,
        SlackChannelCanvases,
        SlackChannelCanvas,
    )
}
