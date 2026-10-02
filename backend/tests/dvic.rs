mod common;
use dispatch_backend::{
    collectors::Provider,
    db::{self, Store, s},
    dvic::{self, Capture, Request},
};
use serde_json::json;

fn request(weeks: &[&str]) -> Request {
    Request::parse(&json!({"collection":"dvic","station":"TST1","weeks":weeks}))
        .unwrap()
        .unwrap()
}
fn ready() -> (tempfile::TempDir, Store, String) {
    let (root, db, id) = common::bootstrapped();
    db.set_profile(
        &id,
        json!({"stationCode":"TST1","abbreviation":"FXTR","setupRequired":false}),
    )
    .unwrap();
    db.collector(&id, Provider::Cortex)
        .unwrap()
        .exec("UPDATE connections SET enabled=1,status='ready'", [])
        .unwrap();
    (root, db, id)
}
fn publish(db: &Store, id: &str, key: &str, capture: &Capture) -> String {
    let job = db
        .enqueue_dvic(id, None, key, Some(&capture.weeks[0]), capture.weeks.len())
        .unwrap();
    let job = s(&job, "id").to_owned();
    db.publish_dvic(id, &job, capture).unwrap();
    // The worker normally transitions the job after publication.
    db.jobs
        .exec("UPDATE jobs SET status='succeeded' WHERE id=?", [&job])
        .unwrap();
    job
}
fn count(db: &Store, id: &str, table: &str) -> i64 {
    db.dvic(id)
        .unwrap()
        .count(&format!("SELECT count(*) FROM {table}"), [])
        .unwrap()
}

#[test]
fn rolling_overlap_updates_names_once_and_older_backfills_cannot_replace_corrections() {
    let (_root, db, id) = ready();
    let mut newer = dvic::fixture(&request(&["2026-W39"])).unwrap();
    newer.reports[0].rows.as_mut().unwrap()[0].start_date = "2026-09-19".into();
    newer.reports[0].rows.as_mut().unwrap()[0].start_time = "2026-09-19 09:00:00".into();
    newer.reports[0].rows.as_mut().unwrap()[0].end_time = "2026-09-19 09:01:15".into();
    newer.reports[0].rows.as_mut().unwrap()[0].transporter_name = "Updated Name".into();
    let first = publish(&db, &id, "newer", &newer);
    db.publish_dvic(&id, &first, &newer).unwrap();
    publish(&db, &id, "same-file", &newer);
    let older = dvic::fixture(&request(&["2026-W38"])).unwrap();
    publish(&db, &id, "backfill", &older);
    assert_eq!(count(&db, &id, "dvic_inspections"), 1);
    assert_eq!(count(&db, &id, "dvic_reports"), 2);
    assert_eq!(count(&db, &id, "dvic_revisions"), 2);
    let found = db
        .dvic_inspections(&id, "2026-09-01", "2026-09-30", None, "", 100)
        .unwrap();
    assert_eq!(found.inspections[0].driver_name, "Updated Name");
    assert_eq!(found.inspections[0].source_report_date, "2026-09-27");
    assert_eq!(found.inspections[0].short_by_seconds, 15.0);
    // A corrected source explicitly meeting the minimum supersedes the exception.
    let row = &mut newer.reports[0].rows.as_mut().unwrap()[0];
    row.duration = 90.0;
    row.end_time = "2026-09-19 09:01:30".into();
    newer.reports[0].sha256 = dvic::hash(b"corrected bytes");
    newer.reports[0].modified_at += 1000;
    publish(&db, &id, "correction", &newer);
    publish(&db, &id, "older-again", &older);
    assert_eq!(db.dvic_status(&id).unwrap().short_inspections, 0);
    assert_eq!(count(&db, &id, "dvic_revisions"), 3);
}

