//! Every database kind has one ordered list of numbered migrations, gathered from the
//! owners that declare them: core's in `schema`, each collector's and feature's in its
//! manifest. Adding a table or a column is one new migration after the last. A feature
//! adding to a database other owners add to as well (each DSP's, the platform's, a
//! collector's) numbers its own instead, from 1 (`OwnMigrations`): they run after the
//! database's own list and are recorded under the feature's name, in `owner_migrations`, so
//! features built at once never take each other's numbers. The shared list keeps every
//! migration features declared there before, recorded as it always was.
//!
//! Contract for migration authors: additive only. New tables, new nullable or
//! defaulted columns, and new indexes. The previous release must keep working on
//! a database this release has migrated, so never drop, rename or rewrite what
//! it reads. Anything else takes two releases: the first stops using it and ships,
//! and only the next, whose rollback target no longer needs it, removes it. The one
//! rewrite allowed is widening a `CHECK` list by rebuilding the table under the same
//! name with the same columns, as `jobs/0002_scorecard_kind.sql` does: the previous
//! release reads it unchanged, and the release that rebuilds never writes the new
//! values. Never edit or renumber a migration that has shipped; append a new one.
//!
//! A database may record ids this binary does not know. That is a rollback: the
//! next release added a migration and this release was started again on its data.
//! Rollback must work one release back, so those ids are logged and tolerated,
//! never refused. They are safe to ignore because migrations are additive.
//! `PRAGMA user_version` stays pinned per kind for the same reason: older
//! binaries refuse any other value.
use super::{Db, now};
use crate::{Error, Result, foundation::observability, manifest::registry};
use rusqlite::{Transaction, TransactionBehavior};
use serde_json::json;
use std::collections::BTreeSet;

#[derive(Clone, Copy)]
pub enum Apply {
    Sql(&'static str),
    /// For steps that must look before they change anything, such as a column an
    /// older binary may already have added. Runs inside the migration transaction,
    /// so it must not begin one of its own.
    Code(fn(&Db) -> Result<()>),
}
#[derive(Clone, Copy)]
pub struct Migration {
    pub id: u32,
    pub name: &'static str,
    pub apply: Apply,
}
/// What one owner adds to one kind of database. A kind's migrations may come from
/// several owners; their ids together run from 1 without a gap.
pub struct Migrations {
    pub kind: Kind,
    pub list: &'static [Migration],
}
/// What a feature adds to a database other owners add to as well, numbered by the feature
/// from 1 without a gap and recorded under its name.
pub struct OwnMigrations {
    pub kind: Kind,
    pub list: &'static [Migration],
}

/// A kind of database, declared by the owner that creates its files: core the platform's,
/// the environment's jobs and each DSP's own; a collector its provider's; a feature one it
/// keeps whole.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Kind {
    name: &'static str,
    version: i64,
    cache_kib: i64,
}
impl Kind {
    pub const PLATFORM: Self = Self::new("platform", 3);
    pub const JOBS: Self = Self::new("jobs", 1);
    pub const DSP: Self = Self::new("dsp", 1);
    /// `name` names it in the migration log and its schema snapshot; `version` is its
    /// pinned `user_version`. Each connection caches 512 KiB of its pages.
    pub const fn new(name: &'static str, version: i64) -> Self {
        Self {
            name,
            version,
            cache_kib: 512,
        }
    }
    /// For a database whose reads need more of its pages in memory.
    pub const fn cache(self, cache_kib: i64) -> Self {
        Self { cache_kib, ..self }
    }
    pub fn name(self) -> &'static str {
        self.name
    }
    /// Pinned. Released binaries refuse to open a database with any other value.
    pub fn version(self) -> i64 {
        self.version
    }
    /// What each connection caches of its pages, in kibibytes.
    pub(crate) fn cache_kib(self) -> i64 {
        self.cache_kib
    }
    /// Its migrations, from every owner that declares some, in order.
    pub fn migrations(self) -> Vec<Migration> {
        registry().migrations(self)
    }
    /// The migrations features number for it themselves, with the feature each is recorded
    /// under, in the registry's order.
    pub fn owned_migrations(self) -> Vec<(&'static str, Migration)> {
        registry().owned_migrations(self)
    }
}

/// `kind`'s migrations, gathered from every owner's `Migrations` and ordered by id. Their
/// ids must run from 1 without a gap or a repeat, or this panics: an id is a migration's
/// place in every database's record for good, whoever declares it.
pub fn ledger<'a>(kind: Kind, owners: impl IntoIterator<Item = &'a Migrations>) -> Vec<Migration> {
    let mut list: Vec<Migration> = owners
        .into_iter()
        .filter(|owned| owned.kind.name == kind.name)
        .flat_map(|owned| owned.list.iter().copied())
        .collect();
    list.sort_by_key(|migration| migration.id);
    for (index, migration) in list.iter().enumerate() {
        let expected = index as u32 + 1;
        assert!(
            migration.id >= expected,
            "{} migration {} is declared twice",
            kind.name,
            migration.id
        );
        assert!(
            migration.id == expected,
            "{} migration {expected} is missing",
            kind.name
        );
    }
    list
}

