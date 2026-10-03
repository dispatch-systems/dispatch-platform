//! Every database kind has one ordered list of numbered migrations, gathered from the
//! owners that declare them: core's in `schema`, each collector's and feature's in its
//! manifest. Adding a table or a column is one new migration after the last.
//!
//! Contract for migration authors: additive only. New tables, new nullable or
//! defaulted columns, and new indexes. The previous release must keep working on
//! a database this release has migrated, so never drop, rename or rewrite what
//! it reads. Anything else takes two releases: the first stops using it and ships,
//! and only the next, whose rollback target no longer needs it, removes it. The one
//! rewrite allowed is widening a `CHECK` list by rebuilding the table under the same
//! name with the same columns (`docs/database.md`, Rebuilding a table): the previous
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
use crate::{Error, Result, manifest::registry, observability};
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
    pub(crate) fn version(self) -> i64 {
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

fn pending<'a>(done: &BTreeSet<u32>, list: &'a [Migration]) -> Vec<&'a Migration> {
    list.iter().filter(|m| !done.contains(&m.id)).collect()
}

/// Applies what is pending. The caller holds the write transaction, so a failure
/// leaves the database exactly as it was.
pub(super) fn apply(db: &Db, kind: &str, list: &[Migration]) -> Result<()> {
    db.0.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (id INTEGER PRIMARY \
        KEY, name TEXT NOT NULL, applied_at INTEGER NOT NULL)",
    )?;
    // Read again under the write lock: another connection may have just finished.
    for migration in pending(&applied(db)?, list) {
        let result = match migration.apply {
            Apply::Sql(sql) => db.0.execute_batch(sql).map_err(Error::from),
            Apply::Code(code) => code(db),
        };
        if result.is_err() {
            observability::event(
                "error",
                "storage.migration_failed",
                json!({"kind":kind,"id":migration.id,"name":migration.name}),
            );
            return Err(Error::new("migration_failed", 503));
        }
        db.0.execute(
            "INSERT INTO schema_migrations(id,name,applied_at) VALUES (?,?,?)",
            rusqlite::params![migration.id, migration.name, now()],
        )?;
        observability::event(
            "info",
            "storage.migration_applied",
            json!({"kind":kind,"id":migration.id,"name":migration.name}),
        );
    }
    Ok(())
}

pub(super) fn immediate(db: &Db) -> Result<Transaction<'_>> {
    Ok(Transaction::new_unchecked(
        &db.0,
        TransactionBehavior::Immediate,
    )?)
}

pub(super) fn run(db: &Db, kind: &str, list: &[Migration]) -> Result<()> {
    // Startup checks every database and nearly always finds nothing to do, so look
    // without taking the write lock first.
    let done = applied(db)?;
    let newer: Vec<u32> = done
        .iter()
        .copied()
        .filter(|id| list.iter().all(|m| m.id != *id))
        .collect();
    if !newer.is_empty() {
        observability::event(
            "warn",
            "storage.migrations_newer",
            json!({"kind":kind,"ids":newer}),
        );
    }
    if pending(&done, list).is_empty() {
        return Ok(());
    }
    let tx = immediate(db)?;
    apply(db, kind, list)?;
    tx.commit()?;
    Ok(())
}

/// Brings an open database up to this binary's list for its kind. Runs where
/// initialization always has: startup, the operator commands and provisioning.
/// Requests open databases without it.
pub fn migrate(db: &Db, kind: Kind) -> Result<()> {
    run(db, kind.name(), &kind.migrations())
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
    let done = applied(db)?;
    if done.contains(&5) {
        verify_dsp_identity(db, id)?;
        return run(db, Kind::DSP.name(), list);
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
    apply(db, Kind::DSP.name(), list)?;
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
