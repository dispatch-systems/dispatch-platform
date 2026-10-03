//! What collectors and features declare, and the one registry the app builds from them.
//! Core reaches collectors and features through `registry()`, never by name.
use crate::{
    Code, Error, Result, State,
    browsers::{Collected, Driver, Pending, browseros},
    collectors::AddedStorage,
    config::Config,
    contracts::AuditArea,
    db::{self, Db, Kind, Migration, Migrations, Store},
    job_metrics::Counts,
    read_cache::{self, Cached, DataDomain},
};
use serde_json::Value;
use std::{
    future::Future,
    path::Path,
    pin::Pin,
    sync::{Arc, OnceLock},
    time::Duration,
};

/// Everything this build is made of. The app builds one and installs it at startup.
pub struct Registry {
    pub collectors: &'static [&'static dyn Collector],
    pub features: &'static [&'static Feature],
}

static INSTALLED: OnceLock<&'static Registry> = OnceLock::new();

/// Makes `registry` the one `registry()` answers, once it has checked it. Installing it
/// again changes nothing; installing a different one panics.
pub fn install(registry: &'static Registry) {
    registry.check();
    let installed = *INSTALLED.get_or_init(|| registry);
    assert!(
        std::ptr::eq(installed, registry),
        "a different registry is already installed"
    );
}

/// The installed registry.
pub fn registry() -> &'static Registry {
    // A module's own tests run without the app's startup, so they find the app's registry.
    #[cfg(test)]
    INSTALLED.get_or_init(|| &crate::REGISTRY);
    INSTALLED
        .get()
        .copied()
        .expect("no registry is installed: the app installs one before anything reads it")
}