/// `kind`'s migrations each feature numbers itself, in the order the features come: each
/// feature's must run from 1 without a gap or a repeat, or this panics.
pub fn owned_ledger<'a>(
    kind: Kind,
    owners: impl IntoIterator<Item = (&'static str, &'a OwnMigrations)>,
) -> Vec<(&'static str, Migration)> {
    let mut gathered: Vec<(&'static str, Vec<Migration>)> = Vec::new();
    for (owner, owned) in owners {
        if owned.kind.name != kind.name {
            continue;
        }
        match gathered.iter_mut().find(|(each, _)| *each == owner) {
            Some((_, list)) => list.extend(owned.list.iter().copied()),
            None => gathered.push((owner, owned.list.to_vec())),
        }
    }
    let mut ledger = Vec::new();
    for (owner, mut list) in gathered {
        list.sort_by_key(|migration| migration.id);
        for (index, migration) in list.iter().enumerate() {
            let expected = index as u32 + 1;
            assert!(
                migration.id >= expected,
                "{} migration {} of {owner} is declared twice",
                kind.name,
                migration.id
            );
            assert!(
                migration.id == expected,
                "{} migration {expected} of {owner} is missing",
                kind.name
            );
        }
        ledger.extend(list.into_iter().map(|migration| (owner, migration)));
    }
    ledger
}

/// Adds a column unless an older binary's startup already did.
pub fn add_column(db: &Db, table: &str, column: &str, definition: &str) -> Result<()> {
    let found = db.0.query_row(
        "SELECT count(*) FROM pragma_table_info(?) WHERE name=?",
        [table, column],
        |row| row.get::<_, i64>(0),
    )?;
    if found == 0 {
        db.0.execute_batch(&format!(
            "ALTER TABLE {table} ADD COLUMN {column} {definition}"
        ))?;
    }
    Ok(())
}

fn applied(db: &Db) -> Result<BTreeSet<u32>> {
    let exists = db.0.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='schema_migrations'",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    if exists == 0 {
        return Ok(BTreeSet::new());
    }
    let mut statement = db.0.prepare("SELECT id FROM schema_migrations")?;
    let ids = statement
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(ids)
}

/// The migrations features numbered themselves that the database records, by feature.
fn applied_owned(db: &Db) -> Result<BTreeSet<(String, u32)>> {
    let exists = db.0.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='owner_migrations'",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    if exists == 0 {
        return Ok(BTreeSet::new());
    }
    let mut statement = db.0.prepare("SELECT owner,id FROM owner_migrations")?;
    let ids = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(ids)
}

fn pending<'a>(done: &BTreeSet<u32>, list: &'a [Migration]) -> Vec<&'a Migration> {
    list.iter().filter(|m| !done.contains(&m.id)).collect()
}
fn pending_owned<'a>(
    done: &BTreeSet<(String, u32)>,
    owned: &'a [(&'static str, Migration)],
) -> Vec<&'a (&'static str, Migration)> {
    owned
        .iter()
        .filter(|(owner, m)| !done.contains(&((*owner).to_owned(), m.id)))
        .collect()
}

/// Runs one migration, or says which failed.
fn step(db: &Db, kind: &str, owner: Option<&str>, migration: &Migration) -> Result<()> {
    let result = match migration.apply {
        Apply::Sql(sql) => db.0.execute_batch(sql).map_err(Error::from),
        Apply::Code(code) => code(db),
    };
    let mut about = json!({"kind":kind,"id":migration.id,"name":migration.name});
    if let Some(owner) = owner {
        about["owner"] = json!(owner);
    }
    if result.is_err() {
        observability::event("error", "storage.migration_failed", about);
        return Err(Error::new("migration_failed", 503));
    }
    observability::event("info", "storage.migration_applied", about);
    Ok(())
}

