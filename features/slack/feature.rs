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

/// What another feature's integration through Slack uses of Slack, beside core's
/// `Integration`: the bot that acts in a DSP's workspace, calling Slack and downloading and
/// uploading files there; what Slack keeps of the DSP's workspaces and their members, to read;
/// linking a workspace's channels to folders where the DSP keeps its files, as Import Files and
/// Sync Canvases do; and the queue their work that mustn't be lost is kept in. Its folder takes
/// their shape, `integrations/<name>/` in its feature, as `dispatchdev new integration <name>
/// --to <feature>` writes it, and its feature declares `slack` in `depends_on` once it uses this.
pub mod kit {
    pub use crate::backend::bot::Bot;
    pub use crate::backend::channel_links::{
        Linking,
        storage::{Link, LinkStore},
    };
    pub use crate::backend::kit::{
        SOURCE, integration_on_since, member_id, members, workspaces, workspaces_on,
    };
    pub use crate::backend::members::member;
    pub use crate::backend::slack::Bytestream;
    pub use crate::backend::storage::{Member, Workspace};
    pub use crate::backend::work::{Queue, dsps, due, failed, pass};
}
/// What a dependant's tests use of Slack, behind its `testing` feature: connecting one of a
/// DSP's fixture workspaces, reading its members, switching an integration there, what the bot
/// posted there, and disconnecting it.
#[cfg(any(test, feature = "testing"))]
pub mod testing {
    pub use crate::backend::fixture::{Posted, posted};
    pub use crate::backend::members::read as read_members;
    pub use crate::backend::testing::{connect, disconnect, switch};
}

pub const FEATURE: Feature = Feature {
    place: 120,
    switch: optional("slack", "Slack", &[]),
    // What Dispatch does in each of the DSP's workspaces, apart from adding them and taking them
    // out, which those who manage the DSP's connections do.
    permissions: &[perm("slack.manage", "Manage Slack", 120)],
    // Its workspaces are among the DSP's own accounts, on Settings' DSP Connections.
    dsp_connection: true,
    routes: api::routes::routes,
    // Its workspaces' settings, on its own page of Settings.
    settings: &[backend::settings::WORKSPACES],
    tables: &[
        ("platform", &["slack_teams"]),
        (
            "dsp",
            &[
                "slack_workspaces",
                "slack_members",
                "slack_integrations",
                "slack_integrations_off",
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
                Migration {
                    id: 7,
                    name: "slack_integrations_off",
                    apply: Sql(include_str!(
                        "migrations/dsp/0007_slack_integrations_off.sql"
                    )),
                },
                Migration {
                    id: 8,
                    name: "slack_workspaces_added",
                    apply: Code(backend::storage::add_workspaces_added),
                },
            ],
        },
    ],
    // Slack's app: its ID and secret, and the secret Slack signs its requests with.
    env: &[&[
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
    // What it offers every other feature and agent tool, through its contract.
    exposes: &[dispatch_core::expose!(
        dyn dispatch_slack_contract::ChatAccounts,
        &backend::exposed::EXPOSED,
        "chat_accounts",
        "Each person's accounts in the DSP's Slack workspaces: where, and whether each is active.",
    )],
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
