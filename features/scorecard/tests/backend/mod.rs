use super::*;
use crate::collectors::cortex::scorecard::{dataset, fixture};
#[test]
fn requests_are_recognized_by_their_collection() {
    assert!(
        Request::parse(&json!({"date":"2026-09-13"}))
            .unwrap()
            .is_none()
    );
    let request =
        Request::parse(&json!({"collection":"scorecard","week":"2026-W38","station":"TST1",
            "timezone":"America/Los_Angeles","dspName":"Fixture Delivery","dspAbbreviation":"FXTR"}))
            .unwrap()
            .unwrap();
    assert_eq!(
        request
            .interval(dataset("da_dsp_daily_psb_stop").unwrap())
            .unwrap()
            .0,
        "2026-09-13"
    );
    assert!(
        Request::parse(&json!({"collection":"scorecard","week":"2026-W38","station":"tst1",
            "timezone":"America/Los_Angeles","dspName":"Fixture Delivery","dspAbbreviation":"FXTR"}))
            .is_err()
    );
    assert!(
        Request::parse(
            &json!({"collection":"scorecard","week":"2026-W38","station":"TST1",
                "timezone":"America/Los_Angeles","dspName":"Fixture Delivery",
                "dspAbbreviation":"FXTR","extra":1})
        )
        .is_err()
    );
}
#[test]
fn the_fixture_is_a_valid_posted_week_and_its_keys_are_read_from_rows() {
    let request =
        Request::parse(&json!({"collection":"scorecard","week":"2026-W38","station":"TST1",
            "timezone":"America/Los_Angeles","dspName":"Fixture Delivery","dspAbbreviation":"FXTR"}))
            .unwrap()
            .unwrap();
    let capture = fixture(&request).unwrap();
    assert!(capture.posted);
    assert_eq!(capture.datasets.len(), DATASETS.len());
    let returns = dataset("da_dsp_weekly_rts_deep_dive").unwrap();
    let rows = &capture
        .datasets
        .iter()
        .find(|d| d.id == returns.id)
        .unwrap()
        .rows;
    let first = keys(returns, &rows[0]);
    assert_eq!(first.impact, Some(1));
    assert_eq!(first.tracking_id.as_deref(), Some("TBA000000000001"));
    assert_eq!(keys(returns, &rows[1]).impact, Some(0));
    let concessions = dataset("da_dsp_station_daily_dsb_dnr_tba").unwrap();
    let rows = &capture
        .datasets
        .iter()
        .find(|d| d.id == concessions.id)
        .unwrap()
        .rows;
    assert_eq!(keys(concessions, &rows[0]).impact, Some(1));
    assert_eq!(
        keys(concessions, &rows[0]).data_date.as_deref(),
        Some("2026-09-13")
    );
    let mut other = capture.clone();
    other.posted = false;
    assert!(other.validate(&request).is_err());
}
