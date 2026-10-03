#[path = "../../../../../core/db/tests/support/common.rs"]
mod common;
use dispatch_backend::{
    collectors::cortex::{
        self,
        discovery::{CollectionRequest, Scope},
        dvic::{self, Capture, Request},
    },
    db::{self, Store, s},
    dvic::{DvicStore, hidden, weeks_ending},
};
use serde_json::json;

fn request(weeks: &[&str]) -> Request {
    // Publication binds the DSP's current local date, so the expected scope must too.
    let date = chrono::Utc::now()
        .with_timezone(&chrono_tz::America::Los_Angeles)
        .date_naive()
        .to_string();
    Request::parse(&json!({"collection":"dvic","station":"TST1","weeks":weeks,
        "date":date,"timezone":"America/Los_Angeles",
        "dspName":"Fixture Delivery","dspAbbreviation":"FXTR"}))
    .unwrap()
    .unwrap()
}
fn job_scope(db: &Store, id: &str, job: &str) -> Scope {
    let queued: serde_json::Value =
        serde_json::from_str(&db.job_row(job, Some(id)).unwrap().request).unwrap();
    let weeks = queued["weeks"].as_array().unwrap();
    let weeks = weeks
        .iter()
        .map(|week| week.as_str().unwrap())
        .collect::<Vec<_>>();
    let request = request(&weeks);
    match request.scope_request() {
        CollectionRequest::Discover(discovery) => {
            discovery.scope("area-fixture", "company-fixture").unwrap()
        }
        CollectionRequest::Scoped(scope) => scope,
    }
}
fn publish_capture(
    db: &Store,
    id: &str,
    job: &str,
    capture: &Capture,
) -> dispatch_backend::Result<()> {
    db.publish_dvic(id, job, capture, &job_scope(db, id, job))
}
fn ready() -> (tempfile::TempDir, Store, String) {
    let (root, db, id) = common::bootstrapped();
    db.platform
        .exec(
            "UPDATE dsps SET name='Fixture Delivery',timezone='America/Los_Angeles' WHERE id=?",
            [&id],
        )
        .unwrap();
    db.set_profile(
        &id,
        json!({"stationCode":"TST1","abbreviation":"FXTR","setupRequired":false}),
    )
    .unwrap();
    db.collector(&id, cortex::PROVIDER)
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
    publish_capture(db, id, &job, capture).unwrap();
    // The worker normally transitions the job after publication.
    db.jobs
        .exec("UPDATE jobs SET status='succeeded' WHERE id=?", [&job])
        .unwrap();
    job
}
fn count(db: &Store, id: &str, table: &str) -> i64 {
    db.dvic_db(id)
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
    publish_capture(&db, &id, &first, &newer).unwrap();
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
        .dvic_db(&id)
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
    assert!(publish_capture(&db, &id, s(&job, "id"), &bad).is_err());
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
    assert_eq!(jobs[0].1.as_object().unwrap().len(), 3);
    let weeks = jobs[0].1["weeks"].as_array().unwrap();
    assert_eq!(weeks.len(), 4);
    assert_eq!(weeks[0], dvic::report_week(chrono::Utc::now().date_naive()));
}

