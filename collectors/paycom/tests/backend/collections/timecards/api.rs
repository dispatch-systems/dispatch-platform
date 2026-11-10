use super::*;

#[test]
fn a_directory_row_reads_as_the_pages_roster_does_with_its_station_from_the_named_category() {
    let categories = [
        json!({"catcode": 1, "catdesc": "Job"}),
        json!({"catcode": "2", "catdesc": "Delivery Station Code"}),
    ];
    assert_eq!(station_category(&categories), Some(2));
    assert_eq!(station_category(&categories[..1]), None);
    let row = json!({"eecode":"A0B1","eename":"Morgan, Avery","firstname":"Avery",
        "lastname":"Morgan","eestatus":"A","deptdesc":"Delivery","cat1":"DRV","cat2":"DCK6"});
    assert_eq!(
        employee(&row, 2).unwrap(),
        json!({"code":"A0B1","name":"Morgan, Avery","department":"Delivery","position":"",
            "station":"DCK6","active":true})
    );
    // Without the full name, the two it has; and a code a path can't carry is refused.
    let mut unnamed = row.clone();
    unnamed["eename"] = json!("");
    assert_eq!(employee(&unnamed, 2).unwrap()["name"], "Avery Morgan");
    let mut unsafe_code = row.clone();
    unsafe_code["eecode"] = json!("../x");
    assert!(
        employee(&unsafe_code, 2)
            .unwrap_err()
            .is(codes::API_UNREADABLE)
    );
    assert_eq!(
        position(&[json!({"position":"","position_title":"Driver"})]).unwrap(),
        "Driver"
    );
}

#[test]
fn a_pay_period_is_counted_in_fortnights_from_the_anchor_before_it_and_after() {
    let anchor = NaiveDate::from_ymd_opt(2026, 9, 6).unwrap();
    for (date, from, to) in [
        ("2026-09-06", "2026-09-06", "2026-09-19"),
        ("2026-09-19", "2026-09-06", "2026-09-19"),
        ("2026-09-20", "2026-09-20", "2026-10-03"),
        ("2026-09-05", "2026-08-23", "2026-09-05"),
    ] {
        let period = period_of(anchor, date.parse().unwrap());
        assert_eq!(
            (period.from.as_str(), period.to.as_str()),
            (from, to),
            "{date}"
        );
    }
}
