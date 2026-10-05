//! Existing identifiers survive the weekly rename without granting new access or
//! leaving settings and jobs unreadable by the previous release.
use dispatch_core::{
    accounts::Auth,
    collection::jobs::CancelJobs,
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
fn a_saved_disabled_switch_is_preserved_and_both_inputs_write_the_rollback_spelling() {
    let (_root, db, dsp) = ready();
    db.platform.exec("UPDATE dsp_features SET enabled=0,changed_at='2026-01-01' WHERE dsp_id=? AND feature='scorecard'", [&dsp]).unwrap();
    assert!(!db.feature_enabled(&dsp, "weekly_scorecard").unwrap());
    let report = db.feature_report(&dsp).unwrap();
    let state = report
        .features
        .iter()
        .find(|f| f.feature == "weekly_scorecard")
        .unwrap();
    assert!(!state.enabled);
    assert_eq!(state.changed_at.as_deref(), Some("2026-01-01"));
    let actor = common::platform_owner(&db);
    db.set_feature(&dsp, "weekly_scorecard", true, &actor)
        .unwrap();
    db.set_feature(&dsp, "scorecard", false, &actor).unwrap();
    let raw = db.platform.all("SELECT feature,enabled FROM dsp_features WHERE dsp_id=? AND feature IN ('scorecard','weekly_scorecard')", [&dsp]).unwrap();
    assert_eq!(raw, vec![json!({"feature":"scorecard","enabled":0})]);
}

#[test]
fn restricted_roles_read_old_permissions_and_save_only_compatible_grants() {
    let (_root, db, dsp) = ready();
    let context = owner(&db, &dsp);
    db.platform.exec("INSERT INTO roles(id,dsp_id,name,permissions,system,created_at) VALUES ('legacy-weekly',?,'Weekly reader','[\"scorecard.view\"]',0,'2026-01-01')", [&dsp]).unwrap();
    let role = db.role(&dsp, "legacy-weekly").unwrap();
    assert_eq!(role.permissions, ["weekly_scorecard.view"]);
    let mut auth = context.auth.clone();
    auth.preview = Some(role.id.clone());
    let limited = db.context(&auth, &dsp, "weekly_scorecard.view").unwrap();
    assert!(!limited.can("weekly_scorecard.collect"));
    assert!(!limited.can("weekly_scorecard.manage"));
    assert!(
        db.create_role(
            &limited,
            "Cannot grant collection",
            &["weekly_scorecard.collect".into()]
        )
        .is_err()
    );
    db.update_role(
        &context,
        &role.id,
        "Weekly reader",
        &["weekly_scorecard.view".into()],
    )
    .unwrap();
    let saved = db
        .platform
        .one("SELECT permissions FROM roles WHERE id=?", [&role.id])
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(s(&saved, "permissions")).unwrap(),
        json!(["scorecard.view"])
    );
    let role = db
        .create_role(&context, "Weekly collector", &["scorecard.collect".into()])
        .unwrap();
    assert_eq!(
        role.permissions,
        ["weekly_scorecard.view", "weekly_scorecard.collect"]
    );
    let saved = db
        .platform
        .one("SELECT permissions FROM roles WHERE id=?", [&role.id])
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(s(&saved, "permissions")).unwrap(),
        json!(["scorecard.view", "scorecard.collect"])
    );
}

#[test]
fn global_and_per_dsp_agent_reads_preserve_their_restrictions_and_rollback_spelling() {
    let (_root, db, dsp) = ready();
    let actor = common::platform_owner(&db);
    let input = |area| {
        AgentKeyRequest::parse(&json!({
            "name":"Weekly reader","allDsps":false,"dsps":[dsp],"access":"read",
            "reads":{"areas":["feedback",area],"bypass":false},
            "dspReads":[{"dsp":dsp,"areas":[area],"bypass":false}],"expiresAt":null
        }))
        .unwrap()
    };
    let created = db.create_agent_key(&actor, &input("scorecard")).unwrap();
    let saved = db
        .update_agent_key(&actor, &created.key.id, &input("weekly_scorecard"))
        .unwrap();
    assert_eq!(
        serde_json::to_value(&saved.reads).unwrap(),
        json!({"areas":["feedback","weekly_scorecard"],"bypass":false})
    );
    assert_eq!(saved.dsp_reads[0].areas.len(), 1);
    assert_eq!(saved.dsp_reads[0].areas[0].as_str(), "weekly_scorecard");
    let raw = db
        .platform
        .one(
            "SELECT areas,bypass FROM agent_keys WHERE id=?",
            [&saved.id],
        )
        .unwrap()
        .unwrap();
    assert_eq!(raw, json!({"areas":"feedback,scorecard","bypass":0}));
    let raw = db
        .platform
        .one(
            "SELECT areas,bypass FROM agent_key_dsp_reads WHERE key_id=? AND dsp_id=?",
            [&saved.id, &dsp],
        )
        .unwrap()
        .unwrap();
    assert_eq!(raw, json!({"areas":"scorecard","bypass":0}));
    db.authenticate_agent(&created.token, "test").unwrap();
}