#[test]
fn unchanged_reports_require_a_matching_committed_revision_and_batches_are_atomic() {
    let (_root, db, id) = ready();
    let mut capture = dvic::fixture(&request(&["2026-W39", "2026-W38"])).unwrap();
    let job = publish(&db, &id, "initial", &capture);
    for report in &mut capture.reports {
        report.rows = None;
    }
    publish(&db, &id, "conditional", &capture);
    assert_eq!(count(&db, &id, "dvic_revisions"), 2);
    assert_eq!(count(&db, &id, "dvic_inspections"), 2);
    let raw = db
        .dvic(&id)
        .unwrap()
        .all(
            "SELECT downloaded,unchanged FROM dvic_runs WHERE job_id<>?",
            [&job],
        )
        .unwrap();
    assert_eq!(raw, vec![json!({"downloaded":0,"unchanged":2})]);
    let mut bad = dvic::fixture(&request(&["2026-W39", "2026-W38"])).unwrap();
    bad.reports[0].sha256 = dvic::hash(b"new first file");
    bad.reports[1].rows = None;
    bad.reports[1].sha256 = dvic::hash(b"not imported");
    let job = db
        .enqueue_dvic(&id, None, "bad", Some("2026-W39"), 2)
        .unwrap();
    assert!(db.publish_dvic(&id, s(&job, "id"), &bad).is_err());
    assert_eq!(count(&db, &id, "dvic_revisions"), 2);
    assert_eq!(count(&db, &id, "dvic_runs"), 2);
}

#[test]
fn empty_reports_keep_history_and_missing_weeks_are_rechecked_without_inventing_coverage() {
    let (_root, db, id) = ready();
    let mut capture = dvic::fixture(&request(&["2026-W39"])).unwrap();
    publish(&db, &id, "initial", &capture);
    capture.reports[0].rows = Some(Vec::new());
    capture.reports[0].sha256 = dvic::hash(b"headers only");
    capture.reports[0].modified_at += 1000;
    publish(&db, &id, "empty", &capture);
    assert_eq!(db.dvic_status(&id).unwrap().short_inspections, 1);
    assert_eq!(db.dvic_status(&id).unwrap().reports[0].min_date, None);
    capture.reports.clear();
    publish(&db, &id, "unposted", &capture);
    assert_eq!(db.dvic_status(&id).unwrap().weeks[0].report_count, 0);
    assert_eq!(count(&db, &id, "dvic_reports"), 1);
    let jobs = db.dvic_jobs(&id).unwrap();
    assert_eq!(jobs.len(), 1);
    let request = Request::parse(&jobs[0].1).unwrap().unwrap();
    assert_eq!(request.weeks.len(), 4);
    assert_eq!(
        request.weeks[0],
        dvic::report_week(chrono::Utc::now().date_naive())
    );
}

#[test]
fn publication_rejects_wrong_scope_and_reads_are_paginated_and_isolated() {
    let (_root, db, id) = ready();
    let mut capture = dvic::fixture(&request(&["2026-W39", "2026-W38"])).unwrap();
    let job = db
        .enqueue_dvic(&id, None, "scope", Some("2026-W39"), 2)
        .unwrap();
    capture.reports[1].rows.as_mut().unwrap()[0].station = "OTHER".into();
    assert!(db.publish_dvic(&id, s(&job, "id"), &capture).is_err());
    assert_eq!(count(&db, &id, "dvic_reports"), 0);
    capture.reports[1].rows.as_mut().unwrap()[0].station = "TST1".into();
    db.publish_dvic(&id, s(&job, "id"), &capture).unwrap();
    let first = db
        .dvic_inspections(&id, "2026-09-01", "2026-09-30", None, "", 1)
        .unwrap();
    assert_eq!(first.inspections.len(), 1);
    let second = db
        .dvic_inspections(
            &id,
            "2026-09-01",
            "2026-09-30",
            None,
            first.next_cursor.as_ref().unwrap(),
            1,
        )
        .unwrap();
    assert_eq!(second.inspections.len(), 1);
    assert!(second.next_cursor.is_none());
    assert_ne!(first.inspections[0].id, second.inspections[0].id);
    assert!(
        db.dvic_inspections(
            &id,
            "2026-09-01",
            "2026-09-30",
            Some("someone-else"),
            "",
            100
        )
        .unwrap()
        .inspections
        .is_empty()
    );
    db.set_profile(&id, json!({"stationCode":"TST2"})).unwrap();
    assert_eq!(db.dvic_status(&id).unwrap().short_inspections, 0);
}

