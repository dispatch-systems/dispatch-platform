use super::*;
use crate::daily_performance::dataset;
use serde_json::json;

// Consecutive days are one range of at most a week; a gap starts another.
#[test]
fn days_group_into_runs_of_consecutive_days() {
    let dates = |list: &[&str]| list.iter().map(|d| (*d).to_owned()).collect::<Vec<_>>();
    let pair = |a: &str, b: &str| (a.to_owned(), b.to_owned());
    assert_eq!(
        runs(&dates(&["2026-10-12", "2026-10-13", "2026-10-14"])).unwrap(),
        vec![pair("2026-10-12", "2026-10-14")]
    );
    assert_eq!(
        runs(&dates(&[
            "2026-09-30",
            "2026-10-01",
            "2026-10-03",
            "2026-10-05"
        ]))
        .unwrap(),
        vec![
            pair("2026-09-30", "2026-10-01"),
            pair("2026-10-03", "2026-10-03"),
            pair("2026-10-05", "2026-10-05"),
        ]
    );
    let ten: Vec<String> = (1..=10).map(|day| format!("2026-09-{day:02}")).collect();
    assert_eq!(
        runs(&ten).unwrap(),
        vec![
            pair("2026-09-01", "2026-09-07"),
            pair("2026-09-08", "2026-09-10")
        ]
    );
}

// A range's rows go back to the days they name, every day of the range answered, even one
// with none; the thresholds, which name no station or day, answer every day alike; a
// station's row naming no day, or a day outside the range, can't be split; one day's own
// read is that day's as it came.
#[test]
fn a_range_splits_into_the_days_its_rows_name() {
    let station = dataset("da_dsp_daily_rts_deep_dive").unwrap();
    let thresholds = dataset("da_daily_thresholds").unwrap();
    let row = |date: &str, n: i64| json!({"data_date": date, "transporter_id": "driver-1", "n": n});
    let split_rows = split(
        station,
        &[
            row("2026-10-12", 1),
            row("2026-10-14T00:00:00", 2),
            row("2026-10-12", 3),
        ],
        "2026-10-12",
        "2026-10-14",
    )
    .unwrap();
    assert_eq!(
        split_rows.keys().collect::<Vec<_>>(),
        ["2026-10-12", "2026-10-13", "2026-10-14"]
    );
    assert_eq!(
        split_rows["2026-10-12"],
        vec![row("2026-10-12", 1), row("2026-10-12", 3)]
    );
    assert!(split_rows["2026-10-13"].is_empty());
    assert_eq!(
        split_rows["2026-10-14"],
        vec![row("2026-10-14T00:00:00", 2)]
    );

    let limits = vec![json!({"metric": "dcr", "fantastic": 99.5})];
    let every = split(thresholds, &limits, "2026-10-12", "2026-10-13").unwrap();
    assert_eq!(every["2026-10-12"], limits);
    assert_eq!(every["2026-10-13"], limits);
    let empty = split(station, &[], "2026-10-12", "2026-10-13").unwrap();
    assert!(empty["2026-10-12"].is_empty() && empty["2026-10-13"].is_empty());

    assert!(split(station, &[row("2026-10-15", 1)], "2026-10-12", "2026-10-14").is_none());
    let undated = json!({"n": 2});
    assert!(
        split(
            station,
            &[row("2026-10-12", 1), undated.clone()],
            "2026-10-12",
            "2026-10-14"
        )
        .is_none()
    );
    // A station's rows that name no day never go to every day of a range.
    assert!(
        split(
            station,
            std::slice::from_ref(&undated),
            "2026-10-12",
            "2026-10-14"
        )
        .is_none()
    );
    let alone = split(
        station,
        &[undated.clone(), row("2026-10-11", 3)],
        "2026-10-12",
        "2026-10-12",
    )
    .unwrap();
    assert_eq!(alone["2026-10-12"], vec![undated, row("2026-10-11", 3)]);
}