#[test]
fn publication_rejects_wrong_scope_and_reads_are_paginated_and_isolated() {
    let (_root, db, id) = ready();
    let mut capture = dvic::fixture(&request(&["2026-W39", "2026-W38"])).unwrap();
    let job = db
        .enqueue_dvic(&id, None, "scope", Some("2026-W39"), 2)
        .unwrap();
    capture.reports[1].rows.as_mut().unwrap()[0].station = "OTHER".into();
    assert!(publish_capture(&db, &id, s(&job, "id"), &capture).is_err());
    assert_eq!(count(&db, &id, "dvic_reports"), 0);
    capture.reports[1].rows.as_mut().unwrap()[0].station = "TST1".into();
    capture.company_id = "company-foreign".into();
    assert!(publish_capture(&db, &id, s(&job, "id"), &capture).is_err());
    capture.company_id = "company-fixture".into();
    capture.dsp_code = "FOREIGN".into();
    assert!(publish_capture(&db, &id, s(&job, "id"), &capture).is_err());
    capture.dsp_code = "FXTR".into();
    publish_capture(&db, &id, s(&job, "id"), &capture).unwrap();
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
fn legacy_unverified_reports_and_inspections_stay_quarantined() {
    let (_root, db, id) = ready();
    let storage = db.dvic_db(&id).unwrap();
    storage.exec(
        "INSERT INTO dvic_reports(id,company_id,dsp_code,station,source_key,name,week,report_date,\
         modified_at,sha256,revision_id,row_count,short_count,min_date,max_date,checked_at) VALUES \
         ('legacy-report','company-foreign','FOREIGN','TST1','legacy-key','Foreign DVIC','2026-W39',\
         '2026-09-27',1,'legacy-sha','legacy-revision',1,1,'2026-09-20','2026-09-20',\
         '2026-09-27T00:00:00Z')",
        [],
    ).unwrap();
    storage
        .exec(
            "INSERT INTO dvic_revisions(id,report_id,sha256,modified_at,collected_at,rows) VALUES \
         ('legacy-revision','legacy-report','legacy-sha',1,'2026-09-27T00:00:00Z','[]')",
            [],
        )
        .unwrap();
    storage.exec(
        "INSERT INTO dvic_inspections(company_id,inspection_key,dsp_code,station,start_date,\
         transporter_id,transporter_name,vin,fleet_type,inspection_type,inspection_status,start_time,\
         end_time,duration_seconds,minimum_seconds,short,report_date,source_modified_at,revision_id) VALUES \
         ('company-foreign','legacy-inspection','FOREIGN','TST1','2026-09-20','foreign-driver',\
         'Foreign Driver','FOREIGNVIN0000001','CDV','PRE_TRIP','COMPLETE','07:00','07:00',1,90,1,\
         '2026-09-27',1,'legacy-revision')",
        [],
    ).unwrap();
    storage
        .exec(
            "INSERT INTO dvic_weeks(station,company_id,week,checked_at,report_count) VALUES \
         ('TST1','company-foreign','2026-W39','2026-09-27T00:00:00Z',1)",
            [],
        )
        .unwrap();

    let status = db.dvic_status(&id).unwrap();
    assert_eq!(status.short_inspections, 0);
    assert!(status.reports.is_empty());
    assert!(status.weeks.is_empty());
    assert!(
        db.dvic_inspections(&id, "2026-09-01", "2026-09-30", None, "", 100)
            .unwrap()
            .inspections
            .is_empty()
    );

    let capture = dvic::fixture(&request(&["2026-W39"])).unwrap();
    let source_key = &capture.reports[0].source_key;
    let matching_id = dvic::hash(
        serde_json::to_string(&["company-fixture", "TST1", source_key])
            .unwrap()
            .as_bytes(),
    );
    storage.exec(
        "INSERT INTO dvic_reports(id,company_id,dsp_code,station,source_key,name,week,report_date,\
         modified_at,sha256,revision_id,row_count,short_count,checked_at) VALUES \
         (?1,'company-fixture','FXTR','TST1',?2,'Poisoned matching DVIC','2026-W39','2026-09-27',\
         9223372036854775807,'poisoned-sha','poisoned-revision',0,0,'2026-09-27T00:00:00Z')",
        rusqlite::params![matching_id, source_key],
    ).unwrap();
    storage
        .exec(
            "INSERT INTO dvic_revisions(id,report_id,sha256,modified_at,collected_at,rows) VALUES \
         ('poisoned-revision',?,'poisoned-sha',9223372036854775807,'2026-09-27T00:00:00Z','[]')",
            [&matching_id],
        )
        .unwrap();
    let expected = &capture.reports[0].rows.as_ref().unwrap()[0];
    storage
        .exec(
            "INSERT INTO dvic_inspections(company_id,inspection_key,dsp_code,station,start_date,\
             transporter_id,transporter_name,vin,fleet_type,inspection_type,inspection_status,start_time,\
             end_time,duration_seconds,minimum_seconds,short,report_date,source_modified_at,revision_id) VALUES \
             ('company-fixture',?,'FXTR','TST1',?,'driver-1','Poisoned Driver','1FIXTURE000000001',\
             'CV','PRE_TRIP_DVIC','PASSED',?,?,75,90,1,'2099-01-01',9223372036854775807,'poisoned-revision')",
            rusqlite::params![
                expected.key(),
                expected.start_date,
                expected.start_time,
                expected.end_time,
            ],
        )
        .unwrap();

    publish(&db, &id, "verified-after-legacy", &capture);
    let visible = db.dvic_status(&id).unwrap();
    assert_eq!(visible.short_inspections, 1);
    assert_eq!(visible.reports.len(), 1);
    assert_eq!(visible.weeks.len(), 1);
    assert_eq!(
        db.dvic_inspections(&id, "2026-09-01", "2026-09-30", None, "", 100)
            .unwrap()
            .inspections[0]
            .driver_name,
        "Fixture Driver"
    );
    assert_eq!(
        storage
            .all(
                "SELECT company_id,scope_verified FROM dvic_reports ORDER BY company_id",
                [],
            )
            .unwrap(),
        vec![
            json!({"company_id":"company-fixture","scope_verified":1}),
            json!({"company_id":"company-foreign","scope_verified":0}),
        ]
    );
}

#[test]
fn the_status_keeps_its_jobs_however_many_of_other_kinds_came_since() {
    let (_root, db, id) = ready();
    let job = db.enqueue_dvic(&id, None, "busy", None, 2).unwrap();
    let job = s(&job, "id").to_owned();
    // A busy DSP: more newer jobs of another kind than the latest 200 hold.
    let transaction = db.jobs.0.unchecked_transaction().unwrap();
    for index in 0..201 {
        let other = format!("paycom-{index}");
        db.jobs.exec("INSERT INTO jobs(id,dsp_id,environment,kind,status,available_at,created_at,release,connection_revision,idempotency_key) VALUES (?,?,'preview','paycom.collect','succeeded',0,?,'test',1,?)",rusqlite::params![other,id,db::at(db::now()+1000+index),other]).unwrap();
    }
    transaction.commit().unwrap();
    let listed = |jobs: Vec<dispatch_backend::contracts::PublicJob>| {
        jobs.into_iter().map(|j| j.id).collect::<Vec<_>>()
    };
    assert!(!listed(db.recent_jobs(Some(&id)).unwrap()).contains(&job));
    assert_eq!(listed(db.dvic_status(&id).unwrap().jobs), [job]);
    // The other way round: the Timecard's list keeps its own jobs however many DVIC jobs
    // came since.
    let transaction = db.jobs.0.unchecked_transaction().unwrap();
    for index in 0..201 {
        let other = format!("dvic-{index}");
        db.jobs.exec("INSERT INTO jobs(id,dsp_id,environment,kind,status,available_at,created_at,release,connection_revision,idempotency_key) VALUES (?,?,'preview','cortex.dvic.collect','succeeded',0,?,'test',1,?)",rusqlite::params![other,id,db::at(db::now()+5000+index),other]).unwrap();
    }
    transaction.commit().unwrap();
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
        weeks_ending("2027-W01", 2).unwrap(),
        ["2027-W01", "2026-W53"]
    );
}