/// Applies what is pending: the kind's own list, then what features numbered themselves.
/// The caller holds the write transaction, so a failure leaves the database exactly as it
/// was.
pub(super) fn apply(
    db: &Db,
    kind: &str,
    list: &[Migration],
    owned: &[(&'static str, Migration)],
) -> Result<()> {
    db.0.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (id INTEGER PRIMARY \
        KEY, name TEXT NOT NULL, applied_at INTEGER NOT NULL)",
    )?;
    // Read again under the write lock: another connection may have just finished.
    for migration in pending(&applied(db)?, list) {
        step(db, kind, None, migration)?;
        db.0.execute(
            "INSERT INTO schema_migrations(id,name,applied_at) VALUES (?,?,?)",
            rusqlite::params![migration.id, migration.name, now()],
        )?;
    }
    if owned.is_empty() {
        return Ok(());
    }
    db.0.execute_batch(
        "CREATE TABLE IF NOT EXISTS owner_migrations (owner TEXT NOT NULL, id INTEGER NOT \
        NULL, name TEXT NOT NULL, applied_at INTEGER NOT NULL, PRIMARY KEY(owner, id))",
    )?;
    for (owner, migration) in pending_owned(&applied_owned(db)?, owned) {
        step(db, kind, Some(owner), migration)?;
        db.0.execute(
            "INSERT INTO owner_migrations(owner,id,name,applied_at) VALUES (?,?,?,?)",
            rusqlite::params![owner, migration.id, migration.name, now()],
        )?;
    }
    Ok(())
}

pub(super) fn immediate(db: &Db) -> Result<Transaction<'_>> {
    Ok(Transaction::new_unchecked(
        &db.0,
        TransactionBehavior::Immediate,
    )?)
}

pub(super) fn run(
    db: &Db,
    kind: &str,
    list: &[Migration],
    owned: &[(&'static str, Migration)],
) -> Result<()> {
    // Startup checks every database and nearly always finds nothing to do, so look
    // without taking the write lock first.
    let done = applied(db)?;
    let done_owned = applied_owned(db)?;
    let newer: Vec<u32> = done
        .iter()
        .copied()
        .filter(|id| list.iter().all(|m| m.id != *id))
        .collect();
    let newer_owned: Vec<String> = done_owned
        .iter()
        .filter(|(owner, id)| owned.iter().all(|(o, m)| o != owner || m.id != *id))
        .map(|(owner, id)| format!("{owner}:{id}"))
        .collect();
    if !newer.is_empty() || !newer_owned.is_empty() {
        observability::event(
            "warn",
            "storage.migrations_newer",
            json!({"kind":kind,"ids":newer,"owned":newer_owned}),
        );
    }
    if pending(&done, list).is_empty() && pending_owned(&done_owned, owned).is_empty() {
        return Ok(());
    }
    let tx = immediate(db)?;
    apply(db, kind, list, owned)?;
    tx.commit()?;
    Ok(())
}

/// Brings an open database up to this binary's list for its kind. Runs where
/// initialization always has: startup, the operator commands and provisioning.
/// Requests open databases without it.
pub fn migrate(db: &Db, kind: Kind) -> Result<()> {
    run(
        db,
        kind.name(),
        &kind.migrations(),
        &kind.owned_migrations(),
    )
}

/// Verifies that core DSP storage belongs to the tenant whose path selected it.
/// Unlike provider storage, legacy core databases need one explicit adoption
/// while migration 5 is first applied.
pub(crate) fn verify_dsp_identity(db: &Db, id: &str) -> Result<()> {
    crate::ensure(
        db.count(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='storage_identity'",
            [],
        )? == 1,
        "dsp_storage_identity_mismatch",
        503,
    )?;
    let rows = db.all("SELECT dsp_id,provider,source FROM storage_identity", [])?;
    crate::ensure(
        rows.len() == 1
            && super::s(&rows[0], "dsp_id") == id
            && super::s(&rows[0], "provider") == "dispatch"
            && super::s(&rows[0], "source") == "dispatch-v1",
        "dsp_storage_identity_mismatch",
        503,
    )
}

/// Migrates an existing core DSP database and binds a legacy file exactly once.
/// The identity row and migration record commit together, so a database that has
/// recorded migration 5 but later loses or changes its identity is never rebound.
pub(crate) fn migrate_dsp(db: &Db, id: &str) -> Result<()> {
    migrate_dsp_after_probe(db, id, || {})
}

fn migrate_dsp_after_probe<F: FnOnce()>(db: &Db, id: &str, after_probe: F) -> Result<()> {
    let list = &Kind::DSP.migrations();
    let owned = &Kind::DSP.owned_migrations();
    let done = applied(db)?;
    if done.contains(&5) {
        verify_dsp_identity(db, id)?;
        return run(db, Kind::DSP.name(), list, owned);
    }
    after_probe();
    let tx = immediate(db)?;
    let done = applied(db)?;
    let bind = !done.contains(&5);
    if bind {
        crate::ensure(
            db.count(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='storage_identity'",
                [],
            )? == 0,
            "dsp_storage_identity_mismatch",
            503,
        )?;
    }
    apply(db, Kind::DSP.name(), list, owned)?;
    if bind {
        db.0.execute(
            "INSERT INTO storage_identity(dsp_id,provider,source) VALUES (?,'dispatch','dispatch-v1')",
            [id],
        )?;
    }
    verify_dsp_identity(db, id)?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
#[path = "../tests/backend/migrations.rs"]
mod tests;
