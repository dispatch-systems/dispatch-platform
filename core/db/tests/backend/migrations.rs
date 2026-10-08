use super::*;
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
// The jobs table as 0004 left it, listing every kind, before 0005 opened it.
fn before_open_kinds(schema: String) -> String {
    assert!(!schema.contains("CHECK(kind IN"));
    schema.replace(
        "kind TEXT NOT NULL, status",
        "kind TEXT NOT NULL CHECK(kind IN ('paycom.collect','cortex.meal_breaks.collect',\
         'cortex.scorecard.collect','cortex.routes.collect','cortex.dvic.collect')), status",
    )
}
// The schedules table as 0006 left it, listing every collection, before 0008 opened it.
fn before_open_collections(schema: String) -> String {
    assert!(!schema.contains("CHECK(collection IN"));
    schema.replace(
        "collection TEXT NOT NULL, cadence",
        "collection TEXT NOT NULL CHECK(collection IN ('paycom','meal_break','both','scorecard',\
         'routes','dvic')), cadence",
    )
}
fn ids(db: &Db) -> Vec<i64> {
    db.all("SELECT id FROM schema_migrations ORDER BY id", [])
        .unwrap()
        .iter()
        .map(|row| row["id"].as_i64().unwrap())
        .collect()
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
fn concurrent_legacy_dsp_adoption_accepts_the_identity_committed_while_waiting() {
    crate::testing::install(&[], &[]);
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
    assert_eq!(ids(&first), vec![1, 2, 3, 4, 5, 6, 7, 8]);
}

#[test]
fn jobs_and_schedules_from_before_the_scorecard_keep_their_rows_through_the_rebuild() {
    crate::testing::install(&[], &[]);
    let root = private();
    // v0.0.9 names neither the scorecard job kind nor the scorecard collection.
    let jobs_schema = before_open_kinds(recorded(Kind::JOBS))
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

    let dsp_schema = before_open_collections(legacy(Kind::DSP)).replace(",'scorecard'", "");
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
    crate::testing::install(&[], &[]);
    let root = private();
    // v0.0.12 names neither the routes job kind nor the routes collection.
    let jobs_schema = before_open_kinds(recorded(Kind::JOBS))
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

    let dsp_schema = before_open_collections(legacy(Kind::DSP)).replace(",'routes'", "");
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
    crate::testing::install(&[], &[]);
    let root = private();
    // The previous release names neither the DVIC job kind nor its schedule collection.
    let jobs_schema = before_open_kinds(recorded(Kind::JOBS)).replace(",'cortex.dvic.collect'", "");
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
    let dsp_schema = before_open_collections(recorded(Kind::DSP)).replace(",'dvic'", "");
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
    assert_eq!(ids(&db), vec![1, 2, 3, 4, 5, 6, 7, 8]);
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
fn jobs_from_before_kinds_were_open_keep_their_rows_through_the_rebuild() {
    crate::testing::install(&[], &[]);
    let root = private();
    // The previous release lists every kind it queues.
    let jobs_schema = before_open_kinds(recorded(Kind::JOBS));
    let file = root.path().join("jobs.sqlite");
    older(&file, Kind::JOBS, &jobs_schema);
    rusqlite::Connection::open(&file)
        .unwrap()
        .execute_batch(
            "INSERT INTO schema_migrations VALUES (1,'baseline',0),(2,'scorecard_kind',0),\
             (3,'routes_kind',0),(4,'dvic_kind',0); \
             INSERT INTO jobs(id,dsp_id,environment,kind,status,available_at,created_at,\
             release,connection_revision,idempotency_key) VALUES ('job_1','dsp_1','preview',\
             'cortex.dvic.collect','succeeded',0,'2026-01-01T00:00:00Z','0.0.13',1,'k1'); \
             INSERT INTO job_metrics VALUES ('job_1',1,'worker','{}');",
        )
        .unwrap();
    let db = Db::create(&file, Kind::JOBS, "").unwrap();
    assert_eq!(dump(&db), recorded(Kind::JOBS));
    assert_eq!(ids(&db), vec![1, 2, 3, 4, 5]);
    assert_eq!(
        db.all("SELECT id,kind FROM jobs", []).unwrap(),
        vec![json!({"id":"job_1","kind":"cortex.dvic.collect"})]
    );
    assert_eq!(
        db.all("SELECT job_id,attempt,owner FROM job_metrics", [])
            .unwrap(),
        vec![json!({"job_id":"job_1","attempt":1,"owner":"worker"})]
    );
    assert!(db.all("PRAGMA foreign_key_check", []).unwrap().is_empty());
    // The table takes a kind no release lists: queueing refuses the ones no collector
    // collects instead.
    db.exec(
        "INSERT INTO jobs(id,dsp_id,environment,kind,status,available_at,created_at,\
         release,connection_revision,idempotency_key) VALUES ('job_2','dsp_1','preview',\
         'next.collect','queued',0,'2026-01-02T00:00:00Z','0.0.14',1,'k2')",
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
}

#[test]
fn schedules_from_before_collections_were_open_keep_their_rows_through_the_rebuild() {
    crate::testing::install(&[], &[]);
    let root = private();
    // The previous release lists every collection it schedules.
    let dsp_schema = before_open_collections(recorded(Kind::DSP));
    let file = root.path().join("dsp.sqlite");
    let id = format!("dsp_{}", "3".repeat(32));
    older(&file, Kind::DSP, &dsp_schema);
    rusqlite::Connection::open(&file)
        .unwrap()
        .execute_batch(&format!(
            "INSERT INTO schema_migrations VALUES (1,'baseline',0),(2,'uniform_inventory',0),\
             (3,'scorecard_collection',0),(4,'routes_collection',0),(5,'storage_identity',0),\
             (6,'dvic_collection',0),(7,'driver_match',0); \
             INSERT INTO storage_identity(dsp_id,provider,source) VALUES ('{id}','dispatch','dispatch-v1'); \
             INSERT INTO collection_schedules(id,name,collection,cadence,local_time,anchor,\
             enabled,created_at) VALUES ('s1','DVIC','dvic','daily','05:00',0,1,'2026-01-01')"
        ))
        .unwrap();
    let db = Db::open(&file, Kind::DSP).unwrap();
    migrate_dsp(&db, &id).unwrap();
    verify_dsp_identity(&db, &id).unwrap();
    assert_eq!(ids(&db), vec![1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(dump(&db), recorded(Kind::DSP));
    assert_eq!(
        db.all("SELECT id,collection,enabled FROM collection_schedules", [])
            .unwrap(),
        vec![json!({"id":"s1","collection":"dvic","enabled":1})]
    );
    // The table takes a collection no release lists: saving a schedule refuses the ones no
    // collector schedules instead.
    db.exec(
        "INSERT INTO collection_schedules(id,name,collection,cadence,local_time,anchor,\
         enabled,created_at) VALUES ('s2','Next','next','daily','06:00',0,1,'2026-01-02')",
        [],
    )
    .unwrap();
}

#[test]
fn dsps_from_before_features_defaulted_off_keep_what_they_had() {
    crate::testing::install(&[], &[]);
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
    crate::testing::install(&[], &[]);
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
fn migrations_a_newer_release_recorded_are_tolerated() {
    crate::testing::install(&[], &[]);
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
    crate::testing::install(&[], &[]);
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
        run(&db, "test", &list, &[]).unwrap_err().code,
        "migration_failed"
    );
    assert_eq!(dump(&db), before);
    assert_eq!(ids(&db), before_ids);
    assert!(db.0.is_autocommit());
}

#[test]
fn connections_racing_to_open_first_apply_each_migration_once() {
    crate::testing::install(&[], &[]);
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
                    run(&db, "test", list, &[]).unwrap();
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

// Two features built at once each number their own from 1: neither takes the other's
// number, and each is recorded under its name, apart from the database's own list, which a
// release that knows none of them still finds complete.
#[test]
fn features_number_their_own_migrations_and_each_is_recorded_under_its_name() {
    crate::testing::install(&[], &[]);
    let root = private();
    let db = Db::create(&root.path().join("owned.sqlite"), Kind::DSP, "").unwrap();
    let shared = ids(&db);
    let owned = [
        (
            "alpha",
            Migration {
                id: 1,
                name: "alpha_notes",
                apply: Apply::Sql("CREATE TABLE alpha_notes (id TEXT);"),
            },
        ),
        (
            "beta",
            Migration {
                id: 1,
                name: "beta_notes",
                apply: Apply::Sql("CREATE TABLE beta_notes (id TEXT);"),
            },
        ),
    ];
    run(&db, "dsp", &Kind::DSP.migrations(), &owned).unwrap();
    let recorded = |db: &Db| -> Vec<(String, i64, String)> {
        db.0.prepare("SELECT owner,id,name FROM owner_migrations ORDER BY owner")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    };
    assert_eq!(
        recorded(&db),
        [
            ("alpha".to_owned(), 1, "alpha_notes".to_owned()),
            ("beta".to_owned(), 1, "beta_notes".to_owned()),
        ]
    );
    assert_eq!(ids(&db), shared);
    // Running again applies nothing, and a feature this binary doesn't know is a rollback.
    db.0.execute(
        "INSERT INTO owner_migrations(owner,id,name,applied_at) VALUES ('gamma',1,'later',0)",
        [],
    )
    .unwrap();
    run(&db, "dsp", &Kind::DSP.migrations(), &owned).unwrap();
    assert_eq!(recorded(&db).len(), 3);
}

#[test]
#[should_panic(expected = "dsp migration 2 of alpha is missing")]
fn a_feature_whose_own_migrations_skip_a_number_is_refused() {
    static LIST: &[Migration] = &[Migration {
        id: 3,
        name: "late",
        apply: Apply::Sql(""),
    }];
    static FIRST: &[Migration] = &[Migration {
        id: 1,
        name: "first",
        apply: Apply::Sql(""),
    }];
    let owned = [
        OwnMigrations {
            kind: Kind::DSP,
            list: FIRST,
        },
        OwnMigrations {
            kind: Kind::DSP,
            list: LIST,
        },
    ];
    owned_ledger(Kind::DSP, owned.iter().map(|each| ("alpha", each)));
}
