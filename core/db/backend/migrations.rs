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
mod tests {
    use super::*;
    use crate::{
        config::Config, db::Store, dvic::DvicStore, routedata::RoutesStore,
        scorecard::ScorecardStore,
    };
    use std::{
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
    };

    const RECORD: &str = "CREATE TABLE schema_migrations (id INTEGER PRIMARY KEY, name TEXT NOT NULL, applied_at INTEGER NOT NULL);\n";
    const DSP_IDENTITY: &str = "CREATE TABLE storage_identity ( dsp_id TEXT PRIMARY KEY, provider TEXT NOT NULL, source TEXT NOT NULL );\n";

    fn snapshot(kind: Kind) -> PathBuf {
        // The repository root holds Cargo.lock, wherever this crate's manifest sits in it.
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find(|dir| dir.join("Cargo.lock").is_file())
            .expect("repository root")
            .join("core/db/tests/backend/schema")
            .join(format!("{}.sql", kind.name()))
    }
    // Tables, then indexes, then triggers, so a snapshot also runs as a script.
    fn dump(db: &Db) -> String {
        db.all(
            "SELECT sql FROM sqlite_master WHERE sql IS NOT NULL ORDER BY CASE type WHEN \
            'table' THEN 0 WHEN 'index' THEN 1 ELSE 2 END,name",
            [],
        )
        .unwrap()
        .iter()
        .map(|row| {
            let sql: Vec<&str> = row["sql"].as_str().unwrap().split_whitespace().collect();
            format!("{};\n", sql.join(" "))
        })
        .collect()
    }
    fn private() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        root
    }
    // What an older binary left behind: the schema without any record of migrations.
    fn older(file: &Path, kind: Kind, schema: &str) {
        let db = rusqlite::Connection::open(file).unwrap();
        db.execute_batch(schema).unwrap();
        db.pragma_update(None, "user_version", kind.version())
            .unwrap();
        std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    fn recorded(kind: Kind) -> String {
        std::fs::read_to_string(snapshot(kind)).unwrap()
    }
    fn legacy(kind: Kind) -> String {
        let schema = recorded(kind).replace(RECORD, "");
        if kind == Kind::DSP {
            schema.replace(DSP_IDENTITY, "")
        } else {
            schema
        }
    }
    fn ids(db: &Db) -> Vec<i64> {
        db.all("SELECT id FROM schema_migrations ORDER BY id", [])
            .unwrap()
            .iter()
            .map(|row| row["id"].as_i64().unwrap())
            .collect()
    }

    /// Startup and provisioning, exactly as the core runs them. Every schema change
    /// shows up in review as a change to core/db/tests/backend/schema. After adding a
    /// migration, rewrite the snapshots with
    /// `DISPATCH_UPDATE_SCHEMA=1 cargo test --locked -j 3 --lib db::migrations`.
    #[test]
    fn new_databases_match_the_recorded_schema() {
        let root = private();
        let mut config = Config::load().unwrap();
        config.root = root.path().into();
        let store = Store::initialize(config).unwrap();
        let id = crate::crypto::id("dsp").unwrap();
        store
            .platform
            .exec(
                "INSERT INTO \
            dsps(id,name,environment,status,timezone,created_at) VALUES \
            (?,'Schema','preview','provisioning','UTC',?)",
                [&id, &crate::db::iso()],
            )
            .unwrap();
        store.provision(&id).unwrap();
        let dsp = store.dsp(&id).unwrap();
        let paycom = store
            .collector(&id, crate::collectors::paycom::PROVIDER)
            .unwrap();
        let cortex = store
            .collector(&id, crate::collectors::cortex::PROVIDER)
            .unwrap();
        let scorecard = store.scorecard_db(&id).unwrap();
        let routedata = store.routes_db(&id).unwrap();
        let dvic = store.dvic_db(&id).unwrap();
        let databases: [(Kind, &Db); 8] = [
            (Kind::PLATFORM, &store.platform),
            (Kind::JOBS, &store.jobs),
            (Kind::DSP, &dsp),
            (crate::collectors::paycom::DATABASE, &paycom),
            (crate::collectors::cortex::DATABASE, &cortex),
            (crate::scorecard::DATABASE, &scorecard),
            (crate::routedata::DATABASE, &routedata),
            (crate::dvic::DATABASE, &dvic),
        ];
        for (kind, db) in databases {
            if std::env::var_os("DISPATCH_UPDATE_SCHEMA").is_some() {
                std::fs::write(snapshot(kind), dump(db)).unwrap();
            }
            assert_eq!(dump(db), recorded(kind), "{} schema", kind.name());
            let expected: Vec<i64> = kind.migrations().iter().map(|m| m.id.into()).collect();
            assert_eq!(ids(db), expected);
            // A second pass finds nothing to do.
            migrate(db, kind).unwrap();
            assert_eq!(dump(db), recorded(kind));
            assert_eq!(ids(db), expected);
        }
    }

    #[test]
    fn lists_are_numbered_from_one_without_gaps_or_repeats() {
        for kind in registry().databases() {
            for (index, migration) in kind.migrations().iter().enumerate() {
                assert_eq!(migration.id as usize, index + 1, "{}", kind.name());
                assert!(!migration.name.is_empty());
            }
        }
    }

    #[test]
    fn a_kind_gathers_its_owners_migrations_in_order() {
        let owners = [
            Migrations {
                kind: Kind::DSP,
                list: &[Migration {
                    id: 3,
                    name: "third",
                    apply: Apply::Sql(""),
                }],
            },
            Migrations {
                kind: Kind::JOBS,
                list: &[Migration {
                    id: 2,
                    name: "another_kind",
                    apply: Apply::Sql(""),
                }],
            },
            Migrations {
                kind: Kind::DSP,
                list: &[
                    Migration {
                        id: 1,
                        name: "first",
                        apply: Apply::Sql(""),
                    },
                    Migration {
                        id: 2,
                        name: "second",
                        apply: Apply::Sql(""),
                    },
                ],
            },
        ];
        let list: Vec<_> = ledger(Kind::DSP, &owners)
            .iter()
            .map(|migration| (migration.id, migration.name))
            .collect();
        assert_eq!(list, [(1, "first"), (2, "second"), (3, "third")]);
    }

    #[test]
    fn employee_history_uses_the_code_index() {
        let root = private();
        let file = root.path().join("paycom.sqlite");
        let db = Db::create(&file, crate::collectors::paycom::DATABASE, "").unwrap();
        let plan = db
            .all(
                "EXPLAIN QUERY PLAN SELECT p.id FROM publications p \
            JOIN employees e ON e.publication_id=p.id WHERE e.code='E001' \
            ORDER BY p.period_to DESC LIMIT 1",
                [],
            )
            .unwrap();
        assert!(
            plan.iter().any(|row| row["detail"]
                .as_str()
                .unwrap_or_default()
                .contains("employees_by_code")),
            "{plan:?}"
        );
        assert!(
            !plan.iter().any(|row| row["detail"]
                .as_str()
                .unwrap_or_default()
                .contains("SCAN e")),
            "{plan:?}"
        );
    }

    #[test]
    fn a_database_an_older_binary_made_ends_like_a_new_one() {
        let root = private();
        for kind in registry().databases() {
            let file = root.path().join(format!("{}.sqlite", kind.name()));
            older(&file, kind, &legacy(kind));
            let db = Db::create(&file, kind, "INSERT INTO never_run VALUES (1);").unwrap();
            assert_eq!(dump(&db), recorded(kind), "{}", kind.name());
        }
    }

    #[test]
    fn concurrent_legacy_dsp_adoption_accepts_the_identity_committed_while_waiting() {
        let root = private();
        let file = root.path().join("dsp.sqlite");
        let id = format!("dsp_{}", "1".repeat(32));
        older(&file, Kind::DSP, &legacy(Kind::DSP));
        let first = Db::open(&file, Kind::DSP).unwrap();
        let second = Db::open(&file, Kind::DSP).unwrap();
        let probed = std::sync::Barrier::new(2);
        let resume = std::sync::Barrier::new(2);

        std::thread::scope(|scope| {
            let id = &id;
            let probed = &probed;
            let resume = &resume;
            let waiting = scope.spawn(move || {
                migrate_dsp_after_probe(&second, id, || {
                    probed.wait();
                    resume.wait();
                })
            });
            probed.wait();
            migrate_dsp(&first, id).unwrap();
            resume.wait();
            waiting.join().unwrap().unwrap();
        });

        verify_dsp_identity(&first, &id).unwrap();
        assert_eq!(ids(&first), vec![1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn platform_databases_from_before_each_added_column_gain_it() {
        let root = private();
        let columns = |db: &Db, table: &str| {
            db.all(
                &format!("SELECT name,type FROM pragma_table_info('{table}') ORDER BY name"),
                [],
            )
            .unwrap()
        };
        let new = root.path().join("new.sqlite");
        let new = Db::create(&new, Kind::PLATFORM, "").unwrap();
        // v0.0.9 has no audit data or shown. Before v0.0.6 there was no actor_name,
        // and before roles no role_id or its indexes.
        let v9 = recorded(Kind::PLATFORM)
            .replace(RECORD, "")
            .replace(", data TEXT, shown INTEGER)", ")");
        let first = v9
            .replace(", actor_name TEXT)", ")")
            .replace(", role_id TEXT REFERENCES roles(id)", "")
            .replace(", role_id TEXT)", ")")
            .replace(
                "CREATE INDEX memberships_role ON memberships(role_id);\n",
                "",
            )
            .replace(
                "CREATE INDEX invitations_role ON invitations(role_id) WHERE used_at IS NULL;\n",
                "",
            );
        assert!(!first.contains("role_id") && !first.contains("actor_name"));
        for (name, schema) in [("v9", v9), ("first", first)] {
            let file = root.path().join(format!("{name}.sqlite"));
            older(&file, Kind::PLATFORM, &schema);
            rusqlite::Connection::open(&file)
                .unwrap()
                .execute("INSERT INTO audit(at,action) VALUES ('then','kept')", [])
                .unwrap();
            let db = Db::create(&file, Kind::PLATFORM, "").unwrap();
            for table in ["audit", "memberships", "invitations"] {
                assert_eq!(columns(&db, table), columns(&new, table), "{name} {table}");
            }
            assert_eq!(dump(&db), dump(&new), "{name}");
            assert_eq!(
                db.all("SELECT action,data,shown,actor_name FROM audit", [])
                    .unwrap(),
                vec![json!({"action":"kept","data":null,"shown":null,"actor_name":null})]
            );
        }
    }

    #[test]
    fn jobs_and_schedules_from_before_the_scorecard_keep_their_rows_through_the_rebuild() {
        let root = private();
        // v0.0.9 names neither the scorecard job kind nor the scorecard collection.
        let jobs_schema = recorded(Kind::JOBS)
            .replace(RECORD, "")
            .replace(",'cortex.scorecard.collect'", "");
        assert!(!jobs_schema.contains("scorecard"));
        let file = root.path().join("jobs.sqlite");
        older(&file, Kind::JOBS, &jobs_schema);
        rusqlite::Connection::open(&file)
            .unwrap()
            .execute_batch(
                "INSERT INTO jobs(id,dsp_id,environment,kind,status,available_at,created_at,\
                 release,connection_revision,idempotency_key) VALUES ('job_1','dsp_1','preview',\
                 'paycom.collect','succeeded',0,'2026-01-01T00:00:00Z','0.0.9',1,'k1'); \
                 INSERT INTO job_metrics VALUES ('job_1',1,'worker','{}');",
            )
            .unwrap();
        let db = Db::create(&file, Kind::JOBS, "").unwrap();
        assert_eq!(dump(&db), recorded(Kind::JOBS));
        assert_eq!(
            db.all("SELECT id,kind FROM jobs", []).unwrap(),
            vec![json!({"id":"job_1","kind":"paycom.collect"})]
        );
        assert_eq!(
            db.all("SELECT job_id,attempt,owner FROM job_metrics", [])
                .unwrap(),
            vec![json!({"job_id":"job_1","attempt":1,"owner":"worker"})]
        );
        assert!(db.all("PRAGMA foreign_key_check", []).unwrap().is_empty());
        db.exec(
            "INSERT INTO jobs(id,dsp_id,environment,kind,status,available_at,created_at,\
             release,connection_revision,idempotency_key) VALUES ('job_2','dsp_1','preview',\
             'cortex.scorecard.collect','queued',0,'2026-01-02T00:00:00Z','0.0.10',1,'k2')",
            [],
        )
        .unwrap();
        // Metrics still follow their job.
        db.exec("DELETE FROM jobs WHERE id='job_1'", []).unwrap();
        assert!(
            db.all("SELECT job_id FROM job_metrics", [])
                .unwrap()
                .is_empty()
        );

        let dsp_schema = legacy(Kind::DSP).replace(",'scorecard'", "");
        assert!(!dsp_schema.contains("scorecard"));
        let file = root.path().join("dsp.sqlite");
        older(&file, Kind::DSP, &dsp_schema);
        rusqlite::Connection::open(&file)
            .unwrap()
            .execute_batch(
                "INSERT INTO collection_schedules(id,name,collection,cadence,local_time,anchor,\
                 enabled,created_at) VALUES ('s1','Meals','meal_break','daily','06:00',0,1,'2026-01-01')",
            )
            .unwrap();
        let db = Db::create(&file, Kind::DSP, "").unwrap();
        assert_eq!(dump(&db), recorded(Kind::DSP));
        assert_eq!(
            db.all("SELECT id,collection,enabled FROM collection_schedules", [])
                .unwrap(),
            vec![json!({"id":"s1","collection":"meal_break","enabled":1})]
        );
        db.exec(
            "INSERT INTO collection_schedules(id,name,collection,cadence,local_time,anchor,\
             enabled,created_at) VALUES ('s2','Scorecard','scorecard','daily','07:00',0,1,'2026-01-02')",
            [],
        )
        .unwrap();
    }

    #[test]
    fn jobs_and_schedules_from_before_the_routes_collection_keep_their_rows_through_the_rebuild() {
        let root = private();
        // v0.0.12 names neither the routes job kind nor the routes collection.
        let jobs_schema = recorded(Kind::JOBS)
            .replace(RECORD, "")
            .replace(",'cortex.routes.collect'", "");
        assert!(!jobs_schema.contains("routes"));
        let file = root.path().join("jobs.sqlite");
        older(&file, Kind::JOBS, &jobs_schema);
        rusqlite::Connection::open(&file)
            .unwrap()
            .execute_batch(
                "INSERT INTO jobs(id,dsp_id,environment,kind,status,available_at,created_at,\
                 release,connection_revision,idempotency_key) VALUES ('job_1','dsp_1','preview',\
                 'cortex.scorecard.collect','succeeded',0,'2026-01-01T00:00:00Z','0.0.12',1,'k1'); \
                 INSERT INTO job_metrics VALUES ('job_1',1,'worker','{}');",
            )
            .unwrap();
        let db = Db::create(&file, Kind::JOBS, "").unwrap();
        assert_eq!(dump(&db), recorded(Kind::JOBS));
        assert_eq!(
            db.all("SELECT id,kind FROM jobs", []).unwrap(),
            vec![json!({"id":"job_1","kind":"cortex.scorecard.collect"})]
        );
        assert_eq!(
            db.all("SELECT job_id,attempt,owner FROM job_metrics", [])
                .unwrap(),
            vec![json!({"job_id":"job_1","attempt":1,"owner":"worker"})]
        );
        assert!(db.all("PRAGMA foreign_key_check", []).unwrap().is_empty());
        db.exec(
            "INSERT INTO jobs(id,dsp_id,environment,kind,status,available_at,created_at,\
             release,connection_revision,idempotency_key) VALUES ('job_2','dsp_1','preview',\
             'cortex.routes.collect','queued',0,'2026-01-02T00:00:00Z','0.0.13',1,'k2')",
            [],
        )
        .unwrap();
        // Metrics still follow their job.
        db.exec("DELETE FROM jobs WHERE id='job_1'", []).unwrap();
        assert!(
            db.all("SELECT job_id FROM job_metrics", [])
                .unwrap()
                .is_empty()
        );

        let dsp_schema = legacy(Kind::DSP).replace(",'routes'", "");
        assert!(!dsp_schema.contains("routes"));
        let file = root.path().join("dsp.sqlite");
        older(&file, Kind::DSP, &dsp_schema);
        rusqlite::Connection::open(&file)
            .unwrap()
            .execute_batch(
                "INSERT INTO collection_schedules(id,name,collection,cadence,local_time,anchor,\
                 enabled,created_at) VALUES ('s1','Scorecard','scorecard','daily','07:00',0,1,'2026-01-01')",
            )
            .unwrap();
        let db = Db::create(&file, Kind::DSP, "").unwrap();
        assert_eq!(dump(&db), recorded(Kind::DSP));
        assert_eq!(
            db.all("SELECT id,collection,enabled FROM collection_schedules", [])
                .unwrap(),
            vec![json!({"id":"s1","collection":"scorecard","enabled":1})]
        );
        db.exec(
            "INSERT INTO collection_schedules(id,name,collection,cadence,local_time,anchor,\
             enabled,created_at) VALUES ('s2','Routes','routes','daily','05:00',0,1,'2026-01-02')",
            [],
        )
        .unwrap();
    }

    #[test]
    fn jobs_and_schedules_from_before_the_dvic_collection_keep_their_rows_through_the_rebuild() {
        let root = private();
        // The previous release names neither the DVIC job kind nor its schedule collection.
        let jobs_schema = recorded(Kind::JOBS).replace(",'cortex.dvic.collect'", "");
        assert!(!jobs_schema.contains("dvic"));
        let file = root.path().join("jobs.sqlite");
        older(&file, Kind::JOBS, &jobs_schema);
        rusqlite::Connection::open(&file)
            .unwrap()
            .execute_batch(
                "INSERT INTO schema_migrations VALUES (1,'baseline',0),(2,'scorecard_kind',0),(3,'routes_kind',0); \
                 INSERT INTO jobs(id,dsp_id,environment,kind,status,available_at,created_at,\
                 release,connection_revision,idempotency_key) VALUES ('job_1','dsp_1','preview',\
                 'cortex.routes.collect','succeeded',0,'2026-01-01T00:00:00Z','0.0.12',1,'k1'); \
                 INSERT INTO job_metrics VALUES ('job_1',1,'worker','{}');",
            )
            .unwrap();
        let db = Db::create(&file, Kind::JOBS, "").unwrap();
        assert_eq!(dump(&db), recorded(Kind::JOBS));
        assert_eq!(
            db.all("SELECT id,kind FROM jobs", []).unwrap(),
            vec![json!({"id":"job_1","kind":"cortex.routes.collect"})]
        );
        assert_eq!(
            db.all("SELECT job_id,attempt,owner FROM job_metrics", [])
                .unwrap(),
            vec![json!({"job_id":"job_1","attempt":1,"owner":"worker"})]
        );
        assert!(db.all("PRAGMA foreign_key_check", []).unwrap().is_empty());
        db.exec(
            "INSERT INTO jobs(id,dsp_id,environment,kind,status,available_at,created_at,\
             release,connection_revision,idempotency_key) VALUES ('job_2','dsp_1','preview',\
             'cortex.dvic.collect','queued',0,'2026-01-02T00:00:00Z','0.0.13',1,'k2')",
            [],
        )
        .unwrap();
        // Metrics still follow their job.
        db.exec("DELETE FROM jobs WHERE id='job_1'", []).unwrap();
        assert!(
            db.all("SELECT job_id FROM job_metrics", [])
                .unwrap()
                .is_empty()
        );

        // A DSP database already bound by migration 5 migrates through the startup path,
        // which verifies its identity before and after the rebuild.
        let dsp_schema = recorded(Kind::DSP).replace(",'dvic'", "");
        assert!(!dsp_schema.contains("dvic"));
        let file = root.path().join("dsp.sqlite");
        let id = format!("dsp_{}", "2".repeat(32));
        older(&file, Kind::DSP, &dsp_schema);
        rusqlite::Connection::open(&file)
            .unwrap()
            .execute_batch(&format!(
                "INSERT INTO schema_migrations VALUES (1,'baseline',0),(2,'uniform_inventory',0),\
                 (3,'scorecard_collection',0),(4,'routes_collection',0),(5,'storage_identity',0); \
                 INSERT INTO storage_identity(dsp_id,provider,source) VALUES ('{id}','dispatch','dispatch-v1'); \
                 INSERT INTO collection_schedules(id,name,collection,cadence,local_time,anchor,\
                 enabled,created_at) VALUES ('s1','Routes','routes','daily','07:00',0,1,'2026-01-01')"
            ))
            .unwrap();
        let db = Db::open(&file, Kind::DSP).unwrap();
        migrate_dsp(&db, &id).unwrap();
        verify_dsp_identity(&db, &id).unwrap();
        assert_eq!(ids(&db), vec![1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(dump(&db), recorded(Kind::DSP));
        assert_eq!(
            db.all("SELECT id,collection,enabled FROM collection_schedules", [])
                .unwrap(),
            vec![json!({"id":"s1","collection":"routes","enabled":1})]
        );
        db.exec(
            "INSERT INTO collection_schedules(id,name,collection,cadence,local_time,anchor,\
             enabled,created_at) VALUES ('s2','DVIC','dvic','daily','05:00',0,1,'2026-01-02')",
            [],
        )
        .unwrap();
    }

    #[test]
    fn dsps_from_before_features_defaulted_off_keep_what_they_had() {
        let root = private();
        let file = root.path().join("platform.sqlite");
        older(
            &file,
            Kind::PLATFORM,
            &recorded(Kind::PLATFORM).replace(RECORD, ""),
        );
        // One DSP read every feature at its old default, on; the other had switched one off.
        rusqlite::Connection::open(&file)
            .unwrap()
            .execute_batch(
                "INSERT INTO dsps(id,name,environment,status,timezone,created_at) VALUES \
                 ('dsp_a','A','preview','active','UTC','2026-01-01'),\
                 ('dsp_b','B','preview','active','UTC','2026-01-01'); \
                 INSERT INTO dsp_features(dsp_id,feature,enabled,changed_at) VALUES \
                 ('dsp_b','uniforms',0,'2026-01-02');",
            )
            .unwrap();
        let db = Db::create(&file, Kind::PLATFORM, "").unwrap();
        assert_eq!(
            db.all(
                "SELECT dsp_id,feature,enabled FROM dsp_features ORDER BY dsp_id,feature",
                []
            )
            .unwrap(),
            vec![
                json!({"dsp_id":"dsp_a","feature":"cortex","enabled":1}),
                json!({"dsp_id":"dsp_a","feature":"paycom","enabled":1}),
                json!({"dsp_id":"dsp_a","feature":"scorecard","enabled":1}),
                json!({"dsp_id":"dsp_a","feature":"timecard","enabled":1}),
                json!({"dsp_id":"dsp_a","feature":"uniforms","enabled":1}),
                json!({"dsp_id":"dsp_b","feature":"cortex","enabled":1}),
                json!({"dsp_id":"dsp_b","feature":"paycom","enabled":1}),
                json!({"dsp_id":"dsp_b","feature":"scorecard","enabled":1}),
                json!({"dsp_id":"dsp_b","feature":"timecard","enabled":1}),
                json!({"dsp_id":"dsp_b","feature":"uniforms","enabled":0}),
            ]
        );
    }

    #[test]
    fn the_scorecard_switch_starts_on_wherever_the_timecard_was() {
        let root = private();
        let file = root.path().join("platform.sqlite");
        let db = Db::create(&file, Kind::PLATFORM, "").unwrap();
        // The release before the switch: Timecard on for one DSP, off for another and never
        // switched for a third; a role granting the timecard.
        db.0.execute_batch(
            "DELETE FROM schema_migrations WHERE id=12; \
             INSERT INTO dsps(id,name,environment,status,timezone,created_at) VALUES \
             ('dsp_on','On','preview','active','UTC','2026-01-01'),\
             ('dsp_off','Off','preview','active','UTC','2026-01-01'),\
             ('dsp_new','New','preview','active','UTC','2026-01-01'); \
             INSERT INTO dsp_features(dsp_id,feature,enabled,changed_at) VALUES \
             ('dsp_on','timecard',1,'2026-01-02'),('dsp_off','timecard',0,'2026-01-02'); \
             INSERT INTO roles(id,dsp_id,name,permissions,system,created_at) VALUES \
             ('manager','dsp_on','Manager','[\"timecard.manage\",\"collections.run\"]',0,'2026-01-02');",
        )
        .unwrap();
        migrate(&db, Kind::PLATFORM).unwrap();
        assert_eq!(
            db.all(
                "SELECT dsp_id FROM dsp_features WHERE feature='scorecard' AND enabled=1",
                []
            )
            .unwrap(),
            vec![json!({"dsp_id":"dsp_on"})]
        );
        // The scorecard's permissions owe nothing to the timecard's.
        assert_eq!(
            db.one("SELECT permissions FROM roles WHERE id='manager'", [])
                .unwrap()
                .unwrap()["permissions"],
            json!("[\"timecard.manage\",\"collections.run\"]")
        );
    }

    #[test]
    fn agent_keys_from_before_connected_apps_stay_keys() {
        let root = private();
        let new = Db::create(&root.path().join("new.sqlite"), Kind::PLATFORM, "").unwrap();
        // The release before Sign in with Dispatch: no OAuth tables, and keys of one kind
        // that read no differently.
        let before: String = recorded(Kind::PLATFORM)
            .replace(RECORD, "")
            .replace(
                ", kind TEXT NOT NULL DEFAULT 'key' CHECK(kind IN ('key','app')), client_id TEXT, \
                 client_name TEXT, client_verified INTEGER NOT NULL DEFAULT 0, areas TEXT NOT \
                 NULL DEFAULT 'routes,timecards,meal_breaks,dvic,feedback,safety,returns,\
                 scorecard', bypass INTEGER NOT NULL DEFAULT 0 CHECK(bypass IN (0,1)))",
                ")",
            )
            .lines()
            .filter(|line| !line.contains("oauth_"))
            .map(|line| format!("{line}\n"))
            .collect();
        assert!(!before.contains("client_verified"));
        let file = root.path().join("platform.sqlite");
        older(&file, Kind::PLATFORM, &before);
        rusqlite::Connection::open(&file)
            .unwrap()
            .execute_batch(
                "INSERT INTO users(id,email,first_name,last_name,password,platform_owner,\
                 created_at) VALUES ('u','owner@example.test','O','Wner','x',1,'then'); \
                 INSERT INTO agent_keys(id,name,hash,hint,user_id,all_dsps,access,tools,\
                 locations,created_at) VALUES ('k','Laptop','h','abcd','u',1,'read','full',0,'then');",
            )
            .unwrap();
        let db = Db::create(&file, Kind::PLATFORM, "").unwrap();
        assert_eq!(dump(&db), dump(&new));
        assert_eq!(
            db.all(
                "SELECT kind,client_id,client_name,client_verified FROM agent_keys",
                []
            )
            .unwrap(),
            vec![json!({"kind":"key","client_id":null,"client_name":null,"client_verified":0})]
        );
    }

    #[test]
    fn agent_keys_from_before_reads_read_every_kind_with_their_addresses() {
        let root = private();
        let new = Db::create(&root.path().join("new.sqlite"), Kind::PLATFORM, "").unwrap();
        // The release before reads: tools and addresses on each key, no DSP's own settings,
        // and calls never marked as bypassing features.
        let before: String = recorded(Kind::PLATFORM)
            .replace(RECORD, "")
            .replace(
                ", areas TEXT NOT NULL DEFAULT 'routes,timecards,meal_breaks,dvic,feedback,\
                 safety,returns,scorecard', bypass INTEGER NOT NULL DEFAULT 0 CHECK(bypass IN \
                 (0,1)))",
                ")",
            )
            .replace(" , bypassed INTEGER NOT NULL DEFAULT 0)", " )")
            .lines()
            .filter(|line| !line.contains("agent_key_dsp_reads"))
            .map(|line| format!("{line}\n"))
            .collect();
        assert!(!before.contains("bypass") && !before.contains("areas"));
        let file = root.path().join("platform.sqlite");
        older(&file, Kind::PLATFORM, &before);
        rusqlite::Connection::open(&file)
            .unwrap()
            .execute_batch(
                "INSERT INTO users(id,email,first_name,last_name,password,platform_owner,\
                 created_at) VALUES ('u','owner@example.test','O','Wner','x',1,'then'); \
                 INSERT INTO agent_keys(id,name,hash,hint,user_id,all_dsps,access,tools,\
                 locations,created_at,kind) VALUES \
                 ('essential','Essential','h1','abcd','u',1,'read','essential',0,'then','key'),\
                 ('essential_places','Essential places','h2','abcd','u',1,'read','essential',1,\
                 'then','key'),\
                 ('full','Full','h3','abcd','u',1,'operator','full',0,'then','key'),\
                 ('full_places','App','app:x','','u',0,'read','full',1,'then','app'); \
                 INSERT INTO agent_activity(at,key_id,key_name,key_kind,surface,outcome,ms,bytes) \
                 VALUES (1,'full','Full','key','rest:whoami','ok',1,1);",
            )
            .unwrap();
        let db = Db::create(&file, Kind::PLATFORM, "").unwrap();
        assert_eq!(dump(&db), dump(&new));
        let every = "routes,locations,timecards,meal_breaks,dvic,feedback,safety,returns,scorecard";
        let unplaced = "routes,timecards,meal_breaks,dvic,feedback,safety,returns,scorecard";
        assert_eq!(
            db.all("SELECT id,areas,bypass FROM agent_keys ORDER BY id", [])
                .unwrap(),
            vec![
                json!({"id":"essential","areas":unplaced,"bypass":0}),
                json!({"id":"essential_places","areas":every,"bypass":0}),
                json!({"id":"full","areas":unplaced,"bypass":0}),
                json!({"id":"full_places","areas":every,"bypass":0}),
            ]
        );
        assert_eq!(
            db.all("SELECT bypassed FROM agent_activity", []).unwrap(),
            vec![json!({"bypassed":0})]
        );
        assert_eq!(
            db.all("SELECT count(*) n FROM agent_key_dsp_reads", [])
                .unwrap(),
            vec![json!({"n":0})]
        );
    }

    #[test]
    fn migrations_a_newer_release_recorded_are_tolerated() {
        let root = private();
        let file = root.path().join("dsp.sqlite");
        let db = Db::create(&file, Kind::DSP, "").unwrap();
        db.0.execute_batch(
            "CREATE TABLE from_the_next_release (id TEXT); INSERT INTO \
            schema_migrations VALUES (9000,'from_the_next_release',1);",
        )
        .unwrap();
        drop(db);
        let db = Db::create(&file, Kind::DSP, "").unwrap();
        migrate(&db, Kind::DSP).unwrap();
        Db::open(&file, Kind::DSP).unwrap();
        let mut expected: Vec<i64> = Kind::DSP
            .migrations()
            .iter()
            .map(|m| i64::from(m.id))
            .collect();
        expected.push(9000);
        assert_eq!(ids(&db), expected);
        db.one("SELECT count(*) FROM from_the_next_release", [])
            .unwrap();
    }

    #[test]
    fn a_failing_migration_changes_nothing() {
        let root = private();
        let file = root.path().join("dsp.sqlite");
        let db = Db::create(&file, Kind::DSP, "").unwrap();
        let before = dump(&db);
        let before_ids = ids(&db);
        let next = Kind::DSP.migrations().last().unwrap().id + 1;
        let list = [
            Migration {
                id: next,
                name: "fine",
                apply: Apply::Sql("CREATE TABLE fine (id TEXT);"),
            },
            Migration {
                id: next + 1,
                name: "broken",
                apply: Apply::Sql("CREATE TABLE half (id TEXT); INSERT INTO missing VALUES (1);"),
            },
        ];
        assert_eq!(
            run(&db, "test", &list).unwrap_err().code,
            "migration_failed"
        );
        assert_eq!(dump(&db), before);
        assert_eq!(ids(&db), before_ids);
        assert!(db.0.is_autocommit());
    }

    #[test]
    fn connections_racing_to_open_first_apply_each_migration_once() {
        let root = private();
        // Neither statement can run twice.
        let next = Kind::DSP.migrations().last().unwrap().id + 1;
        let list: &[Migration] = &[Migration {
            id: next,
            name: "once",
            apply: Apply::Sql(
                "CREATE TABLE once (id INTEGER PRIMARY KEY); INSERT INTO once VALUES (1);",
            ),
        }];
        for round in 0..8 {
            let file = root.path().join(format!("race-{round}.sqlite"));
            let barrier = std::sync::Barrier::new(4);
            std::thread::scope(|scope| {
                for _ in 0..4 {
                    scope.spawn(|| {
                        barrier.wait();
                        // The seed would fail on its primary key if two connections ran it.
                        let db = Db::create(
                            &file,
                            Kind::DSP,
                            "INSERT INTO settings VALUES ('seeded','1');",
                        )
                        .unwrap();
                        run(&db, "test", list).unwrap();
                    });
                }
            });
            let db = Db::open(&file, Kind::DSP).unwrap();
            let mut expected: Vec<i64> = Kind::DSP
                .migrations()
                .iter()
                .map(|m| i64::from(m.id))
                .collect();
            expected.push(i64::from(next));
            assert_eq!(ids(&db), expected);
            assert_eq!(
                db.all("SELECT * FROM once", []).unwrap(),
                vec![json!({"id":1})]
            );
            assert_eq!(
                db.all("SELECT key FROM settings", []).unwrap(),
                vec![json!({"key":"seeded"})]
            );
        }
    }
}
