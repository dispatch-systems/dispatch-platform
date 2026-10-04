//! Every registered owner's databases, as new ones and as older binaries left them, against
//! the recorded schema.
use dispatch_core::{
    db::{Db, Kind, Store, migrate},
    foundation::config::Config,
    manifest::registry,
};
use dispatch_dvic::DvicStore;
use dispatch_routes::RoutesStore;
use dispatch_scorecard::ScorecardStore;
use serde_json::json;
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
/// `DISPATCH_UPDATE_SCHEMA=1 cargo test --locked -j 3 --lib schema::`.
#[cfg(feature = "default")]
#[test]
fn new_databases_match_the_recorded_schema() {
    crate::install();
    let root = private();
    let mut config = Config::load().unwrap();
    config.root = root.path().into();
    let store = Store::initialize(config).unwrap();
    let id = dispatch_core::foundation::crypto::id("dsp").unwrap();
    store
        .platform
        .exec(
            "INSERT INTO \
        dsps(id,name,environment,status,timezone,created_at) VALUES \
        (?,'Schema','preview','provisioning','UTC',?)",
            [&id, &dispatch_core::db::iso()],
        )
        .unwrap();
    store.provision(&id).unwrap();
    let dsp = store.dsp(&id).unwrap();
    let paycom = store.collector(&id, dispatch_paycom::PROVIDER).unwrap();
    let cortex = store.collector(&id, dispatch_cortex::PROVIDER).unwrap();
    let scorecard = store.scorecard_db(&id).unwrap();
    let routedata = store.routes_db(&id).unwrap();
    let dvic = store.dvic_db(&id).unwrap();
    let databases: [(Kind, &Db); 8] = [
        (Kind::PLATFORM, &store.platform),
        (Kind::JOBS, &store.jobs),
        (Kind::DSP, &dsp),
        (dispatch_paycom::DATABASE, &paycom),
        (dispatch_cortex::DATABASE, &cortex),
        (dispatch_scorecard::DATABASE, &scorecard),
        (dispatch_routes::DATABASE, &routedata),
        (dispatch_dvic::DATABASE, &dvic),
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
    crate::install();
    for kind in registry().databases() {
        for (index, migration) in kind.migrations().iter().enumerate() {
            assert_eq!(migration.id as usize, index + 1, "{}", kind.name());
            assert!(!migration.name.is_empty());
        }
    }
}

#[test]
fn a_database_an_older_binary_made_ends_like_a_new_one() {
    crate::install();
    let root = private();
    for kind in registry().databases() {
        let file = root.path().join(format!("{}.sqlite", kind.name()));
        older(&file, kind, &legacy(kind));
        let db = Db::create(&file, kind, "INSERT INTO never_run VALUES (1);").unwrap();
        assert_eq!(dump(&db), recorded(kind), "{}", kind.name());
    }
}

#[cfg(feature = "driver_match")]
#[test]
fn platform_databases_from_before_each_added_column_gain_it() {
    crate::install();
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

#[cfg(feature = "driver_match")]
#[test]
fn agent_keys_from_before_connected_apps_stay_keys() {
    crate::install();
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

#[cfg(feature = "driver_match")]
#[test]
fn agent_keys_from_before_reads_read_every_kind_with_their_addresses() {
    crate::install();
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
