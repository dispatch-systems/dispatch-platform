use dispatch_core::testing as common;
use dispatch_core::{
    collection::{api::jobs::PublicJob, registry::Provider},
    db::{Store, s},
};
use dispatch_cortex::{self as cortex, discovery::Scope, meals};
use dispatch_paycom as paycom;
use dispatch_timecard::TimecardStore;
use serde_json::{Value, json};

/// Timecard, the Driver Match it joins drivers through, and both collectors it keeps.
fn install() {
    common::install(
        &[&dispatch_paycom::COLLECTOR, &dispatch_cortex::COLLECTOR],
        &[&dispatch_driver_match::FEATURE, &dispatch_timecard::FEATURE],
    );
}

fn fixture() -> (tempfile::TempDir, Store, String, String) {
    let (root, db, id) = common::bootstrapped();
    let actor = common::a_user(&db);
    (root, db, id, actor)
}
fn enable(db: &Store, id: &str, provider: Provider) {
    common::ready_connection(db, id, provider).unwrap();
}
fn seed(db: &Store, id: &str, date: &str, index: usize) {
    let scope = Scope {
        date: date.into(),
        station: format!("DEM{index}"),
        service_area_id: format!("area-{index}"),
        provider: "provider-demo".into(),
        timezone: "America/Los_Angeles".into(),
    };
    db.publish_meals(
        id,
        &format!("seed-{date}-{index}"),
        &meals::fixture(&scope),
        &scope,
    )
    .unwrap();
}
fn jobs(db: &Store, id: &str) -> Vec<PublicJob> {
    db.recent_jobs(Some(id)).unwrap()
}
#[test]
fn first_sync_discovers_from_tenant_profile_and_replays_after_publication() {
    install();
    let (_root, db, id, actor) = fixture();
    enable(&db, &id, paycom::PROVIDER);
    enable(&db, &id, cortex::PROVIDER);
    db.set_profile(&id, json!({"stationCode":"TST1", "abbreviation":"NLOG"}))
        .unwrap();
    assert_eq!(
        db.meal_sync_status(&id, "2026-01-11").unwrap()["scopeAvailable"],
        true
    );
    let queued = db
        .enqueue_meal_sync(&id, &actor, "first:ñ", "2026-01-11")
        .unwrap();
    assert_eq!(queued["jobs"].as_array().unwrap().len(), 2);
    let row = db.job_row(s(&queued["jobs"][1], "id"), Some(&id)).unwrap();
    let request: Value = serde_json::from_str(&row.request).unwrap();
    assert_eq!(request["station"], "TST1");
    assert_eq!(request["dspAbbreviation"], "NLOG");
    assert_eq!(request["dspName"], db.find_dsp(&id).unwrap().name);
    assert!(request.get("serviceAreaId").is_none());
    db.cancel_dsp(&id).unwrap();
    seed(&db, &id, "2026-01-11", 1);
    seed(&db, &id, "2026-01-11", 2);
    let replay = db
        .enqueue_meal_sync(&id, &actor, "first:ñ", "2026-01-11")
        .unwrap();
    for index in 0..2 {
        assert_eq!(queued["jobs"][index]["id"], replay["jobs"][index]["id"]);
    }
    assert_eq!(jobs(&db, &id).len(), 2);
    assert_eq!(
        db.enqueue_meal_sync(&id, &actor, "first:ñ", "2026-01-12")
            .unwrap_err()
            .code,
        "idempotency_conflict"
    );
}
#[test]
fn combined_sync_reuses_tenant_scope_records_date_and_is_idempotent() {
    install();
    let (_root, db, id, actor) = fixture();
    enable(&db, &id, paycom::PROVIDER);
    enable(&db, &id, cortex::PROVIDER);
    assert_eq!(
        db.enqueue_meal_sync(&id, &actor, "first", "2026-01-11")
            .unwrap_err()
            .code,
        "meal_sync_scope_required"
    );
    assert!(jobs(&db, &id).is_empty());
    seed(&db, &id, "2026-01-10", 1);
    let queued = db
        .enqueue_meal_sync(&id, &actor, "first", "2026-01-11")
        .unwrap();
    assert_eq!(queued["jobs"].as_array().unwrap().len(), 2);
    assert_eq!(
        queued,
        db.enqueue_meal_sync(&id, &actor, "first", "2026-01-11")
            .unwrap()
    );
    assert_eq!(
        db.enqueue_meal_sync(&id, &actor, "first", "2026-01-12")
            .unwrap_err()
            .code,
        "idempotency_conflict"
    );
    assert_eq!(
        db.enqueue_meal_sync(&id, &actor, "second", "2026-01-11")
            .unwrap_err()
            .code,
        "sync_in_progress"
    );
    for job in queued["jobs"].as_array().unwrap() {
        let row = db.job_row(s(job, "id"), Some(&id)).unwrap();
        let request: Value = serde_json::from_str(&row.request).unwrap();
        assert_eq!(request["date"], "2026-01-11");
        if row.kind.as_str() == "cortex.meal_breaks.collect" {
            assert_eq!(request["serviceAreaId"], "area-1");
            assert_eq!(request["timezone"], "America/Los_Angeles");
        }
    }
    let status = db.meal_sync_status(&id, "2026-01-11").unwrap();
    assert_eq!(status["paycom"]["active"], true);
    assert_eq!(status["flex"]["active"], true);
    assert_eq!(status["flex"]["collectedAt"], Value::Null);
    db.cancel_dsp(&id).unwrap();
    common::set_status_of_kind(&db, "cortex.meal_breaks.collect", "failed").unwrap();
    common::set_status_of_kind(&db, "paycom.collect", "succeeded").unwrap();
    let status = db.meal_sync_status(&id, "2026-01-11").unwrap();
    assert_eq!(status["flex"]["job"]["status"], "failed");
    assert_eq!(status["paycom"]["job"]["status"], "succeeded");
    assert_eq!(
        db.meal_sync_status(&id, "2026-01-12").unwrap()["flex"]["job"],
        Value::Null
    );
}
#[test]
fn invalid_dates_missing_connections_and_capacity_never_queue_half_a_sync() {
    install();
    let (_root, db, id, actor) = fixture();
    seed(&db, &id, "2026-01-10", 1);
    enable(&db, &id, paycom::PROVIDER);
    assert_eq!(
        db.enqueue_meal_sync(&id, &actor, "missing", "2026-01-10")
            .unwrap_err()
            .code,
        "meal_sync_flex_required"
    );
    enable(&db, &id, cortex::PROVIDER);
    for date in ["2026-02-30", "2099-01-01", "2026-1-10", "1999-12-31"] {
        assert_eq!(
            db.enqueue_meal_sync(&id, &actor, "invalid", date)
                .unwrap_err()
                .code,
            "invalid_date"
        );
    }
    assert!(jobs(&db, &id).is_empty());
    for i in 2..=15 {
        seed(&db, &id, "2026-01-10", i);
    }
    assert_eq!(
        db.enqueue_meal_sync(&id, &actor, "full", "2026-01-10")
            .unwrap_err()
            .code,
        "queue_full"
    );
    assert!(jobs(&db, &id).is_empty());
    assert_eq!(
        db.enqueue_paycom_date(&id, Some(&actor), "day", "2026-01-09")
            .unwrap()["kind"],
        "paycom.collect"
    );
    assert_eq!(
        db.meal_sync_status(&id, "2026-01-10").unwrap()["paycom"]["active"],
        true
    );
    assert_eq!(
        json!({"date":"2026-01-09"}),
        serde_json::from_str::<Value>(&db.job_row(&jobs(&db, &id)[0].id, None).unwrap().request)
            .unwrap()
    );
}