#[test]
fn the_status_keeps_its_jobs_however_many_of_other_kinds_came_since() {
    let (_root, db, id) = ready();
    let job = db.enqueue_dvic(&id, None, "busy", None, 2).unwrap();
    let job = s(&job, "id").to_owned();
    // A busy DSP: more newer jobs of another kind than the latest 200 hold.
    for index in 0..201 {
        let other = format!("paycom-{index}");
        db.jobs.exec("INSERT INTO jobs(id,dsp_id,environment,kind,status,available_at,created_at,release,connection_revision,idempotency_key) VALUES (?,?,'preview','paycom.collect','succeeded',0,?,'test',1,?)",rusqlite::params![other,id,db::at(db::now()+1000+index),other]).unwrap();
    }
    let listed = |jobs: Vec<dispatch_backend::contracts::PublicJob>| {
        jobs.into_iter().map(|j| j.id).collect::<Vec<_>>()
    };
    assert!(!listed(db.recent_jobs(Some(&id)).unwrap()).contains(&job));
    assert_eq!(listed(db.dvic_status(&id).unwrap().jobs), [job]);
    // The other way round: the Timecard's list keeps its own jobs however many DVIC jobs
    // came since.
    for index in 0..201 {
        let other = format!("dvic-{index}");
        db.jobs.exec("INSERT INTO jobs(id,dsp_id,environment,kind,status,available_at,created_at,release,connection_revision,idempotency_key) VALUES (?,?,'preview','cortex.dvic.collect','succeeded',0,?,'test',1,?)",rusqlite::params![other,id,db::at(db::now()+5000+index),other]).unwrap();
    }
    let others = listed(db.recent_jobs_in(&id, &["paycom.collect"]).unwrap());
    assert_eq!(others.len(), 200);
    assert!(others.iter().all(|j| j.starts_with("paycom-")));
}

#[test]
fn thresholds_are_strict_and_publication_weeks_follow_iso_including_year_rollover() {
    let capture = dvic::fixture(&request(&["2026-W39"])).unwrap();
    let mut row = capture.reports[0].rows.as_ref().unwrap()[0].clone();
    for (fleet, min) in [("CV", 90.0), ("CDV", 90.0), ("SV", 300.0)] {
        row.fleet_type = fleet.into();
        row.duration = min - 0.1;
        assert!(row.is_short().unwrap());
        row.duration = min;
        assert!(!row.is_short().unwrap());
    }
    row.fleet_type = "unknown".into();
    assert!(row.is_short().is_err());
    for (date, week) in [
        ("2026-09-27", "2026-W39"),
        ("2026-09-28", "2026-W40"),
        ("2027-01-01", "2026-W53"),
    ] {
        assert_eq!(
            dvic::report_week(chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap()),
            week
        );
    }
    assert_eq!(
        dvic::weeks_ending("2027-W01", 2).unwrap(),
        ["2027-W01", "2026-W53"]
    );
}

#[test]
fn catch_up_prioritizes_unseen_weeks_before_refreshing_older_observations() {
    let (_root, db, id) = ready();
    let latest = dvic::report_week(chrono::Utc::now().date_naive());
    let weeks = dvic::weeks_ending(&latest, dvic::MAX_WEEKS).unwrap();
    let storage = db.dvic(&id).unwrap();
    for week in weeks.iter().skip(2).take(22) {
        storage.exec(
            "INSERT INTO dvic_weeks VALUES ('TST1','company-fixture',?,'2020-01-01T00:00:00Z',0)",
            [week],
        ).unwrap();
    }
    let jobs = db.dvic_jobs(&id).unwrap();
    let request = Request::parse(&jobs[0].1).unwrap().unwrap();
    assert_eq!(&request.weeks[..2], &weeks[..2]);
    assert_eq!(&request.weeks[2..], &weeks[24..]);
}

