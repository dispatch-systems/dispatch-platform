use super::*;
use serde_json::json;

// Consecutive days are one range; a gap starts another.
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
}

// A range's rows go back to the days they name, every day of the range answered, even one
// with none; a dataset whose rows name no day, as the thresholds, answers every day alike;
// a row naming no day among dated rows, or a day outside the range, can't be split.
#[test]
fn a_range_splits_into_the_days_its_rows_name() {
    let row = |date: &str, n: i64| json!({"data_date": date, "transporter_id": "driver-1", "n": n});
    let split_rows = split(
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

    let thresholds = vec![json!({"metric": "dcr", "fantastic": 99.5})];
    let every = split(&thresholds, "2026-10-12", "2026-10-13").unwrap();
    assert_eq!(every["2026-10-12"], thresholds);
    assert_eq!(every["2026-10-13"], thresholds);
    assert!(split(&[], "2026-10-12", "2026-10-13").unwrap()["2026-10-13"].is_empty());

    assert!(split(&[row("2026-10-15", 1)], "2026-10-12", "2026-10-14").is_none());
    assert!(
        split(
            &[row("2026-10-12", 1), json!({"n": 2})],
            "2026-10-12",
            "2026-10-14"
        )
        .is_none()
    );
}