#[test]
fn employee_period_batches_all_proven_scopes_without_an_employee_or_station_job_limit() {
    install();
    let (_root, db, id, actor) = fixture();
    let zone = db.find_dsp(&id).unwrap().timezone;
    db.publish_timecards(&id, &paycom::fixtures::fixture(&zone).unwrap())
        .unwrap();
    let latest = db.timecard_period(&id, None, None, None).unwrap();
    let period = latest.shift(-1).unwrap();
    enable(&db, &id, paycom::PROVIDER);
    enable(&db, &id, cortex::PROVIDER);
    // No single day holds all scope identities. Every day in the period must collect
    // their union, including a station not yet collected for this period.
    for index in 1..=5 {
        let date = (period.start().unwrap() + chrono::Duration::days(index as i64)).to_string();
        seed(&db, &id, &date, index);
    }
    seed(&db, &id, &period.shift(-1).unwrap().from, 6);
    let queued = db
        .enqueue_employee_sync(&id, &actor, "period:ñ", &period)
        .unwrap();
    assert_eq!(queued.paycom.total, 1);
    assert_eq!(queued.cortex.total, 14);
    assert_eq!(jobs(&db, &id).len(), 15);
    assert_eq!(queued.through, period.to);
    let paycom: Value = serde_json::from_str(
        &db.job_row(&queued.paycom.jobs[0].id, Some(&id))
            .unwrap()
            .request,
    )
    .unwrap();
    assert_eq!(
        paycom,
        json!({"allEmployees":true,"from":period.from,"to":period.to})
    );
    let mut dates = Vec::new();
    for job in &queued.cortex.jobs {
        let value: Value =
            serde_json::from_str(&db.job_row(&job.id, Some(&id)).unwrap().request).unwrap();
        let request = meals::DayRequest::parse(&value).unwrap().unwrap();
        assert_eq!(request.scope_requests.len(), 6);
        assert!(
            value["scopeRequests"]
                .as_array()
                .unwrap()
                .iter()
                .all(|scope| scope["date"] == request.date)
        );
        dates.push(request.date);
    }
    assert_eq!(dates.first().unwrap(), &period.from);
    assert_eq!(dates.last().unwrap(), &period.to);
    db.cancel_dsp(&id).unwrap();
    seed(&db, &id, &period.from, 7);
    // An accepted request remains replayable when the connection later goes off.
    for provider in [paycom::PROVIDER, cortex::PROVIDER] {
        db.collector(&id, provider)
            .unwrap()
            .exec("UPDATE connections SET enabled=0", [])
            .unwrap();
    }
    let replay = db
        .enqueue_employee_sync(&id, &actor, "period:ñ", &period)
        .unwrap();
    assert!(!replay.paycom.enabled && !replay.cortex.enabled);
    assert_eq!(queued.paycom.jobs[0].id, replay.paycom.jobs[0].id);
    assert_eq!(
        queued
            .cortex
            .jobs
            .iter()
            .map(|job| &job.id)
            .collect::<Vec<_>>(),
        replay
            .cortex
            .jobs
            .iter()
            .map(|job| &job.id)
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        db.enqueue_employee_sync(&id, &actor, "period:ñ", &latest)
            .unwrap_err()
            .code,
        "idempotency_conflict"
    );
    assert_eq!(
        db.employee_sync_status(&id, &period, None)
            .unwrap()
            .request_id
            .as_deref(),
        Some("period:ñ")
    );
}

