//! Communications: the DSP's direct messages with the members of its Slack workspaces, and the
//! texts its dispatch phones relay, read and answered from Dispatch; and the phones themselves,
//! paired on Settings' DSP Connections. Its README says more.
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
    GroupProgress, Message, MessageFile, MessageGroup, Pace, PairingStatus, PhonePairing,
    PhonePairingStatus, PhoneRow, Phones, Profile, Reachable, RouteDriver, Sender, SentMessage,
    TextContact, TextContacts, TextStatus, Today, TodaysRoutes, Typing, WaitingMessage, Workday,
    Writing,
};

use dispatch_core::db::{
    Kind, Migration, OwnMigrations,
    migrations::Apply::{Code, Sql},
};
use dispatch_core::{
    manifest::{Audit, Feature, feature, optional, perm},
    tenancy::api::audit::AuditArea::Connections,
};
use dispatch_routes_contract::WorkPhonesChanged;

pub const FEATURE: Feature = Feature {
    place: 95,
    switch: optional("communications", "Communications", &[]),
    // It works through Slack: what's written to the app in the DSP's workspaces; and through its
    // own dispatch phones, which relay the texts its number receives and sends.
    depends_on: &["slack"],
    // Reading the DSP's conversations and answering them, one permission for both; and pairing,
    // renaming and removing its phones, as those who manage the DSP's connections may too.
    permissions: &[
        perm("communications.use", "Use Communications", 95),
        perm("phones.manage", "Manage Phones", 150),
    ],
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
            "phones_devices",
            "phones_pairings",
            "phones_texts",
            "phones_files",
            "phones_change",
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
            Migration {
                id: 6,
                name: "communications_texts_done",
                apply: Code(backend::texts::storage::add_text_done),
            },
            // The phones' tables, adopted from Phones as its migrations 1 and 2 made them: on a
            // database they ran in, each finds what it makes there already.
            Migration {
                id: 7,
                name: "phones",
                apply: Sql(include_str!("migrations/dsp/0007_phones.sql")),
            },
            Migration {
                id: 8,
                name: "phones_pairing_ids",
                apply: Code(backend::phone::storage::add_pairing_ids),
            },
        ],
    }],
    // Its dispatch phones are the DSP's own, managed on DSP Connections.
    dsp_connection: true,
    // Catching up on what Dispatch didn't hear of, hourly and as the server starts; downloading
    // what members attach, sending what's due to go to groups' people, filing texts it didn't
    // hear of, and filing today's again where they belong now, every minute; and letting go of
    // the phones' pictures never sent and codes expired, hourly.
    maintenance: &[
        integrations::messages::catch_up::MAINTENANCE,
        backend::files::MAINTENANCE,
        backend::groups::send::MAINTENANCE,
        backend::texts::filing::MAINTENANCE,
        backend::phone::tidy::MAINTENANCE,
    ],
    // A development DSP's stand-in phone, and the texts it received.
    demo: Some(backend::phone::demo::demo),
    // Pairing, renaming and removing its phones, under Connections.
    audit: Audit {
        areas: &[("phones.", Connections)],
        ..Audit::NONE
    },
    // Filing a day's texts again as who held its work phones settles.
    on: &[dispatch_core::on!(
        WorkPhonesChanged,
        backend::texts::filing::refile
    )],
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
        SentMessage,
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
        Phones,
        PhoneRow,
        PhonePairing,
        PhonePairingStatus,
        PairingStatus,
    )
}