#[test]
fn catch_up_prioritizes_unseen_weeks_before_refreshing_older_observations() {
    let (_root, db, id) = ready();
    let latest = dvic::report_week(chrono::Utc::now().date_naive());
    let weeks = weeks_ending(&latest, dvic::MAX_WEEKS).unwrap();
    let storage = db.dvic_db(&id).unwrap();
    for week in weeks.iter().skip(2).take(22) {
        storage.exec(
            "INSERT INTO dvic_weeks(station,company_id,week,checked_at,report_count,scope_verified) \
             VALUES ('TST1','company-fixture',?,'2020-01-01T00:00:00Z',0,1)",
            [week],
        ).unwrap();
    }
    let jobs = db.dvic_jobs(&id).unwrap();
    let requested = jobs[0].1["weeks"].as_array().unwrap();
    assert_eq!(requested[..2], weeks[..2]);
    assert_eq!(requested[2..], weeks[24..]);
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
    let stored = |sql: &str| db.dvic_db(&id).unwrap().count(sql, []).unwrap();
    let theirs = "SELECT count(*) FROM dvic_inspections WHERE transporter_id='A2HIDDEN00001'";
    let copies = "SELECT count(*) FROM dvic_revisions WHERE rows LIKE '%A2HIDDEN00001%' \
                  OR rows LIKE '%Hidden Driver%'";
    let counts = || {
        db.dvic_db(&id)
            .unwrap()
            .all("SELECT row_count,short_count FROM dvic_reports", [])
            .unwrap()
    };

    // Hiding removes their inspection and their row from the report copy, then recounts it.
    let summary = hidden::hide(
        &db.dvic_db(&id).unwrap(),
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
        hidden::list(&db.dvic_db(&id).unwrap()).unwrap(),
        json!([{"driver":"A2HIDDEN00001","note":"Platform test","hiddenAt":"2026-10-01T00:00:00.000Z"}])
    );

    // Shown again, their later reports are stored; nothing earlier comes back by itself.
    hidden::unhide(&db.dvic_db(&id).unwrap(), "A2HIDDEN00001").unwrap();
    assert_eq!(stored(theirs), 0);
    capture.reports[0].sha256 = dvic::hash(b"newest bytes");
    capture.reports[0].modified_at += 1000;
    publish(&db, &id, "shown", &capture);
    assert_eq!(stored(theirs), 1);

    let dvic = db.dvic_db(&id).unwrap();
    let code = |result: dispatch_backend::Result<serde_json::Value>| result.unwrap_err().code;
    assert_eq!(
        code(hidden::unhide(&dvic, "A2HIDDEN00001")),
        "driver_not_hidden"
    );
    assert_eq!(
        code(hidden::hide(&dvic, "a2hidden", "x", "t")),
        "invalid_driver_id"
    );
    assert_eq!(
        code(hidden::hide(&dvic, "A2HIDDEN00001", " ", "t")),
        "invalid_note"
    );
}

#[test]
fn the_operator_commands_hide_list_and_unhide_without_stopping_the_server() {
    let (_root, db, id) = ready();
    let run = |args: &[&str]| {
        let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
        dispatch_backend::dvic::cli::run(&db.config, &args)
    };
    // The server's own connection stays open throughout, as it would on a live host.
    let server = db.dvic_db(&id).unwrap();
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