impl Registry {
    /// Every feature's keepers, in the registry's order.
    pub fn keepers(&self) -> impl Iterator<Item = &'static dyn Keeper> {
        self.features
            .iter()
            .flat_map(|feature| feature.keeps.iter().copied())
    }
    /// The feature that keeps the collection a job of `kind` collects.
    pub fn keeping(&self, kind: &str) -> Option<&'static Feature> {
        self.features
            .iter()
            .copied()
            .find(|feature| feature.keeps.iter().any(|keeper| keeper.keeps() == kind))
    }
    /// The keeper of the collection a job of `kind` collects.
    pub fn keeper(&self, kind: &str) -> &'static dyn Keeper {
        self.keepers()
            .find(|keeper| keeper.keeps() == kind)
            .unwrap_or_else(|| panic!("no feature keeps {kind}"))
    }
    /// Every kind of database: core's, each collector's, then those the keepers of its
    /// collections add beside it, in the order of its collections.
    pub fn databases(&self) -> impl Iterator<Item = Kind> {
        let collectors = self.collectors.iter().map(|collector| collector.database());
        let added = self
            .collectors
            .iter()
            .flat_map(|collector| collector.collections())
            .flat_map(|collection| self.keepers().filter(|k| k.keeps() == collection.job_kind))
            .flat_map(|keeper| keeper.storages().iter().map(|storage| storage.kind));
        db::CORE_DATABASES
            .iter()
            .copied()
            .chain(collectors)
            .chain(added)
    }
    /// What every owner adds to the databases: core's parts, each collector, each feature.
    fn migration_lists(&self) -> impl Iterator<Item = &'static Migrations> {
        let collectors = self.collectors.iter().flat_map(|c| c.migrations());
        let features = self.features.iter().flat_map(|feature| feature.migrations);
        db::CORE_MIGRATIONS.iter().chain(collectors).chain(features)
    }
    /// One kind of database's migrations, gathered from every owner, in order.
    pub fn migrations(&self, kind: Kind) -> Vec<Migration> {
        db::migrations::ledger(kind, self.migration_lists())
    }
    /// Every domain a reviewed write may name: core's, then each feature's.
    pub fn domains(&self) -> impl Iterator<Item = DataDomain> {
        let features = self.features.iter().flat_map(|feature| feature.domains);
        read_cache::DOMAINS.iter().chain(features).copied()
    }
    /// What evicts each cached read: core's declarations, then each feature's.
    pub fn cached(&self) -> impl Iterator<Item = &'static Cached> {
        let features = self.features.iter().flat_map(|feature| feature.cached);
        read_cache::CACHED.iter().chain(features)
    }
    /// Every permission as declared: core's own, then each feature's, in the registry's
    /// order. Lists of permissions follow their `order` instead.
    pub fn permissions(&self) -> impl Iterator<Item = &'static Permission> {
        let features = self.features.iter().flat_map(|feature| feature.permissions);
        CORE_PERMISSIONS.iter().copied().chain(features)
    }
    /// Panics unless every collection has exactly one keeper, every keeper keeps a
    /// registered collection, one page runs the schedules, only a page has tabs, every
    /// permission has an id and an order of its own and implies only permissions that
    /// exist, every database is declared once, with migrations numbered from 1 without a
    /// gap or a repeat, every domain is declared once and before it is named, and every
    /// audit prefix is a dotted name listed under an area other than settings.
    pub fn check(&self) {
        let kinds: Vec<&str> = self
            .collectors
            .iter()
            .flat_map(|collector| collector.collections())
            .map(|collection| collection.job_kind)
            .collect();
        for kind in &kinds {
            let keepers = self.keepers().filter(|k| k.keeps() == *kind).count();
            assert!(keepers == 1, "{kind} has {keepers} keepers, not one");
        }
        for keeper in self.keepers() {
            assert!(
                kinds.contains(&keeper.keeps()),
                "{} is kept, but no collector collects it",
                keeper.keeps()
            );
        }
        let schedules: Vec<_> = self.features.iter().filter(|f| f.schedules).collect();
        assert!(
            schedules.len() == 1 && schedules[0].switch.is_some(),
            "exactly one page runs the schedules"
        );
        for feature in self.features {
            assert!(
                feature.switch.is_some() || feature.tabs.is_empty(),
                "{} has tabs but no page",
                feature.name
            );
        }
        let permissions: Vec<_> = self.permissions().collect();
        for (index, permission) in permissions.iter().enumerate() {
            assert!(
                permissions[..index]
                    .iter()
                    .all(|other| other.id != permission.id && other.order != permission.order),
                "{} repeats another permission's id or order",
                permission.id
            );
            for implied in permission.implies {
                assert!(
                    permissions.iter().any(|other| other.id == *implied),
                    "{} implies {implied}, which is not a permission",
                    permission.id
                );
            }
        }
        let databases: Vec<Kind> = self.databases().collect();
        for (index, kind) in databases.iter().enumerate() {
            assert!(
                databases[..index]
                    .iter()
                    .all(|other| other.name() != kind.name()),
                "the {} database is declared twice",
                kind.name()
            );
            self.migrations(*kind);
        }
        for owned in self.migration_lists() {
            assert!(
                databases.contains(&owned.kind),
                "migrations name a {} database that is not declared, or not as declared",
                owned.kind.name()
            );
        }
        let domains: Vec<DataDomain> = self.domains().collect();
        for (index, domain) in domains.iter().enumerate() {
            assert!(
                !domains[..index].contains(domain),
                "the {} domain is declared twice",
                domain.id()
            );
        }
        let named = self
            .keepers()
            .map(|keeper| keeper.domain())
            .chain(self.cached().flat_map(|cached| match cached.evicted {
                read_cache::Evicted::By(named) | read_cache::Evicted::ByAllBut(named) => {
                    named.iter().copied()
                }
            }));
        for domain in named {
            assert!(
                domains.contains(&domain),
                "the {} domain is named but not declared",
                domain.id()
            );
        }
        // Prefixes are written into the log's SQL.
        for (prefix, area) in self.features.iter().flat_map(|f| f.audit.areas) {
            let plain = prefix.ends_with('.')
                && prefix
                    .split('.')
                    .all(|part| part.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'));
            assert!(
                plain && *area != AuditArea::Settings,
                "{prefix} is no audit prefix of an area"
            );
        }
    }
}

/// What a feature declares in its `feature.rs`. Fields join as the slots that read them land.
pub struct Feature {
    /// Its directory's name.
    pub name: &'static str,
    /// Its page's switch on the DSPs page. `None` for a feature that is always on.
    pub switch: Option<Switch>,
    /// Its page's tabs, each switched on its own, in the order the page shows them.
    pub tabs: &'static [Tab],
    /// Whether its page is the one whose schedules, collections and jobs run: the
    /// generic schedule and job routes', and a `both` schedule's. One feature's is.
    pub schedules: bool,
    /// The permissions it owns, for the role sheet.
    pub permissions: &'static [Permission],
    /// The permissions that may follow its collections' progress as it arrives, through
    /// the collection updates the dashboard waits on.
    pub live: &'static [&'static str],
    /// The collections it keeps.
    pub keeps: &'static [&'static dyn Keeper],
    /// What it adds to databases: its own, kept beside a collector's, or another owner's.
    pub migrations: &'static [Migrations],
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
}
/// A feature that fills no slot yet. A manifest starts here and names what it adds:
/// `Feature { …, ..feature("timecard") }`.
pub const fn feature(name: &'static str) -> Feature {
    Feature {
        name,
        switch: None,
        tabs: &[],
        schedules: false,
        permissions: &[],
        live: &[],
        keeps: &[],
        migrations: &[],
        domains: &[],
        cached: &[],
        maintenance: &[],
        after_collection: None,
        audit: Audit::NONE,
        demo: None,
        commands: None,
    }
}