#[test]
fn a_hidden_driver_is_never_stored_and_hiding_removes_what_was() {
    let (_root, db, id) = ready();
    let mut capture = dvic::fixture(&request(&["2026-W39"])).unwrap();
    let rows = capture.reports[0].rows.as_mut().unwrap();
    rows[0].transporter_id = "A1KEPT0000001".into();
    let mut other = rows[0].clone();
    other.transporter_id = "A2HIDDEN00001".into();
    other.transporter_name = "Hidden Driver".into();
    other.vin = "1FIXTURE000000002".into();
    rows.push(other);
    publish(&db, &id, "both", &capture);
    assert_eq!(count(&db, &id, "dvic_inspections"), 2);
    let stored = |sql: &str| db.dvic(&id).unwrap().count(sql, []).unwrap();
    let theirs = "SELECT count(*) FROM dvic_inspections WHERE transporter_id='A2HIDDEN00001'";
    let copies = "SELECT count(*) FROM dvic_revisions WHERE rows LIKE '%A2HIDDEN00001%' \
                  OR rows LIKE '%Hidden Driver%'";
    let counts = || {
        db.dvic(&id)
            .unwrap()
            .all("SELECT row_count,short_count FROM dvic_reports", [])
            .unwrap()
    };

    // Hiding removes their inspection and their row from the report copy, then recounts it.
    let summary = dvic::hidden::hide(
        &db.dvic(&id).unwrap(),
        "A2HIDDEN00001",
        " Platform test ",
        "2026-10-01T00:00:00.000Z",
    )
    .unwrap();
    assert_eq!(
        summary,
        json!({"driver":"A2HIDDEN00001","inspectionsDeleted":1,"reportCopiesCleaned":1})
    );
    assert_eq!((stored(theirs), stored(copies)), (0, 0));
    assert_eq!(counts(), vec![json!({"row_count":1,"short_count":1})]);
    assert_eq!(count(&db, &id, "dvic_inspections"), 1);

    // A changed report that has them again stores none of their rows.
    capture.reports[0].sha256 = dvic::hash(b"newer bytes");
    capture.reports[0].modified_at += 1000;
    publish(&db, &id, "again", &capture);
    assert_eq!((stored(theirs), stored(copies)), (0, 0));
    assert_eq!(counts(), vec![json!({"row_count":1,"short_count":1})]);
    assert_eq!(
        dvic::hidden::list(&db.dvic(&id).unwrap()).unwrap(),
        json!([{"driver":"A2HIDDEN00001","note":"Platform test","hiddenAt":"2026-10-01T00:00:00.000Z"}])
    );

    // Shown again, their later reports are stored; nothing earlier comes back by itself.
    dvic::hidden::unhide(&db.dvic(&id).unwrap(), "A2HIDDEN00001").unwrap();
    assert_eq!(stored(theirs), 0);
    capture.reports[0].sha256 = dvic::hash(b"newest bytes");
    capture.reports[0].modified_at += 1000;
    publish(&db, &id, "shown", &capture);
    assert_eq!(stored(theirs), 1);

    let dvic = db.dvic(&id).unwrap();
    let code = |result: dispatch_backend::Result<serde_json::Value>| result.unwrap_err().code;
    assert_eq!(
        code(dvic::hidden::unhide(&dvic, "A2HIDDEN00001")),
        "driver_not_hidden"
    );
    assert_eq!(
        code(dvic::hidden::hide(&dvic, "a2hidden", "x", "t")),
        "invalid_driver_id"
    );
    assert_eq!(
        code(dvic::hidden::hide(&dvic, "A2HIDDEN00001", " ", "t")),
        "invalid_note"
    );
}

#[test]
fn the_operator_commands_hide_list_and_unhide_without_stopping_the_server() {
    let (_root, db, id) = ready();
    let run = |args: &[&str]| {
        let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
        dispatch_backend::operations::dvic_drivers(&db.config, &args)
    };
    // The server's own connection stays open throughout, as it would on a live host.
    let server = db.dvic(&id).unwrap();
    assert_eq!(run(&["dvic-hidden", &id]).unwrap(), json!([]));
    let hidden = run(&["dvic-hide", &id, "A2HIDDEN00001", "Platform test"]).unwrap();
    assert_eq!(hidden["inspectionsDeleted"], 0);
    assert_eq!(
        server
            .count("SELECT count(*) FROM dvic_hidden_drivers", [])
            .unwrap(),
        1
    );
    assert_eq!(
        run(&["dvic-hidden", &id]).unwrap()[0]["driver"],
        "A2HIDDEN00001"
    );
    run(&["dvic-unhide", &id, "A2HIDDEN00001"]).unwrap();
    assert_eq!(run(&["dvic-hidden", &id]).unwrap(), json!([]));
    let code = |args: &[&str]| run(args).unwrap_err().code;
    assert_eq!(
        code(&["dvic-hide", &id, "A2HIDDEN00001"]),
        "usage_dvic_hidden_hide_unhide"
    );
    assert_eq!(code(&["dvic-hidden", "dsp_0000"]), "invalid_dsp_id");
    assert_eq!(
        code(&["dvic-hidden", "dsp_00000000000000000000000000000000"]),
        "dsp_not_found"
    );
}
