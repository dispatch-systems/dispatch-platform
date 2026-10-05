use super::*;
#[tokio::test(start_paused = true)]
async fn page_deadlines_keep_navigation_and_content_failure_diagnostics() -> Result<()> {
    use dispatch_core::collection::api::metrics::PageStage;
    assert_eq!(NAVIGATION_TIMEOUT, Duration::from_secs(45));
    assert_eq!(CONTENT_TIMEOUT, Duration::from_secs(30));
    let metrics = Recorder::new(&json!({"attempt":1}));
    metrics.page_start(1, 1);
    let started = Instant::now();
    tokio::time::advance(Duration::from_millis(44_999)).await;
    page_deadline(started, None, Instant::now())?;
    tokio::time::advance(Duration::from_millis(1)).await;
    let navigation = page_deadline(started, None, Instant::now()).unwrap_err();
    assert!(navigation.is(dispatch_core::Code::ProviderNavigationTimeout));
    assert_eq!(navigation.status, 504);
    assert!(navigation.is_any(PAGE_RETRY));
    metrics.page_finish(1, Some(&navigation.code));

    metrics.page_start(2, 1);
    let started = Instant::now();
    tokio::time::advance(Duration::from_secs(44)).await;
    page_deadline(started, None, Instant::now())?;
    let content = Instant::now();
    metrics.page_stage(2, "content");
    // A completed navigation gets its own content budget, even after 45s total.
    tokio::time::advance(Duration::from_millis(29_999)).await;
    page_deadline(started, Some(content), Instant::now())?;
    tokio::time::advance(Duration::from_millis(1)).await;
    let content = page_deadline(started, Some(content), Instant::now()).unwrap_err();
    assert!(content.is(dispatch_core::Code::ProviderContentTimeout));
    assert_eq!(content.status, 504);
    assert!(content.is_any(PAGE_RETRY));
    metrics.page_finish(2, Some(&content.code));

    let reads = metrics.snapshot().page_reads.unwrap();
    assert!(reads.active.is_empty());
    assert_eq!(reads.completed, 0);
    assert_eq!(reads.failures.len(), 2);
    for (failure, stage, error) in [
        (&reads.failures[0], PageStage::Navigation, navigation),
        (&reads.failures[1], PageStage::Content, content),
    ] {
        assert_eq!(failure.stage, stage);
        assert_eq!(failure.error.as_deref(), Some(error.code.as_str()));
    }
    Ok(())
}
fn body() -> Value {
    let mut body = json!({});
    for key in FIELDS {
        body[*key] = Value::Null;
    }
    body["eeCodes"] = json!(["AA01", "BB02"]);
    body["startDate"] = json!("2026-08-30");
    body["endDate"] = json!("2026-09-12");
    body["isAdvancedFilterApplied"] = json!(true);
    body["onlyBorrowedEmployees"] = json!(false);
    body
}
#[test]
fn rejects_partial_rosters_and_selects_timezone_date_period() -> Result<()> {
    let (selected, period, _) = selected_body(&body(), date("2026-09-16")?)?;
    assert_eq!(period["start"], "2026-09-13");
    assert_eq!(period["end"], "2026-09-26");
    assert_eq!(selected["isAdvancedFilterApplied"], false);
    let (_, historical, _) = selected_body(&body(), date("2026-01-10")?)?;
    assert_eq!(historical["start"], "2026-01-04");
    assert_eq!(historical["end"], "2026-01-17");
    for (key, value) in [
        ("q", json!("driver")),
        ("take", json!(1)),
        ("skip", json!(1)),
        ("onlyBorrowedEmployees", json!(true)),
        ("isAdvancedFilterApplied", Value::Null),
        ("eeCodes", json!(["AA01", "aa01"])),
    ] {
        let mut body = body();
        body[key] = value;
        assert!(selected_body(&body, date("2026-09-16")?).is_err());
    }
    assert!(codes(&json!(["AA01", "aa01"])).is_err());
    Ok(())
}
#[test]
fn requested_days_keep_the_cycle_observed_in_paycom() -> Result<()> {
    let mut observed = body();
    observed["startDate"] = json!("2026-09-06");
    observed["endDate"] = json!("2026-09-19");
    for (day, from, to) in [
        ("2026-09-05", "2026-08-23", "2026-09-05"),
        ("2026-09-06", "2026-09-06", "2026-09-19"),
        ("2026-09-19", "2026-09-06", "2026-09-19"),
        ("2026-09-20", "2026-09-20", "2026-10-03"),
        ("2025-12-31", "2025-12-28", "2026-01-10"),
        ("2026-03-08", "2026-03-08", "2026-03-21"),
        ("2028-02-29", "2028-02-20", "2028-03-04"),
    ] {
        let (selected, period, _) = selected_body(&observed, date(day)?)?;
        assert_eq!(selected["startDate"], from);
        assert_eq!(selected["endDate"], to);
        assert_eq!(period["dates"].as_array().unwrap().len(), 14);
        assert_eq!(period["dates"][0], from);
        assert_eq!(period["dates"][13], to);
    }
    Ok(())
}
#[test]
fn projection_reconciles_additional_totals_without_double_counting() -> Result<()> {
    let days = (0..14)
        .map(|i| {
            json!({"date":format!("day{i}"),"hours":if i==0{8}else{0},"totalHours":null,
        "missingPunch":false,"punches":[]})
        })
        .collect::<Vec<_>>();
    let mut record = json!({"days":days,"additionalRows":[{"date":"day0","hours":2,"totalHours":10}],
        "weeklyTotals":[10,0],"periodTotalHours":10});
    assert_eq!(project(&record, "AA01")?[0]["hours"], 10.);
    record["periodTotalHours"] = json!(11);
    assert!(project(&record, "AA01").is_err());
    Ok(())
}
#[test]
fn mixed_pay_code_totals_and_cross_row_punches_remain_exact() -> Result<()> {
    let days = (0..14)
        .map(|i| {
            json!({"date":format!("day{i}"),"hours":null,"totalHours":null,
        "missingPunch":false,"punches":[]})
        })
        .collect::<Vec<_>>();
    let mut record = json!({"days":days,"additionalRows":[
        {"date":"day0","hours":2,"totalHours":null},
        {"date":"day2","hours":1.5,"totalHours":8},
        {"date":"day7","hours":0.75,"totalHours":0.75},
        {"date":"day10","hours":0.5,"totalHours":null}
    ],"weeklyTotals":[18,7.5],"periodTotalHours":25.5});
    for (i, hours, total) in [
        (0, 8., Some(10.)),
        (2, 6.5, None),
        (7, 4.25, Some(4.25)),
        (10, 2., None),
    ] {
        record["days"][i]["hours"] = json!(hours);
        record["days"][i]["totalHours"] = json!(total);
    }
    let projected = project(&record, "AA01")?;
    assert_eq!(
        projected
            .iter()
            .map(|d| d["hours"].as_f64().unwrap())
            .collect::<Vec<_>>(),
        vec![10., 0., 8., 0., 0., 0., 0., 5., 0., 0., 2.5, 0., 0., 0.]
    );
    record["weeklyTotals"] = json!([17, 8.5]);
    assert_eq!(
        project(&record, "AA01").unwrap_err().code,
        "provider_hours_mismatch"
    );
    record["weeklyTotals"] = json!([18, 7.5]);
    record["periodTotalHours"] = json!(24.5);
    assert_eq!(
        project(&record, "AA01").unwrap_err().code,
        "provider_hours_mismatch"
    );
    record["periodTotalHours"] = json!(25.5);
    record["days"][0]["missingPunch"] = json!(true);
    record["days"][0]["punches"] = json!([{"slot":"i1","rowIndex":0,"displayTime":"08:00 \
        AM"},{"slot":"o1","rowIndex":1,"displayTime":"12:00 PM"}]);
    let projected = project(&record, "AA01")?;
    assert_eq!(projected[0]["status"], "Missing punch");
    assert_eq!(
        projected[0]["punches"],
        json!([{"in":"08:00 AM","out":null,"hours":null,"inKind":null,"outKind":null},{"in":null,
            "out":"12:00 PM","hours":null,"inKind":null,"outKind":null}])
    );
    Ok(())
}
#[test]
fn blank_leading_row_uses_additional_totals_and_preserves_punches() -> Result<()> {
    let days = (0..14)
        .map(|i| {
            json!({"date":format!("day{i}"),"hours":null,"totalHours":null,
        "missingPunch":false,"unresolvedSlots":[],"punches":[]})
        })
        .collect::<Vec<_>>();
    let mut record = json!({"days":days,"additionalRows":[{"date":"day0","hours":2,"totalHours":4}],
        "weeklyTotals":[4,0],"periodTotalHours":4});
    record["days"][0]["punches"] = json!([
        {"slot":"i1","rowIndex":1,"displayTime":"08:00 AM"},
        {"slot":"o1","rowIndex":1,"displayTime":"10:00 AM"}
    ]);
    let day = &project(&record, "AA01")?[0];
    assert_eq!(day["hours"], 4.);
    assert_eq!(day["status"], "Complete");
    assert_eq!(
        day["punches"],
        json!([{"in":"08:00 AM","out":"10:00 AM","hours":null,"inKind":null,"outKind":null}])
    );

    record["days"][0]["punches"][0]["kind"] = json!("IN DAY");
    record["days"][0]["punches"][1]["kind"] = json!("OUT LUNCH");
    assert_eq!(
        project(&record, "AA01")?[0]["punches"][0]["outKind"],
        "OUT LUNCH"
    );
    assert_eq!(
        project(&record, "AA01")?[0]["punches"][0]["inKind"],
        "IN DAY"
    );

    // An unresolved punch on the additional row is still reported as such.
    record["days"][0]["missingPunch"] = json!(true);
    record["days"][0]["unresolvedSlots"] = json!(["1:o2"]);
    assert_eq!(project(&record, "AA01")?[0]["status"], "Missing punch");

    // Equal period totals cannot conceal hours in the wrong week.
    record["weeklyTotals"] = json!([0, 4]);
    assert_eq!(
        project(&record, "AA01").unwrap_err().code,
        "provider_hours_mismatch"
    );
    record["weeklyTotals"] = json!([4, 0]);

    // Missing hours on an occupied leading row must still fail closed.
    record["days"][0]["unresolvedSlots"] = json!(["o2"]);
    assert_eq!(
        project(&record, "AA01").unwrap_err().code,
        "invalid_timecard_hours"
    );
    record["days"][0]["missingPunch"] = json!(false);
    record["days"][0]["unresolvedSlots"] = json!([]);
    record["days"][0]["punches"][0]["rowIndex"] = json!(0);
    assert_eq!(
        project(&record, "AA01").unwrap_err().code,
        "invalid_timecard_hours"
    );
    Ok(())
}