/// Fills one DSP, given its timezone, with what a feature shows.
pub type Demo = fn(&Store, &str, &str) -> Result<()>;
/// A family of operator commands, run as `dispatch-backend <name> <arguments>` without the
/// platform lock, so each must be safe beside a running server.
pub struct Commands {
    /// What their names begin with. Every name so begun is theirs, and `run` answers one it
    /// does not know with its usage error.
    pub prefix: &'static str,
    /// Runs one, given every argument, its name first, and answers what it prints.
    pub run: fn(&Config, &[String]) -> Result<Value>,
}

/// How a feature's actions read in the activity log.
pub struct Audit {
    /// What its actions begin with, each with the area of the log they are listed and
    /// counted under. The log lists the rest under settings.
    pub areas: &'static [(&'static str, AuditArea)],
    /// The kinds of record its events' references name, beside core's.
    pub subjects: &'static [&'static str],
    /// Puts names to what the events the log lists name, as they are today.
    pub names: Option<fn(&Store, &mut [Value])>,
}
impl Audit {
    pub const NONE: Self = Self {
        areas: &[],
        subjects: &[],
        names: None,
    };
}

/// Work a feature runs for the platform, with nothing to answer: it logs its own failures.
pub type Upkeep = Pin<Box<dyn Future<Output = ()> + Send>>;
/// Upkeep the scheduler runs for a feature every minute, after the platform's own cleanup
/// and in the registry's order. `run` hears whether `every` has passed since it was last
/// due, for work it does less often than the rest.
pub struct Maintenance {
    pub every: Duration,
    pub run: fn(Arc<State>, bool) -> Upkeep,
}

/// A feature's page on the DSPs page: off for every DSP until the platform owner switches
/// it on.
#[derive(Clone, Copy)]
pub struct Switch {
    /// Its catalog id. Permanent: each DSP's switch is stored under it.
    pub id: &'static str,
    pub label: &'static str,
    /// The capabilities an enabled connection must provide while the page is on.
    pub requires: &'static [&'static str],
}
/// A tab of a feature's page, switched on its own inside the page.
pub struct Tab {
    /// Its catalog id. Permanent.
    pub id: &'static str,
    pub label: &'static str,
}
pub const fn tab(id: &'static str, label: &'static str) -> Tab {
    Tab { id, label }
}

/// Core's own permissions, each declared by the core part that checks it.
const CORE_PERMISSIONS: &[&Permission] = &[&crate::collectors::CONNECTIONS];

/// A permission a DSP's roles grant, as the role sheet offers it.
pub struct Permission {
    /// Permanent: roles store it and routes ask for it.
    pub id: &'static str,
    pub label: &'static str,
    /// Its place in the one order every list of permissions follows: the role sheet, API
    /// answers and audit entries.
    pub order: u16,
    /// What holding it grants as well.
    pub implies: &'static [&'static str],
    /// The default roles a DSP's roles start out holding it in.
    pub defaults: &'static [DefaultRole],
    /// The role sheet's section for a permission no page owns, such as `Team`. A page's own
    /// permissions are listed under the page.
    pub group: Option<&'static str>,
}
/// A permission in its own place in the order, implying nothing, in no default role and
/// owned by its feature's page: `perm("dvic.collect", "Collect DVIC", 41).implies(…)`.
pub const fn perm(id: &'static str, label: &'static str, order: u16) -> Permission {
    Permission {
        id,
        label,
        order,
        implies: &[],
        defaults: &[],
        group: None,
    }
}
impl Permission {
    pub const fn implies(self, implies: &'static [&'static str]) -> Self {
        Self { implies, ..self }
    }
    pub const fn defaults(self, defaults: &'static [DefaultRole]) -> Self {
        Self { defaults, ..self }
    }
    pub const fn group(self, group: &'static str) -> Self {
        Self {
            group: Some(group),
            ..self
        }
    }
}
/// The roles besides the owner's that every DSP starts with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefaultRole {
    Manager,
    Member,
}
impl DefaultRole {
    pub const ALL: [Self; 2] = [Self::Manager, Self::Member];
    /// The legacy role value it stands for.
    pub fn key(self) -> &'static str {
        match self {
            Self::Manager => "manager",
            Self::Member => "member",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Manager => "Manager",
            Self::Member => "Member",
        }
    }
}

