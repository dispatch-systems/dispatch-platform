//! Communications: the DSP's direct messages with the members of its Slack workspaces, read and
//! answered from Dispatch. Its README says more.
mod api;
mod backend;
mod integrations;

/// How each member has the page look, as Settings keeps it for them.
pub use api::display::{
    BubbleShape, EmployeeColor, LineSpacing, ListPreview, ListRows, ListWidth, MessageDisplay,
    MessageLayout, MessageStyle, ProfileOpens, Spacing, TeamColor, TeamSide, Times,
};
/// What its API answers with, which the app writes to TypeScript.
pub use api::types::{
    Communications, CommunicationsUpdates, CommunicationsUpload, CommunicationsWaiting,
    CommunicationsWorkspace, Contact, Contacts, Conversation, ConversationMessages,
    ConversationState, Conversations, Correspondent, Direction, FileStatus, Group, GroupChoice,
    GroupChoiceKind, GroupChoices, GroupEntry, GroupMember, GroupMessages, GroupPeople,
    GroupPerson, GroupPersonState, GroupProgress, Message, MessageFile, MessageGroup, Profile,
    ProfileToday, Reachable, Sender, Typing, Writing,
};

use dispatch_core::db::{
    Kind, Migration, OwnMigrations,
    migrations::Apply::{Code, Sql},
};
use dispatch_core::manifest::{Feature, feature, optional, perm};

pub const FEATURE: Feature = Feature {
    place: 95,
    switch: optional("communications", "Communications", &[]),
    // It works through Slack: what's written to the app in the DSP's workspaces.
    depends_on: &["slack"],
    // Reading the DSP's conversations and answering them, one permission for both.
    permissions: &[perm("communications.use", "Use Communications", 95)],
    // How the page looks to each member, their own, on Settings' Display page.
    settings: &[backend::display::DISPLAY],
    routes: api::routes::routes,
    domains: &[backend::storage::DOMAIN],
    tables: &[(
        "dsp",
        &[
            "communications_conversations",
            "communications_messages",
            "communications_files",
            "communications_changes",
            "communications_groups",
            "communications_group_people",
            "communications_group_messages",
            "communications_deliveries",
            "communications_group_uploads",
        ],
    )],
    own_migrations: &[OwnMigrations {
        kind: Kind::DSP,
        list: &[
            Migration {
                id: 1,
                name: "communications",
                apply: Sql(include_str!("migrations/dsp/0001_communications.sql")),
            },
            Migration {
                id: 2,
                name: "communications_groups",
                apply: Sql(include_str!(
                    "migrations/dsp/0002_communications_groups.sql"
                )),
            },
            Migration {
                id: 3,
                name: "communications_grouped",
                apply: Code(backend::storage::add_groups),
            },
        ],
    }],
    // Catching up on what Dispatch didn't hear of, hourly and as the server starts; downloading
    // what members attach, and sending what's due to go to groups' people, every minute.
    maintenance: &[
        integrations::messages::catch_up::MAINTENANCE,
        backend::files::MAINTENANCE,
        backend::groups::send::MAINTENANCE,
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
        MessageGroup,
        Group,
        GroupEntry,
        GroupProgress,
        GroupMessages,
        GroupPeople,
        GroupPerson,
        GroupPersonState,
        GroupChoices,
        Reachable,
        GroupMember,
        GroupChoice,
        GroupChoiceKind,
        MessageDisplay,
        LineSpacing,
        Spacing,
        MessageLayout,
        MessageStyle,
        TeamSide,
        BubbleShape,
        Times,
        TeamColor,
        EmployeeColor,
        ListWidth,
        ListRows,
        ListPreview,
        ProfileOpens,
    )
}
