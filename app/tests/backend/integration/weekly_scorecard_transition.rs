//! Startup retires old names while preserving data and restrictions; current readers reject them.
use dispatch_core::{
    accounts::Auth,
    db::{Store, s},
    server::startup,
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
    startup::prepare_storage(config).unwrap()
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
        site: dispatch_core::foundation::config::Site::Admin,
        scope: None,
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
    db.dsp(&dsp).unwrap().exec("INSERT INTO roles(id,dsp_id,name,permissions,system,created_at) VALUES ('old-role',?,'Weekly reader','[\"scorecard.view\"]',0,'2026-01-01')",[&dsp]).unwrap();
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
        .dsp(&dsp)
        .unwrap()
        .one("SELECT permissions FROM roles WHERE id='old-role'", [])
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(s(&saved, "permissions")).unwrap(),
        json!(["weekly_scorecard.view"])
    );
}
#[test]
fn schedules_and_pending_jobs_keep_ids_timing_status_and_idempotency() {
    let (_root, db, dsp) = ready();
    let rule = json!({"collects":"weekly_scorecard","kind":"times","days":[0,1,2,3,4,5,6],
        "times":["07:00"]});
    let saved = dispatch_core::testing::schedule(&db, &dsp, "weekly_scorecard", json!([rule]));
    let key = saved.unwrap().rules[0].id.clone().unwrap();
    let before = db
        .dsp(&dsp)
        .unwrap()
        .one("SELECT * FROM collection_schedules WHERE id=?", [&key])
        .unwrap()
        .unwrap();
    db.dsp(&dsp)
        .unwrap()
        .exec(
            "UPDATE collection_schedules SET collection='scorecard' WHERE id=?",
            [&key],
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
            .one("SELECT * FROM collection_schedules WHERE id=?", [&key])
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
    assert!(dispatch_core::collection::schedules::Scheduled::named("scorecard").is_err());
}
