use super::*;
#[test]
fn days_and_ids_are_refused_past_their_limits() {
    let day = |value| NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap();
    // A week at most: seven days are read, an eighth or a range running backward refused.
    assert_eq!(
        days("2026-09-01", "2026-09-07", 7).unwrap(),
        (day("2026-09-01"), day("2026-09-07"))
    );
    for (from, to, code) in [
        ("2026-09-01", "2026-02-30", "invalid_date"),
        ("2026-09-02", "2026-09-01", "invalid_input"),
        ("2026-09-01", "2026-09-08", "invalid_input"),
    ] {
        assert_eq!(days(from, to, 7).unwrap_err().code, code, "{from} {to}");
    }
    // Three IDs at most, each 1 to 128 characters however many bytes they take; none is fine.
    let three: Vec<String> = (0..3).map(|n| n.to_string()).collect();
    let id = |length| "é".repeat(length);
    let refused = Some("invalid_input");
    for (asked, code) in [
        (vec![], None),
        (vec![id(128)], None),
        (three.clone(), None),
        (vec![String::new()], refused),
        (vec![id(129)], refused),
        ([three, vec![id(1)]].concat(), refused),
    ] {
        let error = ids(&asked, 3).err().map(|error| error.code);
        assert_eq!(error.as_deref(), code, "{asked:?}");
    }
}