#[test]
fn employee_period_sync_reserves_both_sources_and_retains_cutoff_and_all_day_outcomes() {
    install();
    let (_root, db, id, actor) = fixture();
    let zone = db.find_dsp(&id).unwrap().timezone;
    db.publish_timecards(&id, &paycom::fixtures::fixture(&zone).unwrap())
        .unwrap();
    let period = db.timecard_period(&id, None, None, None).unwrap();
    enable(&db, &id, paycom::PROVIDER);
    assert_eq!(
        db.enqueue_employee_sync(&id, &actor, "limited", &period)
            .unwrap_err()
            .code,
        "meal_sync_flex_required"
    );
    assert!(jobs(&db, &id).is_empty());
    enable(&db, &id, cortex::PROVIDER);
    assert_eq!(
        db.enqueue_employee_sync(&id, &actor, "limited", &period)
            .unwrap_err()
            .code,
        "meal_sync_scope_required"
    );
    assert!(jobs(&db, &id).is_empty());
    db.set_profile(&id, json!({"stationCode":"TST1","abbreviation":"TEST"}))
        .unwrap();
    let queued = db
        .enqueue_employee_sync(&id, &actor, "limited", &period)
        .unwrap();
    let today = db.local_date(&id).unwrap();
    assert_eq!(queued.through, today);
    let days = (today.parse::<chrono::NaiveDate>().unwrap() - period.start().unwrap()).num_days()
        as usize
        + 1;
    assert_eq!(queued.cortex.total, days);
    let dates: Vec<_> = queued
        .cortex
        .jobs
        .iter()
        .map(|job| {
            let value: Value =
                serde_json::from_str(&db.job_row(&job.id, Some(&id)).unwrap().request).unwrap();
            assert!(value["scopeRequests"][0].get("serviceAreaId").is_none());
            value["date"].as_str().unwrap().to_owned()
        })
        .collect();
    assert_eq!(dates.last().unwrap(), &today);
    common::set_every_status(&db, "succeeded").unwrap();
    common::set_job_status(&db, &queued.cortex.jobs[0].id, "failed").unwrap();
    let failed = db.employee_sync_status(&id, &period, None).unwrap();
    assert!(!failed.complete && !failed.active);
    assert_eq!(failed.cortex.failed, 1);
    assert_eq!(failed.cortex.completed, days - 1);
    assert!(failed.cortex.progress < 100);
    // A later successful date must never hide this failed first date.
    common::set_job_status(&db, &queued.cortex.jobs[0].id, "waiting_verification").unwrap();
    let active = db.employee_sync_status(&id, &period, None).unwrap();
    assert!(active.active && active.cortex.active && !active.complete);
    assert_eq!(
        db.enqueue_employee_sync(&id, &actor, "overlap", &period)
            .unwrap_err()
            .code,
        "sync_in_progress"
    );
    common::set_job_status(&db, &queued.cortex.jobs[0].id, "cancelled").unwrap();
    let cancelled = db
        .employee_sync_status(&id, &period, Some("limited"))
        .unwrap();
    assert_eq!(cancelled.cortex.cancelled, 1);
    assert!(!cancelled.complete);
    common::set_job_status(&db, &queued.cortex.jobs[0].id, "succeeded").unwrap();
    let complete = db.employee_sync_status(&id, &period, None).unwrap();
    assert!(complete.complete);
    assert_eq!(complete.paycom.progress, 100);
    assert_eq!(complete.cortex.progress, 100);
}

