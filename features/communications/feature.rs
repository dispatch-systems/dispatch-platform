//! Communications: the DSP's direct messages with the members of its Slack workspaces, read and
//! answered from Dispatch. Its README says more.
mod api;
mod backend;
mod integrations;

/// What its API answers with, which the app writes to TypeScript.
pub use api::types::{
    Communications, CommunicationsUpdates, CommunicationsUpload, CommunicationsWaiting,
    CommunicationsWorkspace, Contact, Contacts, Conversation, ConversationMessages,
    ConversationState, Conversations, Correspondent, Direction, FileStatus, Message, MessageFile,
    Profile, ProfileToday, Sender, Typing, Writing,
};

use dispatch_core::db::{Kind, Migration, OwnMigrations, migrations::Apply::Sql};
use dispatch_core::manifest::{Feature, feature, optional, perm};

pub const FEATURE: Feature = Feature {
    place: 95,
    switch: optional("communications", "Communications", &[]),
    // It works through Slack: what's written to the app in the DSP's workspaces.
    depends_on: &["slack"],
    // Reading the DSP's conversations and answering them, one permission for both.
    permissions: &[perm("communications.use", "Use Communications", 95)],
    routes: api::routes::routes,
    domains: &[backend::storage::DOMAIN],
    tables: &[(
        "dsp",
        &[
            "communications_conversations",
            "communications_messages",
            "communications_files",
            "communications_changes",
        ],
    )],
    own_migrations: &[OwnMigrations {
        kind: Kind::DSP,
        list: &[Migration {
            id: 1,
            name: "communications",
            apply: Sql(include_str!("migrations/dsp/0001_communications.sql")),
        }],
    }],
    // Catching up on what Dispatch didn't hear of, hourly and as the server starts; downloading
    // what members attach, every minute.
    maintenance: &[
        integrations::messages::catch_up::MAINTENANCE,
        backend::files::MAINTENANCE,
    ],
    // On from the start in every workspace, in place of Slack's answer that DSP Messaging is
    // unavailable.
    integrations: &[integrations::messages::INTEGRATION],
    ..feature("communications")
};

/// Its API types, which the app's export writes to TypeScript in its `api/generated/`.
#[cfg(feature = "ts")]
pub fn typescript(cfg: &ts_rs::Config) -> dispatch_core::Typescript {
    dispatch_core::typescript!(
        cfg,
        Communications,
        CommunicationsWaiting,
        CommunicationsWorkspace,
        Conversation,
        ConversationState,
        Correspondent,
        Message,
        Direction,
        Sender,
        MessageFile,
        FileStatus,
        Conversations,
        ConversationMessages,
        Contacts,
        Contact,
        Profile,
        ProfileToday,
        Typing,
        Writing,
        CommunicationsUpdates,
        CommunicationsUpload,
    )
}
