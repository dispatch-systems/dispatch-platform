use super::*;
use dispatch_core::testing as common;

fn fixture() -> (tempfile::TempDir, Store, Dsp, Period) {
    common::install(&[&dispatch_cortex::COLLECTOR], &[&crate::FEATURE]);
    let (root, db, id) = common::bootstrapped();
    db.set_profile(&id, json!({"stationCode":"TST1","setupRequired":false}))
        .unwrap();
    let dsp = db.find_dsp(&id).unwrap();
    let data = db.dvic_db(&id).unwrap();
    data.exec(
        "INSERT INTO dvic_reports(id,company_id,dsp_code,station,source_key,name,week,\
         report_date,modified_at,sha256,revision_id,row_count,short_count,min_date,max_date,checked_at,scope_verified) \
         VALUES ('fixture','fixture','FXTR','TST1','fixture','DVIC','2026-W39',\
         '2026-09-23',0,'fixture','fixture',0,0,'2026-09-21','2026-09-23','2026-09-23',1)",
        [],
    )
    .unwrap();
    data.exec(
        "INSERT INTO dvic_revisions(id,report_id,sha256,modified_at,collected_at,rows) \
         VALUES ('fixture','fixture','fixture',0,'2026-09-23','[]')",
        [],
    )
    .unwrap();
    for (key, driver, station, verified, date, time, seconds) in [
        ("late", "A", "TST1", 1, "2026-09-22", "09:00", 91.0),
        ("alias", "A-old", "TST1", 1, "2026-09-21", "09:00", 75.4),
        ("early", "A", "TST1", 1, "2026-09-22", "08:00", 90.0),
        ("other", "B", "TST1", 1, "2026-09-22", "07:00", 300.0),
        (
            "literal",
            "A' OR 1=1 --",
            "TST1",
            1,
            "2026-09-22",
            "06:00",
            100.0,
        ),
        ("station", "A", "OTHER", 1, "2026-09-22", "09:00", 10.0),
        ("unverified", "A", "TST1", 0, "2026-09-22", "09:00", 20.0),
        ("before", "A", "TST1", 1, "2026-09-20", "09:00", 30.0),
        ("after", "A", "TST1", 1, "2026-09-24", "09:00", 40.0),
    ] {
        data.exec(
            "INSERT INTO dvic_inspections(company_id,inspection_key,dsp_code,station,start_date,\
             transporter_id,transporter_name,vin,fleet_type,inspection_type,inspection_status,\
             start_time,end_time,duration_seconds,minimum_seconds,short,report_date,\
             source_modified_at,revision_id,scope_verified) \
             VALUES ('fixture',?1,'FXTR',?2,?3,?4,?4,'VIN','CDV','PRE_TRIP','COMPLETE',\
             ?5,?5,?6,90,?7,'2026-09-23',0,'fixture',?8)",
            rusqlite::params![
                key,
                station,
                date,
                driver,
                time,
                seconds,
                i64::from(seconds < 90.0),
                verified
            ],
        )
        .unwrap();
    }
    drop(data);
    let period = Period {
        from: "2026-09-21".parse().unwrap(),
        to: "2026-09-23".parse().unwrap(),
        label: "fixture".into(),
    };
    (root, db, dsp, period)
}

#[test]
fn selected_driver_aliases_preserve_scoping_order_and_inspection_values() {
    let (_root, db, dsp, period) = fixture();
    let (all, all_coverage) = inspections(&db, &dsp, &period, None).unwrap();
    assert_eq!(all.len(), 5);
    let ids = vec!["A".into(), "A-old".into()];
    let (selected, coverage) = inspections(&db, &dsp, &period, Some(&ids)).unwrap();
    let expected: Vec<_> = all
        .iter()
        .filter(|row| ids.contains(&row.transporter_id))
        .collect();
    assert_eq!(json!(selected), json!(expected));
    assert_eq!(json!(coverage), json!(all_coverage));
    assert_eq!(coverage.status(), "complete");
    assert_eq!(
        selected.iter().map(|row| row.seconds).collect::<Vec<_>>(),
        vec![75, 90, 91]
    );
    assert!(selected[0].short);
    assert!(!selected[1].short);

    let (literal, _) = inspections(&db, &dsp, &period, Some(&["A' OR 1=1 --".into()])).unwrap();
    assert_eq!(literal.len(), 1);
    assert_eq!(literal[0].transporter_id, "A' OR 1=1 --");
}

#[test]
fn empty_or_unmatched_drivers_keep_source_coverage_without_broadening() {
    let (_root, db, dsp, mut period) = fixture();
    for ids in [vec![], vec!["unknown".into()]] {
        let (rows, coverage) = inspections(&db, &dsp, &period, Some(&ids)).unwrap();
        assert!(rows.is_empty());
        assert_eq!(coverage.status(), "complete");
        assert_eq!(coverage.days.len(), 3);
    }
    period.from = "2026-09-24".parse().unwrap();
    period.to = period.from;
    let (rows, coverage) = inspections(&db, &dsp, &period, Some(&[])).unwrap();
    assert!(rows.is_empty());
    assert_eq!(coverage.status(), "missing");
    assert!(!coverage.known());
}