#[test]
fn saved_schedules_keep_ids_names_enabled_states_and_deadlines() {
    let (_root, db, dsp) = ready();
    let mut input = json!({"name":"Check for Amazon publication","collection":"scorecard","cadence":"daily","intervalMinutes":null,"localTime":"07:00","enabled":true});
    let before = db.save_schedule(&dsp, None, &input).unwrap();
    input["collection"] = json!("weekly_scorecard");
    input["revision"] = json!(before.revision);
    let after = db.save_schedule(&dsp, Some(&before.id), &input).unwrap();
    assert_eq!(after.id, before.id);
    assert_eq!(after.name, before.name);
    assert_eq!(after.enabled, before.enabled);
    assert!(before.next_run.is_some());
    assert_eq!(after.next_run, before.next_run);
    assert_eq!(after.collection.as_str(), "weekly_scorecard");
    let raw = db
        .dsp(&dsp)
        .unwrap()
        .one(
            "SELECT collection FROM collection_schedules WHERE id=?",
            [&after.id],
        )
        .unwrap()
        .unwrap();
    assert_eq!(raw["collection"], "scorecard");
    assert_eq!(db.collection_schedules(&dsp).unwrap().schedules.len(), 1);
}

#[test]
fn legacy_jobs_are_claimed_once_and_both_kind_scopes_find_and_cancel_them() {
    let (_root, db, dsp) = ready();
    let first = db
        .enqueue_weekly_scorecard(&dsp, None, "existing-week", Some("2026-W38"))
        .unwrap();
    let id = s(&first, "id");
    let canonical = json!({"collection":"weekly_scorecard","week":"2026-W38","station":"TST1"});
    let retry = db
        .enqueue_for(&dsp, None, "existing-week", cortex::PROVIDER, &canonical)
        .unwrap();
    assert_eq!(retry["id"], first["id"]);
    let raw = db
        .jobs
        .one("SELECT kind,request FROM jobs WHERE id=?", [id])
        .unwrap()
        .unwrap();
    assert_eq!(raw["kind"], "cortex.scorecard.collect");
    assert_eq!(
        serde_json::from_str::<Value>(s(&raw, "request")).unwrap()["collection"],
        "scorecard"
    );
    for spelling in ["cortex.scorecard.collect", weekly_scorecard::JOB_KIND] {
        assert_eq!(db.recent_jobs_of(&dsp, spelling).unwrap().len(), 1);
        assert!(db.any_active_job(&dsp, &[spelling]).unwrap());
    }
    let claimed = db.claim_job("weekly-worker", |_, _| true).unwrap().unwrap();
    assert_eq!(claimed.id, id);
    assert_eq!(claimed.kind.as_str(), weekly_scorecard::JOB_KIND);
    db.guard(id, "weekly-worker").unwrap();
    assert!(
        db.claim_job("second-worker", |_, _| true)
            .unwrap()
            .is_none()
    );
    let keeper = dispatch_core::manifest::registry().keeper("cortex.scorecard.collect");
    let bound = keeper
        .bind(&db, &dsp, &serde_json::from_str(&claimed.request).unwrap())
        .unwrap();
    assert!(weekly_scorecard::Request::parse(&bound).unwrap().is_some());
    db.cancel_jobs(CancelJobs::Kind {
        dsp: &dsp,
        kind: weekly_scorecard::JOB_KIND,
    })
    .unwrap();
    assert!(
        !db.any_active_job(&dsp, &["cortex.scorecard.collect"])
            .unwrap()
    );
}

#[test]
fn discovery_lists_only_the_weekly_name_but_accepts_the_legacy_tool() {
    dispatch_backend::install();
    let canonical = catalog::tool("weekly_scorecard").unwrap();
    assert!(std::ptr::eq(canonical, catalog::tool("scorecard").unwrap()));
    assert_eq!(canonical.path, "/api/v1/weekly-scorecard");
    let openapi = catalog::openapi("https://dispatch.example.com");
    assert!(openapi["paths"].get("/api/v1/weekly-scorecard").is_some());
    assert!(openapi["paths"].get("/api/v1/scorecard").is_none());
    assert_eq!(
        catalog::ENDPOINTS
            .iter()
            .filter(|e| e.tool == "weekly_scorecard")
            .count(),
        1
    );
    assert!(catalog::ENDPOINTS.iter().all(|e| e.tool != "scorecard"));
}