/// One kind of data a collector reads. A job of `job_kind` collects it for one DSP, and
/// the one feature that keeps it stores what the job brings.
pub struct Collection {
    pub job_kind: &'static str,
    /// The schedule `collection` that runs it.
    pub schedule: &'static str,
    /// The error a schedule answers while its collector is not connected.
    pub unconnected: &'static str,
}

/// Everything the platform needs to know about one provider: how to reach it and read
/// what it offers. Storage, credentials, the browser and the job queue ask here instead
/// of matching on the provider; what a collection brings is a feature's `Keeper`'s.
pub trait Collector: Sync {
    /// Names its connection row, its files, its secrets and its API path.
    fn id(&self) -> &'static str;
    /// Its name as the dashboard shows it.
    fn label(&self) -> &'static str;
    /// What its connection supplies to the pages that require it (`features`).
    fn capabilities(&self) -> &'static [&'static str];
    /// What it reads, its main collection first; each is chosen by `job_kind_for`. `both`
    /// schedules run the first of every collector's.
    fn collections(&self) -> &'static [Collection];
    /// The kind of job a request queues.
    fn job_kind_for(&self, _request: &Value) -> &'static str {
        self.collections()[0].job_kind
    }
    /// Its database. Its migrations are every owner's, its own included (`migrations`).
    fn database(&self) -> Kind;
    /// What it adds to databases, starting with its own's baseline.
    fn migrations(&self) -> &'static [Migrations];
    /// Written into a new database with its schema. Must identify the storage.
    fn seed(&self, dsp: &str) -> String;
    /// The DSP setting recording that this storage was added to an existing DSP.
    /// `None` only for Paycom, whose storage every DSP was created with.
    fn marker(&self) -> Option<&'static str>;
    /// Fails closed when initialized storage lost what it must hold.
    fn verify(&self, _: &Db) -> Result<()> {
        Ok(())
    }
    /// After every collector of a DSP opened at startup.
    fn opened(&self, _: &Store, _dsp: &str) -> Result<()> {
        Ok(())
    }
    /// Writes what a new DSP's storage holds beyond `seed`, once all of its storage exists.
    fn provision(&self, _: &Store, _dsp: &str, _timezone: &str) -> Result<()> {
        Ok(())
    }
    /// Connects a development seed's DSP with fixture credentials, as its owner would.
    fn demo(&self, _: &Store, _dsp: &str) -> Result<()> {
        Ok(())
    }
    /// What a credential change removes from `state/browsers`.
    fn browser_entries(&self) -> &'static [&'static str];
    /// The hosts its browser may reach, as its own `HostPolicy`.
    fn network(&self) -> browseros::NetworkPolicy;
    fn validate_credentials(&self, value: &Value) -> Result<()>;
    /// Shown beside the connection. Never a secret.
    fn account_label<'a>(&self, _credentials: &'a Value) -> &'a str {
        ""
    }
    /// Its driver, on a browser already running under `network`. `fixture` is the
    /// origin of a local stand-in for the provider's site.
    fn driver<'a>(
        &self,
        browser: browseros::Session,
        profile: &'a Path,
        fixture: Option<&'a str>,
    ) -> Pending<'a, Box<dyn Driver>>;
    /// What a collection returns in fixture mode, where no browser runs.
    fn fixture(&self, timezone: &str, request: &Value) -> Result<Collected>;
    /// The job's message while it collects `request`.
    fn progress(&self, _request: &Value) -> &'static str;
    /// What a finished collection brought, as the job's metrics count it.
    fn counts(&self, _data: &Value) -> Counts {
        Counts::default()
    }
    /// Whether its collections bring the whole roster, so that a finished one naming
    /// no single employee may have changed who is on it.
    fn roster(&self) -> bool {
        false
    }
    /// Every error code its collections raise and branch on.
    fn codes(&self) -> &'static [Code] {
        &[]
    }
    /// Those of its codes a failed attempt is queued again after, while attempts remain.
    fn retryable(&self) -> &'static [Code] {
        &[]
    }
    /// Work a finished collection needs before it is stored, done without the database
    /// and outside the platform lock: parsing, shaping rows, compressing.
    fn prepare(&self, collected: Collected) -> Result<Collected> {
        Ok(collected)
    }
    /// Drops what an unfinished job kept to resume from. `None` means every job.
    fn discard(&self, _: &Store, _dsp: &str, _job: Option<&str>) -> Result<()> {
        Ok(())
    }
    /// Drops what it kept to resume jobs that have ended or expired. The platform's
    /// periodic cleanup runs it for every DSP.
    fn prune(&self, _: &Store, _dsp: &str) -> Result<()> {
        Ok(())
    }
    /// Runs with the connection's own disable, in its transaction.
    fn disabled(&self, _: &Db) -> Result<()> {
        Ok(())
    }
}