#[test]
fn successful_station_does_not_hide_a_failed_station_in_the_same_sync() {
    install();
    let (_root, db, id, actor) = fixture();
    enable(&db, &id, paycom::PROVIDER);
    enable(&db, &id, cortex::PROVIDER);
    seed(&db, &id, "2026-01-10", 1);
    seed(&db, &id, "2026-01-10", 2);
    let result = db
        .enqueue_meal_sync(&id, &actor, "stations", "2026-01-11")
        .unwrap();
    common::set_every_status(&db, "succeeded").unwrap();
    let flex = s(&result["jobs"][1], "id");
    common::set_job_status(&db, flex, "failed").unwrap();
    common::set_job_created(&db, flex, "2026-01-01").unwrap();
    let status = db.meal_sync_status(&id, "2026-01-11").unwrap();
    assert_eq!(status["flex"]["job"]["status"], "failed");
    assert_eq!(status["flex"]["active"], false);
    assert_eq!(status["paycom"]["job"]["status"], "succeeded");
}

#[test]
fn status_reads_allow_a_viewer_date_ahead_of_the_dsp_but_collection_does_not() {
    install();
    let (_root, db, id, actor) = fixture();
    let tomorrow = (chrono::Utc::now().date_naive() + chrono::Duration::days(1)).to_string();
    assert_eq!(
        db.meal_sync_status(&id, &tomorrow).unwrap()["date"],
        tomorrow
    );
    assert_eq!(
        db.enqueue_meal_sync(&id, &actor, "future", &tomorrow)
            .unwrap_err()
            .code,
        "invalid_date"
    );
    assert_eq!(
        db.meal_sync_status(&id, "2026-02-30").unwrap_err().code,
        "invalid_date"
    );
}

#[test]
fn manual_sync_lock_and_original_date_follow_the_entire_batch() {
    install();
    let (_root, db, id, actor) = fixture();
    enable(&db, &id, paycom::PROVIDER);
    enable(&db, &id, cortex::PROVIDER);
    seed(&db, &id, "2026-01-10", 1);
    let original = db
        .enqueue_meal_sync(&id, &actor, "original", "2026-01-11")
        .unwrap();
    let paycom = s(&original["jobs"][0], "id");
    let flex = s(&original["jobs"][1], "id");
    common::set_job_status(&db, paycom, "succeeded").unwrap();
    let scope = Scope {
        date: "2026-01-12".into(),
        station: "DEM1".into(),
        service_area_id: "area-1".into(),
        provider: "provider-demo".into(),
        timezone: "America/Los_Angeles".into(),
    };
    for status in ["queued", "running", "waiting_verification"] {
        common::set_job_status(&db, flex, status).unwrap();
        let viewed = db.meal_sync_status(&id, "2026-01-12").unwrap();
        assert_eq!(viewed["date"], "2026-01-12");
        assert_eq!(viewed["flex"]["jobDate"], "2026-01-11");
        assert_eq!(viewed["flex"]["active"], true);
        assert_eq!(viewed["paycom"]["active"], false);
        assert_eq!(viewed["flex"]["collectedAt"], Value::Null);
        for rejected in [
            db.enqueue_timecards(&id, Some(&actor), "employees"),
            db.enqueue_paycom_date(&id, Some(&actor), "timecard", "2026-01-12"),
            db.enqueue_meals(&id, Some(&actor), "flex-only", &scope),
            db.enqueue_meal_sync(&id, &actor, "other-day", "2026-01-12"),
        ] {
            assert_eq!(rejected.unwrap_err().code, "sync_in_progress");
        }
        // Retrying an accepted request returns its jobs without starting another sync.
        let replay = db
            .enqueue_meal_sync(&id, &actor, "original", "2026-01-11")
            .unwrap();
        assert_eq!(replay["jobs"][0]["id"], paycom);
        assert_eq!(replay["jobs"][1]["id"], flex);
        assert_eq!(jobs(&db, &id).len(), 2);
    }
    for status in ["succeeded", "failed", "cancelled"] {
        common::set_job_status(&db, flex, status).unwrap();
        assert_eq!(
            db.meal_sync_status(&id, "2026-01-12").unwrap()["flex"]["active"],
            false
        );
        let next = db
            .enqueue_timecards(&id, Some(&actor), &format!("after-{status}"))
            .unwrap();
        assert_eq!(next["status"], "queued");
        db.cancel(s(&next, "id"), &id).unwrap();
    }
}
