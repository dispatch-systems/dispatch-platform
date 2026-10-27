//! Communications: the DSP's direct messages with the members of its Slack workspaces, and the
//! texts its dispatch phones relay, read and answered from Dispatch. Its README says more.
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
    CommunicationsWorkspace, ComposeLimits, Contact, Contacts, Conversation, ConversationKind,
    ConversationMessages, ConversationState, Conversations, Correspondent, Direction,
    DispatchPhone, Elsewhere, FileStatus, Group, GroupChoice, GroupChoiceKind, GroupChoices,
    GroupEntry, GroupMember, GroupMessages, GroupPeople, GroupPerson, GroupPersonState,
    GroupProgress, Message, MessageFile, MessageGroup, Pace, Profile, Reachable, RouteDriver,
    Sender, TextContact, TextContacts, TextStatus, Today, TodaysRoutes, Typing, WaitingMessage,
    Workday, Writing,
};

use dispatch_core::db::{
    Kind, Migration, OwnMigrations,
    migrations::Apply::{Code, Sql},
};
use dispatch_core::manifest::{Feature, feature, optional, perm};
use dispatch_phones_contract::TextsChanged;
use dispatch_routes_contract::WorkPhonesChanged;

pub const FEATURE: Feature = Feature {
    place: 95,
    switch: optional("communications", "Communications", &[]),
    // It works through Slack: what's written to the app in the DSP's workspaces. Texts come
    // through whatever offers `texting`, which it needs no dependency for.
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
            "communications_text_conversations",
            "communications_texts",
            "communications_texting",
            "communications_contacts",
            "communications_blocked",
            "communications_picked",
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
            Migration {
                id: 4,
                name: "communications_texts",
                apply: Sql(include_str!("migrations/dsp/0004_communications_texts.sql")),
            },
            Migration {
                id: 5,
                name: "communications_picked",
                apply: Sql(include_str!(
                    "migrations/dsp/0005_communications_picked.sql"
                )),
            },
        ],
    }],
    // Catching up on what Dispatch didn't hear of, hourly and as the server starts; downloading
    // what members attach, sending what's due to go to groups' people, filing texts it didn't
    // hear of, and filing today's again where they belong now, every minute.
    maintenance: &[
        integrations::messages::catch_up::MAINTENANCE,
        backend::files::MAINTENANCE,
        backend::groups::send::MAINTENANCE,
        backend::texts::filing::MAINTENANCE,
    ],
    // Filing each text as it comes, and a day's again as who held its work phones settles.
    on: &[
        dispatch_core::on!(TextsChanged, backend::texts::filing::heard),
        dispatch_core::on!(WorkPhonesChanged, backend::texts::filing::refile),
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
        DispatchPhone,
        CommunicationsWaiting,
        WaitingMessage,
        CommunicationsWorkspace,
        Conversation,
        ConversationKind,
        ConversationState,
        Correspondent,
        Message,
        TextStatus,
        Direction,
        Sender,
        MessageFile,
        FileStatus,
        Conversations,
        ConversationMessages,
        ComposeLimits,
        Elsewhere,
        Workday,
        TodaysRoutes,
        RouteDriver,
        Contacts,
        Contact,
        TextContacts,
        TextContact,
        Profile,
        Today,
        Pace,
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