/// How a feature keeps one collection: what a queued request needs from the DSP before
/// it runs, how what it brings is stored, and what a schedule of it queues. The engine
/// finds it by the collection's job kind.
pub trait Keeper: Sync {
    /// The job kind of the collection it keeps.
    fn keeps(&self) -> &'static str;
    /// The permission its collection runs under: what a member needs to queue or cancel
    /// it, and what a job a member queued still needs when it runs.
    fn permission(&self) -> &'static str;
    /// What publishing its collection changes, for the read cache. Without one of its own,
    /// it evicts every read the DSP's settings do.
    fn domain(&self) -> DataDomain {
        DataDomain::TENANT
    }
    /// Adds current tenant context needed only while a queued request executes. The
    /// persisted request stays compatible with the previous binary for rollback.
    fn bind(&self, _: &Store, _dsp: &str, request: &Value) -> Result<Value> {
        Ok(request.clone())
    }
    /// Stores what it can of a finished collection before the job publishes it, in
    /// steps short enough that the platform lock is never held for long; `publish` then
    /// only makes it current. Each step checks the job is still this worker's.
    fn stage<'a>(
        &'a self,
        _state: &'a Arc<State>,
        _dsp: &'a str,
        _job: &'a str,
        _owner: &'a str,
        collected: Collected,
    ) -> Pending<'a, Collected> {
        Box::pin(async move { Ok(collected) })
    }
    /// Stores a finished collection. Runs while the job is still this worker's.
    fn publish(&self, store: &Store, dsp: &str, job: &str, collected: Collected) -> Result<()>;
    /// Answers a running collection that asks what is already kept for the DSP, so that
    /// it reads from the site only what it lacks. The question and the answer are the
    /// collection's own JSON.
    fn kept(&self, _: &Store, _dsp: &str, _question: &Value) -> Result<Value> {
        Ok(Value::Null)
    }
    /// When `date` was last collected, as a row with `collected_at`.
    fn collected_at(&self, _: &Store, _dsp: &str, _date: &str) -> Result<Option<Value>> {
        Ok(None)
    }
    /// When what it holds for the DSP was last collected, for the DSP list.
    fn last_collected(&self, _: &Store, _dsp: &str) -> Result<Option<String>> {
        Ok(None)
    }
    /// What else a schedule needs before it can run the collection.
    fn schedule_ready(&self, _: &Store, _dsp: &str) -> Result<()> {
        Ok(())
    }
    /// The jobs one scheduled run queues: an idempotency key suffix and a request each.
    fn scheduled(&self, _: &Store, _dsp: &str) -> Result<Vec<(String, Value)>> {
        Ok(vec![])
    }
    /// The databases it keeps the collection in, beside its collector's own.
    fn storages(&self) -> &'static [&'static AddedStorage] {
        &[]
    }
}

/// Core's error codes, joined by the ones each registered collector declares.
impl Code {
    /// Every code the backend branches on: core's own, then each collector's.
    pub fn all() -> impl Iterator<Item = Code> {
        let collectors = registry().collectors.iter().flat_map(|c| c.codes());
        Code::ALL.iter().chain(collectors).copied()
    }
    /// A failed attempt with one of these is queued again while attempts remain: core's
    /// own, then each collector's.
    pub fn retryable() -> impl Iterator<Item = Code> {
        let collectors = registry().collectors.iter().flat_map(|c| c.retryable());
        Code::RETRYABLE.iter().chain(collectors).copied()
    }
    pub fn parse(text: &str) -> Option<Self> {
        Self::all().find(|code| code.as_str() == text)
    }
    pub fn is_retryable(self) -> bool {
        Self::retryable().any(|code| code == self)
    }
}
impl Error {
    pub fn known(&self) -> Option<Code> {
        Code::parse(&self.code)
    }
    pub fn is_retryable(&self) -> bool {
        Code::retryable().any(|code| self.is(code))
    }
}
