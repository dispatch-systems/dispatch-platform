//! Driver Match: one code for each person, whichever of the DSP's features name them and
//! however each writes them. Every DSP has it. Its README says more.
mod api;
mod backend;

/// What its API answers with, which the app writes to TypeScript.
pub use api::types::{
    Appearance, DriverMatch, Evidence, EvidenceKind, IdLink, MatchCounts, Person, PersonDetails,
    PersonEvent, PersonEventKind, PersonId, PersonRole, PersonTitle, Question, SourceDays,
    Strength,
};
/// Its storage and what one pass reads, for the app's tests. Other features ask core's
/// directory who an ID is.
pub use backend::{Collected, DriverMatchStore};

use dispatch_core::{
    db::{Kind, Migration, Migrations, OwnMigrations, migrations::Apply::Sql},
    manifest::{
        Audit, Feature,
        events::{CollectionSucceeded, PeopleChanged},
        feature, mandatory,
        people::Directory,
        perm,
    },
};

pub const FEATURE: Feature = Feature {
    place: 75,
    // Every DSP has it: what the DSP's features name, it tells apart. Its one screen is a page
    // of its own.
    switch: mandatory("driver_match", "Driver Match"),
    permissions: &[perm("driver_match.manage", "Manage Driver Match", 60)],
    routes: api::routes::routes,
    tables: &[
        ("platform", &["driver_codes"]),
        (
            "dsp",
            &["people", "person_ids", "people_apart", "person_titles"],
        ),
    ],
    migrations: &[
        Migrations {
            kind: Kind::PLATFORM,
            list: &[Migration {
                id: 10,
                name: "driver_codes",
                apply: Sql(include_str!("migrations/platform/0010_driver_codes.sql")),
            }],
        },
        Migrations {
            kind: Kind::DSP,
            list: &[Migration {
                id: 7,
                name: "driver_match",
                apply: Sql(include_str!("migrations/dsp/0007_driver_match.sql")),
            }],
        },
    ],
    own_migrations: &[OwnMigrations {
        kind: Kind::DSP,
        list: &[
            Migration {
                id: 1,
                name: "open_sources",
                apply: Sql(include_str!("migrations/dsp/0001_open_sources.sql")),
            },
            Migration {
                id: 2,
                name: "meal_break_links",
                apply: Sql(include_str!("migrations/dsp/0002_meal_break_links.sql")),
            },
            Migration {
                id: 3,
                name: "person_titles",
                apply: Sql(include_str!("migrations/dsp/0003_person_titles.sql")),
            },
        ],
    }],
    maintenance: &[backend::maintenance::MAINTENANCE],
    // Its events name drivers by their codes, and the log puts today's names to them.
    audit: Audit {
        subjects: &["driver"],
        names: Some(backend::name_driver_events),
        ..Audit::NONE
    },
    // Other features ask it who an ID is, through core's directory.
    exposes: &[dispatch_core::expose!(
        dyn Directory,
        &backend::Directory,
        "directory",
        "Who an ID is: everyone the DSP's features name, told apart.",
    )],
    // Whoever a collection or another feature brings in gets a code at once.
    on: &[
        dispatch_core::on!(CollectionSucceeded, backend::maintenance::collected),
        dispatch_core::on!(PeopleChanged, backend::maintenance::changed),
    ],
    ..feature("driver_match")
};

/// Its API types, which the app's export writes to TypeScript in its `api/generated/`.
#[cfg(feature = "ts")]
pub fn typescript(cfg: &ts_rs::Config) -> dispatch_core::Typescript {
    dispatch_core::typescript!(
        cfg,
        IdLink,
        Strength,
        EvidenceKind,
        PersonEventKind,
        PersonId,
        Person,
        MatchCounts,
        Evidence,
        Question,
        DriverMatch,
        Appearance,
        SourceDays,
        PersonEvent,
        PersonDetails,
    )
}
