//! Startup retires old names while preserving data and restrictions; current readers reject them.
use dispatch_core::manifest::Collector;
use dispatch_core::{
    accounts::Auth,
    db::{Store, s},
    mcp::{api::types::AgentKeyRequest, data::catalog},
    testing as common,
};
use dispatch_cortex::{self as cortex, weekly_scorecard};
use dispatch_weekly_scorecard::WeeklyScorecardStore;
use serde_json::{Value, json};
fn ready() -> (tempfile::TempDir, Store, String) {
    dispatch_backend::install();
    let (root, db, dsp) = common::bootstrapped();
    db.enable_all_features(&dsp).unwrap();
    common::set_dsp(&db, &dsp, "Fixture Delivery", "America/Los_Angeles").unwrap();
    db.set_profile(
        &dsp,
        json!({"stationCode":"TST1","abbreviation":"FXTR","setupRequired":false}),
    )
    .unwrap();
    common::ready_connection(&db, &dsp, cortex::PROVIDER).unwrap();
    (root, db, dsp)
}
fn restart(db: Store) -> Store {
    let config = db.config.clone();
    drop(db);
    Store::initialize(config).unwrap()
}
fn owner(db: &Store, dsp: &str) -> dispatch_core::accounts::Context {
    let actor = common::platform_owner(db);
    let auth = Auth {
        user: db
            .platform
            .one_as("SELECT * FROM users WHERE id=?", [&actor])
            .unwrap()
            .unwrap(),
        hash: String::new(),
        csrf: String::new(),
        raw: String::new(),
        preview: None,
    };
    db.context(&auth, dsp, "access").unwrap()
}
#[test]
fn a_disabled_switch_is_migrated_once_and_old_input_is_rejected() {
    let (_root, db, dsp) = ready();
    db.platform.exec("UPDATE dsp_features SET feature='scorecard',enabled=0,changed_at='2026-01-01' WHERE dsp_id=? AND feature='weekly_scorecard'",[&dsp]).unwrap();
    let db = restart(db);
    assert!(!db.feature_enabled(&dsp, "weekly_scorecard").unwrap());
    let report = db.feature_report(&dsp).unwrap();
    let state = report
        .features
        .iter()
        .find(|feature| feature.feature == "weekly_scorecard")
        .unwrap();
    assert_eq!(state.changed_at.as_deref(), Some("2026-01-01"));
    assert!(
        db.set_feature(&dsp, "scorecard", true, &common::platform_owner(&db))
            .is_err()
    );
    assert_eq!(
        db.platform
            .count(
                "SELECT count(*) FROM dsp_features WHERE feature='scorecard'",
                []
            )
            .unwrap(),
        0
    );
    assert!(
        !restart(db)
            .feature_enabled(&dsp, "weekly_scorecard")
            .unwrap()
    );
}
#[test]
fn restricted_permissions_migrate_without_granting_collection_or_daily_access() {
    let (_root, db, dsp) = ready();
    db.platform.exec("INSERT INTO roles(id,dsp_id,name,permissions,system,created_at) VALUES ('old-role',?,'Weekly reader','[\"scorecard.view\"]',0,'2026-01-01')",[&dsp]).unwrap();
    let db = restart(db);
    let role = db.role(&dsp, "old-role").unwrap();
    assert_eq!(role.permissions, ["weekly_scorecard.view"]);
    let context = owner(&db, &dsp);
    let mut auth = context.auth.clone();
    auth.preview = Some(role.id.clone());
    let limited = db.context(&auth, &dsp, "weekly_scorecard.view").unwrap();
    for permission in [
        "weekly_scorecard.collect",
        "weekly_scorecard.manage",
        "daily_performance.view",
    ] {
        assert!(!limited.can(permission));
    }
    assert!(
        db.create_role(&context, "Old input", &["scorecard.collect".into()])
            .is_err()
    );
    let saved = db
        .platform
        .one("SELECT permissions FROM roles WHERE id='old-role'", [])
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(s(&saved, "permissions")).unwrap(),
        json!(["weekly_scorecard.view"])
    );
}
#[test]
fn key_and_per_dsp_allowances_migrate_and_keep_their_exact_scope() {
    let (_root, db, dsp) = ready();
    let input = AgentKeyRequest::parse(
        &json!({"name":"Weekly reader","allDsps":false,"dsps":[dsp],"access":"read",
        "reads":{"areas":["feedback","weekly_scorecard"],"bypass":false},
        "dspReads":[{"dsp":dsp,"areas":["weekly_scorecard"],"bypass":false}],"expiresAt":null}),
    )
    .unwrap();
    let created = db
        .create_agent_key(&common::platform_owner(&db), &input)
        .unwrap();
    db.platform
        .exec(
            "UPDATE agent_keys SET areas='feedback,scorecard' WHERE id=?",
            [&created.key.id],
        )
        .unwrap();
    db.platform
        .exec(
            "UPDATE agent_key_dsp_reads SET areas='scorecard' WHERE key_id=?",
            [&created.key.id],
        )
        .unwrap();
    let db = restart(db);
    let saved = db
        .agent_keys(&std::collections::HashMap::new())
        .unwrap()
        .keys
        .into_iter()
        .find(|key| key.id == created.key.id)
        .unwrap();
    assert_eq!(
        serde_json::to_value(&saved.reads).unwrap(),
        json!({"areas":["feedback","weekly_scorecard"],"bypass":false})
    );
    assert_eq!(saved.dsp_reads[0].areas[0].as_str(), "weekly_scorecard");
    let caller = db
        .authenticate_agent(&created.token, "migration-test")
        .unwrap();
    assert_eq!(caller.reads_at(&dsp).areas.len(), 1);
    assert_eq!(caller.reads_at(&dsp).areas[0].as_str(), "weekly_scorecard");
    assert_eq!(
        db.platform
            .one(
                "SELECT areas,bypass FROM agent_keys WHERE id=?",
                [&saved.id]
            )
            .unwrap()
            .unwrap(),
        json!({"areas":"feedback,weekly_scorecard","bypass":0})
    );
    assert!(
        AgentKeyRequest::parse(
            &json!({"name":"Retired","allDsps":false,"dsps":[dsp],"access":"read",
        "reads":{"areas":["scorecard"],"bypass":false},"dspReads":[],"expiresAt":null})
        )
        .is_err()
    );
}
#[test]
fn schedules_and_pending_jobs_keep_ids_timing_status_and_idempotency() {
    let (_root, db, dsp) = ready();
    let input = json!({"name":"Check weekly publication","collection":"weekly_scorecard","cadence":"daily",
        "intervalMinutes":null,"localTime":"07:00","enabled":true});
    let schedule = db.save_schedule(&dsp, None, &input).unwrap();
    let before = db
        .dsp(&dsp)
        .unwrap()
        .one(
            "SELECT * FROM collection_schedules WHERE id=?",
            [&schedule.id],
        )
        .unwrap()
        .unwrap();
    db.dsp(&dsp)
        .unwrap()
        .exec(
            "UPDATE collection_schedules SET collection='scorecard' WHERE id=?",
            [&schedule.id],
        )
        .unwrap();
    let queued = db
        .enqueue_weekly_scorecard(&dsp, None, "existing-week", Some("2026-W38"))
        .unwrap();
    let job = s(&queued, "id").to_owned();
    db.jobs.exec("UPDATE jobs SET kind='cortex.scorecard.collect',request=json_set(request,'$.collection','scorecard') WHERE id=?",[&job]).unwrap();
    let db = restart(db);
    assert_eq!(
        db.dsp(&dsp)
            .unwrap()
            .one(
                "SELECT * FROM collection_schedules WHERE id=?",
                [&schedule.id]
            )
            .unwrap()
            .unwrap(),
        before
    );
    let row = db.job_row(&job, Some(&dsp)).unwrap();
    assert_eq!(row.kind.as_str(), weekly_scorecard::JOB_KIND);
    assert_eq!(
        serde_json::from_str::<Value>(&row.request).unwrap()["collection"],
        "weekly_scorecard"
    );
    assert_eq!(
        s(
            &db.enqueue_weekly_scorecard(&dsp, None, "existing-week", Some("2026-W38"))
                .unwrap(),
            "id"
        ),
        job
    );
    let mut retired = input.clone();
    retired["collection"] = json!("scorecard");
    assert!(db.save_schedule(&dsp, None, &retired).is_err());
}
fn make_prior_database(db: Store, dsp: &str) -> Store {
    let directory = db.area(dsp, "data").unwrap();
    let config = db.config.clone();
    db.dsp(dsp)
        .unwrap()
        .exec(
            "DELETE FROM settings WHERE key='storage.weekly_scorecard'",
            [],
        )
        .unwrap();
    db.dsp(dsp)
        .unwrap()
        .set("storage.scorecard", &json!(1))
        .unwrap();
    drop(db);
    std::fs::rename(
        directory.join("weekly_scorecard"),
        directory.join("scorecard"),
    )
    .unwrap();
    let old = directory.join("scorecard/scorecard.sqlite");
    std::fs::rename(directory.join("scorecard/weekly_scorecard.sqlite"), &old).unwrap();
    let connection = rusqlite::Connection::open(&old).unwrap();
    for (new, prior) in [
        ("weekly_scorecard_schema", "scorecard_schema"),
        ("weekly_scorecard_publications", "scorecard_publications"),
        ("weekly_scorecard_sources", "scorecard_sources"),
        ("weekly_scorecard_weeks", "scorecard_weeks"),
        ("driver_weekly_scorecards", "driver_scorecards"),
    ] {
        connection
            .execute_batch(&format!("ALTER TABLE {new} RENAME TO {prior};"))
            .unwrap();
    }
    connection
        .execute("UPDATE storage_identity SET source='scorecard-v1'", [])
        .unwrap();
    drop(connection);
    Store::initialize(config).unwrap()
}
#[test]
fn old_storage_history_is_imported_verified_archived_and_not_duplicated_on_restart() {
    let (_root, db, dsp) = ready();
    let keeper = dispatch_core::manifest::registry().keeper(weekly_scorecard::JOB_KIND);
    for key in ["first", "again"] {
        let queued = db
            .enqueue_weekly_scorecard(&dsp, None, key, Some("2026-W38"))
            .unwrap();
        let request: Value =
            serde_json::from_str(&db.job_row(s(&queued, "id"), Some(&dsp)).unwrap().request)
                .unwrap();
        let request = weekly_scorecard::Request::parse(&keeper.bind(&db, &dsp, &request).unwrap())
            .unwrap()
            .unwrap();
        let collected = cortex::COLLECTOR
            .fixture(
                "America/Los_Angeles",
                &serde_json::to_value(request).unwrap(),
            )
            .unwrap();
        keeper
            .publish(&db, &dsp, s(&queued, "id"), collected)
            .unwrap();
    }
    let counts=db.weekly_scorecard_db(&dsp).unwrap().all("SELECT job_id,active,row_count,scope_verified FROM weekly_scorecard_publications ORDER BY job_id",[]).unwrap();
    let db = make_prior_database(db, &dsp);
    assert_eq!(db.weekly_scorecard_db(&dsp).unwrap().all("SELECT job_id,active,row_count,scope_verified FROM weekly_scorecard_publications ORDER BY job_id",[]).unwrap(),counts);
    assert!(!db.area(&dsp, "data").unwrap().join("scorecard").exists());
    assert!(
        db.area(&dsp, "state")
            .unwrap()
            .join("weekly_scorecard_migration_backup/scorecard.sqlite")
            .is_file()
    );
    assert!(
        db.dsp(&dsp)
            .unwrap()
            .setting("storage.scorecard", Value::Null)
            .unwrap()
            .is_null()
    );
    assert_eq!(
        restart(db)
            .weekly_scorecard_db(&dsp)
            .unwrap()
            .count("SELECT count(*) FROM weekly_scorecard_publications", [])
            .unwrap(),
        2
    );
}
#[test]
fn missing_marked_prior_storage_fails_closed_and_catalog_lists_only_current_names() {
    let (_root, db, dsp) = ready();
    db.dsp(&dsp)
        .unwrap()
        .exec(
            "DELETE FROM settings WHERE key='storage.weekly_scorecard'",
            [],
        )
        .unwrap();
    db.dsp(&dsp)
        .unwrap()
        .set("storage.scorecard", &json!(1))
        .unwrap();
    let config = db.config.clone();
    drop(db);
    assert!(Store::initialize(config).is_err());
    assert!(catalog::tool("scorecard").is_none());
    assert_eq!(
        catalog::tool("weekly_scorecard").unwrap().path,
        "/api/v1/weekly-scorecard"
    );
    assert!(
        catalog::openapi("https://example.test")["paths"]
            .get("/api/v1/scorecard")
            .is_none()
    );
}

#[test]
fn conflicting_current_and_prior_storage_is_preserved_for_recovery() {
    let (_root, db, dsp) = ready();
    let data = db.area(&dsp, "data").unwrap();
    db.dsp(&dsp)
        .unwrap()
        .set("storage.scorecard", &json!(1))
        .unwrap();
    let config = db.config.clone();
    drop(db);
    let old = data.join("scorecard");
    std::fs::create_dir(&old).unwrap();
    std::fs::copy(
        data.join("weekly_scorecard/weekly_scorecard.sqlite"),
        old.join("scorecard.sqlite"),
    )
    .unwrap();
    // A current marker without an import receipt must never archive the old data.
    assert!(Store::initialize(config).is_err());
    assert!(old.join("scorecard.sqlite").is_file());
    assert!(
        data.join("weekly_scorecard/weekly_scorecard.sqlite")
            .is_file()
    );
}
