//! What collectors and features declare, and the one registry the app builds from them.
//! Core reaches collectors and features through `registry()`, never by name.
//!
//! Each slot a feature fills is a module of its own here: the slot's types and the builders a
//! manifest writes it with, what the registry answers of it, and `check`, what must hold of
//! every feature's declaration when the registry is installed. Adding a slot: its module, its
//! field in `Feature` with its empty value in `feature()`, its names in the `pub use` below,
//! and its `check` in `Registry::check`.
pub mod agents;
pub mod audit;
pub mod caching;
pub mod collectors;
pub mod commands;
pub mod integrations;
pub mod keeping;
pub mod people;
pub mod permissions;
pub(crate) mod retirement;
pub mod services;
pub mod settings;
pub mod storage;
pub mod switches;
pub mod upkeep;

pub use agents::Agents;
pub use audit::Audit;
pub use collectors::{Capability, Collection, Collector};
pub use commands::{Commands, Demo};
pub use keeping::{Keeper, MAX_QUEUE_CAPACITY, MIN_QUEUE_CAPACITY, ScheduleAlias};
pub use permissions::{DefaultRole, Permission, perm};
pub use storage::Tables;
pub use switches::{Sub, Switch, mandatory, optional, sub, tab};
pub use upkeep::{Maintenance, Upkeep};

use crate::{
    Result, State,
    db::{Migrations, OwnMigrations, Store},
    server::{
        cache::{Cached, DataDomain},
        http::Route,
    },
};
use integrations::Integration;
use people::{Directory, People};
use services::Services;
use std::sync::{Arc, OnceLock};

/// Everything this build is made of. The app builds one and installs it at startup.
pub struct Registry {
    pub collectors: &'static [&'static dyn Collector],
    pub features: &'static [&'static Feature],
    /// What lets outside agents use Dispatch, if this build has it.
    pub agents: Option<&'static Agents>,
}

/// The registry installed, as given and with its features in their places.
static INSTALLED: OnceLock<(&'static Registry, &'static Registry)> = OnceLock::new();

/// Makes `registry` the one `registry()` answers, its features in their places, once it has
/// checked it. Installing it again changes nothing; installing a different one panics.
pub fn install(registry: &'static Registry) {
    let (given, placed) = *INSTALLED.get_or_init(|| (registry, placed(registry)));
    placed.check();
    assert!(
        std::ptr::eq(given, registry),
        "a different registry is already installed"
    );
}
/// `registry` with its features in their places: by `place`, then by name.
fn placed(registry: &'static Registry) -> &'static Registry {
    let mut features = registry.features.to_vec();
    features.sort_by_key(|feature| (feature.place, feature.name));
    Box::leak(Box::new(Registry {
        collectors: registry.collectors,
        features: Box::leak(features.into_boxed_slice()),
        agents: registry.agents,
    }))
}

/// Installs a registry that leaves out features the product has, as only the proof that
/// each can be removed builds. Its tests run it the way each owner's tests run theirs, with a
/// stand-in for what the features left out would bring; nothing else may run it.
pub fn install_partial(registry: &'static Registry) {
    #[cfg(feature = "testing")]
    crate::testing::install_with(registry.agents, registry.collectors, registry.features);
    #[cfg(not(feature = "testing"))]
    panic!(
        "this build leaves features out, so only its tests run; it has {} features",
        registry.features.len()
    );
}

/// The installed registry.
pub fn registry() -> &'static Registry {
    installed().expect("no registry is installed: the app installs one before anything reads it")
}

/// The installed registry, if there is one yet.
pub fn installed() -> Option<&'static Registry> {
    INSTALLED.get().map(|(_, placed)| *placed)
}

impl Registry {
    /// Panics unless each slot's declarations hold, as each slot's `check` says, and every
    /// feature depends only on registered features and collectors.
    pub fn check(&self) {
        settings::check(self);
        retirement::check(self);
        keeping::check(self);
        switches::check(self);
        for feature in self.features {
            for dependency in feature.depends_on {
                assert!(
                    self.features.iter().any(|other| other.name == *dependency)
                        || self.collectors.iter().any(|c| c.id() == *dependency),
                    "{} depends on {dependency}, which is not registered",
                    feature.name
                );
            }
        }
        integrations::check(self.features);
        services::check(self.features);
        people::check(self);
        permissions::check(self);
        storage::check(self);
        caching::check(self);
        audit::check(self);
        agents::check(self);
    }
}

