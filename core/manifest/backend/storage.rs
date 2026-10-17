//! The storage slots: the databases an owner adds to (`migrations`, `own_migrations`, and
//! `retired_migrations` for one it no longer opens) and the tables it keeps (`tables`), which
//! no other owner's code runs SQL on.
use crate::{
    db::{self, Kind, Migration, Migrations},
    manifest::Registry,
};

/// The tables an owner keeps, by the database that holds them: `&[("paycom",
/// &["employees", …])]`. Only the owner's code runs SQL on them. Core alone also names the
/// database `"*"`: every database that holds the table, beside the tables of their owners.
pub type Tables = &'static [(&'static str, &'static [&'static str])];

impl Registry {
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
    /// What every owner adds to the databases: core's parts, the agents' piece, each
    /// collector, each feature.
    fn migration_lists(&self) -> impl Iterator<Item = &'static Migrations> {
        let agents = self.agents.into_iter().flat_map(|agents| agents.migrations);
        let collectors = self.collectors.iter().flat_map(|c| c.migrations());
        let features = self
            .features
            .iter()
            .flat_map(|feature| feature.migrations.iter().chain(feature.retired_migrations));
        db::CORE_MIGRATIONS
            .iter()
            .chain(agents)
            .chain(collectors)
            .chain(features)
    }
    /// One kind of database's migrations, gathered from every owner, in order.
    pub fn migrations(&self, kind: Kind) -> Vec<Migration> {
        db::migrations::ledger(kind, self.migration_lists())
    }
    /// What each feature numbers itself for one kind of database, with the feature each is
    /// recorded under, in the registry's order.
    pub fn owned_migrations(&self, kind: Kind) -> Vec<(&'static str, Migration)> {
        let owners = self.features.iter().flat_map(|feature| {
            feature
                .own_migrations
                .iter()
                .map(move |owned| (feature.name, owned))
        });
        db::owned_ledger(kind, owners)
    }
    /// Every table an owner declares, as its owner, database and name: core's, the agents'
    /// piece's, then each collector's, then each feature's. Core's owner is `core` and the
    /// agents' piece's `agents`; a database `*` is every database that holds the table.
    pub fn tables(&self) -> Vec<(&'static str, &'static str, &'static str)> {
        let owned = |owner: &'static str, tables: Tables| {
            tables.iter().flat_map(move |(database, names)| {
                names.iter().map(move |table| (owner, *database, *table))
            })
        };
        let collectors = self
            .collectors
            .iter()
            .flat_map(|c| owned(c.id(), c.tables()));
        let features = self.features.iter().flat_map(|f| owned(f.name, f.tables));
        let agents = self
            .agents
            .into_iter()
            .flat_map(|a| owned("agents", a.tables));
        owned("core", db::CORE_TABLES)
            .chain(agents)
            .chain(collectors)
            .chain(features)
            .collect()
    }
}

/// Panics unless every database is declared once, with migrations numbered from 1 without a gap
/// or a repeat, a retired database is neither active nor declared twice, every list of
/// migrations names a database as declared, and every table is declared once.
pub(crate) fn check(registry: &Registry) {
    let databases: Vec<Kind> = registry.databases().collect();
    for (index, kind) in databases.iter().enumerate() {
        assert!(
            databases[..index]
                .iter()
                .all(|other| other.name() != kind.name()),
            "the {} database is declared twice",
            kind.name()
        );
        registry.migrations(*kind);
        registry.owned_migrations(*kind);
    }
    let retired: Vec<Kind> = registry
        .features
        .iter()
        .flat_map(|feature| {
            feature
                .retired_migrations
                .iter()
                .map(|migrations| migrations.kind)
        })
        .collect();
    for (index, kind) in retired.iter().enumerate() {
        assert!(
            !databases.contains(kind) && !retired[..index].contains(kind),
            "retired database is active or declared twice"
        );
        registry.migrations(*kind);
    }
    for owned in registry.migration_lists() {
        assert!(
            databases.contains(&owned.kind) || retired.contains(&owned.kind),
            "migrations name a {} database that is not declared, or not as declared",
            owned.kind.name()
        );
    }
    for feature in registry.features {
        for owned in feature.own_migrations {
            assert!(
                databases.contains(&owned.kind),
                "{}'s own migrations name a {} database that is not declared, or not as \
                 declared",
                feature.name,
                owned.kind.name()
            );
        }
    }
    let tables = registry.tables();
    for (index, (owner, database, table)) in tables.iter().enumerate() {
        assert!(
            *database != "*" || *owner == "core",
            "{owner} declares {table} in every database, as only core may"
        );
        for (other, elsewhere, _) in tables[..index].iter().filter(|(_, _, t)| t == table) {
            assert!(
                database != elsewhere && *database != "*" && *elsewhere != "*",
                "{database}/{table} is declared by {other} and {owner}"
            );
        }
    }
}