/// What a feature declares in its `feature.rs`. Fields join as the slots that read them land.
pub struct Feature {
    /// Its directory's name.
    pub name: &'static str,
    /// Where it sits among the features: its pages in the sidebar, its page on the DSPs page,
    /// its permissions' section on the role sheet. Lowest first; two at one place go by name,
    /// so a number need not be free.
    pub place: u16,
    /// Identifiers migrated at startup and rejected by current readers.
    pub retired_identifiers: &'static [(&'static str, &'static str)],
    /// Owner storage import, before collector storage is opened.
    pub upgrade_storage: Option<fn(&Store, &str) -> Result<()>>,
    /// The features and collectors it uses, by name, beyond the collectors whose
    /// collections it keeps.
    pub depends_on: &'static [&'static str],
    /// Whether every DSP has it, or the platform owner switches it per DSP: `optional(..)`,
    /// off for every DSP until switched on, or `mandatory(..)`. Every feature says which.
    pub switch: Switch,
    /// The parts of its page switched on their own on the DSPs page, in the order the page
    /// shows them: its tabs, and any other part, each with the permissions it owns.
    pub subfeatures: &'static [Sub],
    /// The names a schedule may give to several of the collections it keeps, to run them
    /// at once, as Timecard's `both` runs Paycom's timecards and Cortex's meal breaks.
    pub schedule_aliases: &'static [ScheduleAlias],
    /// The permissions it owns, for the role sheet.
    pub permissions: &'static [Permission],
    /// Whether it adds one of the DSP's own accounts to Settings' DSP Connections, as a
    /// collector's connection does: Manage DSP Connections exists while it is on, to manage
    /// that account.
    pub dsp_connection: bool,
    /// Its endpoints, each with its path and access. The app serves every feature's.
    pub routes: fn() -> Vec<Route>,
    /// The permissions that may follow its collections' progress as it arrives, through
    /// the collection updates the dashboard waits on.
    pub live: &'static [&'static str],
    /// The server's settings it reads with `config.setting`, in groups that go together:
    /// `&[&["GOOGLE_CLIENT_ID", "GOOGLE_CLIENT_SECRET"]]`. A server with only part of a
    /// group set refuses to start.
    pub settings: &'static [&'static [&'static str]],
    /// The collections it keeps.
    pub keeps: &'static [&'static dyn Keeper],
    /// What it adds to databases: its own, kept beside a collector's, or another owner's.
    /// To a database other owners add to as well, it adds only `own_migrations` now; what it
    /// declared here before stays, recorded as it was.
    pub migrations: &'static [Migrations],
    /// What it adds to a database other owners add to as well (each DSP's, the platform's,
    /// a collector's), numbered by the feature itself from 1 and recorded under its name.
    pub own_migrations: &'static [OwnMigrations],
    /// Immutable ledgers needed to read retired storage during an owner-controlled import.
    /// These databases are never created or opened as current storage.
    pub retired_migrations: &'static [Migrations],
    /// The tables it keeps, wherever they are, and no other owner's code runs SQL on.
    pub tables: Tables,
    /// The kinds of data its reviewed writes change, for the read cache.
    pub domains: &'static [DataDomain],
    /// What evicts the reads it caches, and any other owner's read its data feeds.
    pub cached: &'static [Cached],
    /// Upkeep it needs the scheduler to run.
    pub maintenance: &'static [Maintenance],
    /// Runs for the DSP after each of its collections succeeds, whichever it is, once its
    /// outcome is recorded and before the dashboard hears of it.
    pub after_collection: Option<fn(Arc<State>, String) -> Upkeep>,
    /// How its actions read in the activity log.
    pub audit: Audit,
    /// Fills a new DSP with demo data: the development seed's DSPs and Diagnostics' test
    /// DSPs.
    pub demo: Option<Demo>,
    /// Operator commands of the binary it answers for.
    pub commands: Option<Commands>,
    /// Who its data names, for the feature that tells people apart: `people::People` says how to
    /// connect one kind of data.
    pub people: &'static [&'static People],
    /// Who each ID its people name is, if it is the feature that tells them apart: one
    /// feature at most answers.
    pub directory: Option<&'static dyn Directory>,
    /// What it does through other features' connections, which their settings switch on and
    /// off for each account: `integrations::Integration` says how to declare one.
    pub integrations: &'static [Integration],
    /// What it offers every other feature, each through a contract in `services`: one feature
    /// at most offers each.
    pub services: Services,
}
/// A feature that fills no slot yet. A manifest starts here and names what it adds:
/// `Feature { …, ..feature("timecard") }`.
pub const fn feature(name: &'static str) -> Feature {
    Feature {
        name,
        place: u16::MAX,
        retired_identifiers: &[],
        upgrade_storage: None,
        depends_on: &[],
        switch: switches::UNDECLARED,
        subfeatures: &[],
        schedule_aliases: &[],
        permissions: &[],
        dsp_connection: false,
        routes: Vec::new,
        live: &[],
        settings: &[],
        keeps: &[],
        migrations: &[],
        own_migrations: &[],
        retired_migrations: &[],
        tables: &[],
        domains: &[],
        cached: &[],
        maintenance: &[],
        after_collection: None,
        audit: Audit::NONE,
        demo: None,
        commands: None,
        people: &[],
        directory: None,
        integrations: &[],
        services: Services::NONE,
    }
}
