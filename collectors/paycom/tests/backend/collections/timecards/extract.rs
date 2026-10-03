use super::*;
use serde_json::json;

const COLUMNS: &[&str] = &[
    "date",
    "paycode",
    "i1",
    "allocation1",
    "o1",
    "i2",
    "allocation2",
    "o2",
    "hours",
    "total_hours",
    "amount",
    "exception-points",
    "comment",
    "missing-punch",
    "delete",
];
fn period() -> Value {
    let dates = (0..14)
        .map(|i| {
            (NaiveDate::from_ymd_opt(2026, 9, 13).unwrap() + chrono::Duration::days(i)).to_string()
        })
        .collect::<Vec<_>>();
    json!({"start":"2026-09-13","end":"2026-09-26","key":"2026-09-13_2026-09-26","dates":dates})
}
fn row(cells: &[(&str, &str)]) -> String {
    let cell = |name: &str| {
        cells
            .iter()
            .find(|(n, _)| *n == name)
            .map_or("", |(_, v)| *v)
    };
    format!(
        "<tr>{}</tr>",
        COLUMNS
            .iter()
            .map(|c| format!("<td>{}</td>", cell(c)))
            .collect::<String>()
    )
}
fn punch(time: &str) -> String {
    format!(
        concat!(
            r#"<span class="current-timecard-cell">{time}</span>"#,
            r#"<div class="readOnly-combined-cell" style="display:none">{time} edited</div>"#
        ),
        time = time
    )
}
/// A page shaped like Paycom's: Sundays worked 8 hours, a weekly total per week.
/// `sunday` replaces the first Sunday's rows.
fn page(code: &str, doctype: &str, sunday: Option<String>) -> String {
    let mut rows = String::new();
    for index in 0..14 {
        let date = NaiveDate::from_ymd_opt(2026, 9, 13).unwrap() + chrono::Duration::days(index);
        let heading = format!("{} ({})", LABELS[index as usize % 7], date.format("%m/%d"));
        match (&sunday, index) {
            (Some(custom), 0) => rows.push_str(&custom.replace("HEADING", &heading)),
            _ if index % 7 == 0 => rows.push_str(&row(&[
                ("date", &heading),
                ("paycode", "REG"),
                ("i1", &punch("08:00 AM")),
                ("o1", &punch("04:00 PM")),
                ("hours", "8"),
                ("total_hours", "8"),
            ])),
            _ => rows.push_str(&row(&[("date", &heading), ("hours", "0")])),
        }
        if index % 7 == 6 {
            rows.push_str("<tr><td>Weekly Totals</td><td>8</td></tr>");
        }
    }
    format!(
        r#"{doctype}<html><head><title>Timecard Editor</title></head><body>
            <input name="firstrefno" type="hidden" value="{code}">
            <table id="tbltimesheet"><thead><tr>{}</tr></thead><tbody>{rows}</tbody></table>
            <table id="approvals-table"><tbody><tr><td>No Records Found</td></tr></tbody></table>
            </body></html>"#,
        COLUMNS
            .iter()
            .map(|c| format!(r#"<th data-column="{c}">{c}</th>"#))
            .collect::<String>()
    )
}
fn read(html: &str) -> Read<Value> {
    let period = period();
    timecard(
        html,
        &Source {
            employee: "AA01",
            period: &period,
            url: "https://www.paycomonline.net/timecard",
        },
    )
}

#[test]
fn reads_the_shown_punch_times_and_reconciles_them_into_cards() {
    let record = read(&page("AA01", "<!doctype html>", None)).unwrap();
    assert_eq!(record["pageTitle"], "Timecard Editor");
    assert_eq!(record["weeklyTotals"], json!([8.0, 8.0]));
    assert_eq!(record["periodTotalHours"], json!(16.0));
    assert_eq!(record["approvals"], json!([]));
    let sunday = &record["days"][0];
    assert_eq!(sunday["date"], "2026-09-13");
    assert_eq!(sunday["label"], "SUN");
    assert_eq!(sunday["punches"][0]["displayTime"], "08:00 AM");
    assert_eq!(sunday["punches"][1]["slot"], "o1");
    assert_eq!(sunday["punches"][1]["ordinal"], 2);
    let cards = super::super::collection::project(&record, "AA01").unwrap();
    assert_eq!(cards.len(), 14);
    assert_eq!(cards[0]["hours"], json!(8.0));
    assert_eq!(
        cards[0]["punches"],
        json!([{"in":"08:00 AM","out":"04:00 PM","hours":null,"inKind":null,"outKind":null}])
    );
    assert_eq!(cards[1]["status"], "No punches");
}

#[test]
fn folds_a_following_pay_code_row_into_its_day() {
    let sunday = row(&[("date", "HEADING")])
        + &row(&[
            ("paycode", "REG"),
            ("i1", &punch("8:00 AM")),
            ("o1", "??"),
            ("hours", "8"),
            ("total_hours", "8"),
        ]);
    let record = read(&page("AA01", "<!doctype html>", Some(sunday.clone()))).unwrap();
    let day = &record["days"][0];
    assert_eq!(day["punches"][0]["rowIndex"], 1);
    assert_eq!(day["punches"][0]["displayTime"], "08:00 AM");
    assert_eq!(day["unresolvedSlots"], json!(["1:o1"]));
    assert_eq!(day["missingPunch"], true);
    assert_eq!(record["additionalRows"][0]["punchOrdinals"], json!([1]));
    assert_eq!(
        record["additionalRows"][0]["unresolvedSlots"],
        json!(["o1"])
    );
}

#[test]
fn reads_punch_history_and_change_requests_from_their_markup() {
    let history = "IN DAY&lt;br&gt;Actual: 07:58 AM&lt;br/&gt;Rounded: 08:00 AM<br>Clock: \
        Front  Door (12)<br>Comment: on&amp;time";
    // Paycom keeps the shown time a direct child of the cell; history and requests
    // sit beside it.
    let pending = format!(
        concat!(
            r#"{}<i class="pcrPending" title="Operation: Edit&lt;br&gt;"#,
            r#"Current Kind: out break&lt;br&gt;Current Time: 04:00 pm&lt;br&gt;"#,
            r#"Requested Kind: OUT LUNCH&lt;br&gt;Requested Time: 04:30 PM"></i>"#,
            r#"<b title="OUT DAY&lt;br&gt;Actual: 04:01 PM&lt;br&gt;Rounded: 04:00 PM"></b>"#
        ),
        punch("04:00 PM")
    );
    let sunday = row(&[
        ("date", "HEADING"),
        (
            "i1",
            &format!(r#"{}<i title="{history}"></i>"#, punch("08:00 AM")),
        ),
        ("o1", &pending),
        ("hours", "8"),
        ("total_hours", "8"),
        ("comment", r#"<i title="Comment:  Late start"></i>"#),
    ]);
    let record = read(&page("AA01", "<!doctype html>", Some(sunday.clone()))).unwrap();
    let day = &record["days"][0];
    let first = &day["punches"][0];
    assert_eq!(first["kind"], "IN DAY");
    assert_eq!(first["actualTime"], "07:58 AM");
    assert_eq!(first["roundedTime"], "08:00 AM");
    assert_eq!(first["clockName"], "Front Door");
    assert_eq!(first["clockCode"], "12");
    assert_eq!(first["comment"], "on&time");
    assert_eq!(first["changeDetailState"], "not_applicable");
    assert_eq!(day["comments"], json!(["Late start"]));
    let out = &day["punches"][1];
    assert_eq!(out["kind"], "OUT DAY");
    assert_eq!(out["actualTime"], "04:01 PM");
    assert_eq!(out["changeRequestStatus"], "pending");
    assert_eq!(out["approved"], false);
    assert_eq!(out["changeOperation"], "edit");
    assert_eq!(out["currentKind"], "OUT LUNCH");
    assert_eq!(out["currentTime"], "04:00 PM");
    assert_eq!(out["requestedKind"], "OUT LUNCH");
    assert_eq!(out["requestedTime"], "04:30 PM");
    assert_eq!(out["changeNote"], Value::Null);
    assert_eq!(out["changeDetailState"], "complete");
    // The shown time must be the cell's own child, as `timecard.js` requires.
    let wrapped = pending.replacen(
        r#"<span class="current-timecard-cell">04:00 PM</span>"#,
        r#"<div><span class="current-timecard-cell">04:00 PM</span></div>"#,
        1,
    );
    let sunday = row(&[("date", "HEADING"), ("o1", &wrapped)]);
    assert_eq!(
        read(&page("AA01", "<!doctype html>", Some(sunday))),
        Err(Unreadable::Invalid("timecard_punch_invalid"))
    );
}

#[test]
fn a_pending_change_on_the_cell_itself_is_read_completely() {
    let cell = format!(
        r#"{}<i title="OUT DAY&lt;br&gt;Actual: 04:02 PM&lt;br&gt;Rounded: 04:00 PM"></i>"#,
        punch("04:00 PM")
    );
    let sunday = format!(
        concat!(
            r#"<tr><td>HEADING</td><td></td><td>{}</td><td></td><td class="pcrPending" "#,
            r#"data-pcr-operation="Edit" data-pcr-current-kind="OUT BREAK" "#,
            r#"data-pcr-current-time="04:00 pm" data-pcr-requested-kind="out lunch" "#,
            r#"data-pcr-requested-time="04:30 PM">{cell}</td><td></td><td></td><td></td>"#,
            r#"<td>8</td><td>8</td><td></td><td></td><td></td><td></td><td></td></tr>"#
        ),
        punch("08:00 AM"),
        cell = cell
    );
    let record = read(&page("AA01", "<!doctype html>", Some(sunday.clone()))).unwrap();
    let out = &record["days"][0]["punches"][1];
    assert_eq!(out["changeRequestStatus"], "pending");
    assert_eq!(out["changeOperation"], "edit");
    assert_eq!(out["currentKind"], "OUT LUNCH");
    assert_eq!(out["currentTime"], "04:00 PM");
    assert_eq!(out["requestedTime"], "04:30 PM");
    assert_eq!(out["changeDetailState"], "complete");
    assert_eq!(out["provenanceAvailable"], true);
    // A second marker, or a direction without every field, is not a change record.
    let doubled = sunday.replace("<i title", r#"<i class="pcrApproved" title"#);
    assert_eq!(
        read(&page("AA01", "<!doctype html>", Some(doubled))),
        Err(Unreadable::Invalid("timecard_pcr_marker_invalid"))
    );
    let partial = sunday.replace(r#" data-pcr-requested-time="04:30 PM""#, "");
    assert_eq!(
        read(&page("AA01", "<!doctype html>", Some(partial))),
        Err(Unreadable::Invalid("timecard_provenance_invalid"))
    );
}

#[test]
fn refuses_another_employee_and_anything_it_cannot_prove() {
    assert_eq!(
        read(&page("BB02", "<!doctype html>", None)),
        Err(Unreadable::WrongEmployee)
    );
    let untitled = page("AA01", "<!doctype html>", None).replace("Timecard Editor", "Sign In");
    assert_eq!(
        read(&untitled),
        Err(Unreadable::Invalid("timecard_identity_invalid"))
    );
    let hours = page("AA01", "<!doctype html>", None)
        .replace("Weekly Totals</td><td>8", "Weekly Totals</td><td>9");
    assert!(
        read(&hours).is_ok(),
        "reconciling hours is `project`'s check"
    );
    let unknown = row(&[
        ("date", "HEADING"),
        (
            "i1",
            &format!(r#"<div class="pcrMaybe">{}</div>"#, punch("08:00 AM")),
        ),
    ]);
    assert_eq!(
        read(&page("AA01", "<!doctype html>", Some(unknown))),
        Err(Unreadable::Invalid("timecard_pcr_marker_invalid"))
    );
    let control = row(&[
        ("date", "HEADING"),
        ("paycode", r#"<input type="number" value="1">"#),
    ]);
    assert_eq!(
        read(&page("AA01", "<!doctype html>", Some(control))),
        Err(Unreadable::Invalid("timecard_control_unsupported"))
    );
}

#[test]
fn classes_and_ids_ignore_case_only_in_quirks_mode() {
    let shouting = |doctype: &str| {
        page("AA01", doctype, None)
            .replace("current-timecard-cell", "Current-Timecard-Cell")
            .replace("tbltimesheet", "TblTimesheet")
    };
    assert!(read(&shouting("")).is_ok());
    assert_eq!(
        read(&shouting("<!doctype html>")),
        Err(Unreadable::Invalid("timecard_identity_invalid"))
    );
}

#[test]
fn text_follows_the_browser() {
    assert_eq!(clean("\u{feff} a \u{a0}\n b\u{3000}"), "a b");
    assert_eq!(decode("A&lt;br&gt;B"), "A\nB");
    assert_eq!(decode("A&amp;lt;br&amp;gt;B"), "A<br>B");
    assert_eq!(decode("  x\t\ty  "), "x y");
    assert_eq!(line_breaks("a<BR />b<br\t>c<bra>"), "a\nb\nc<bra>");
    assert_eq!(shown_time("8:05 PM").as_deref(), Some("08:05 PM"));
    assert_eq!(shown_time("00:05 PM"), None);
    assert_eq!(
        day_header(" MON  (09/14) "),
        Some(("MON".into(), "09/14".into()))
    );
    assert_eq!(punch_kind("out break (edited)"), "OUT LUNCH");
    assert_eq!(punch_kind("IN DAYS"), "");
    assert_eq!(clock("Door (A (1)"), ("Door (A".into(), "1".into()));
    assert_eq!(clock("Door (1) x"), ("Door (1) x".into(), String::new()));
    assert_eq!(slice_from("Cloc\u{212a}: x", 6), " x");
}

#[test]
fn numbers_follow_javascript() {
    for (text, value) in [
        ("8", Some(8.)),
        ("$1,234.50", Some(1234.5)),
        ("0x10", Some(16.)),
        (".5", Some(0.5)),
        ("5.", Some(5.)),
        ("1e2", Some(100.)),
        ("$", Some(0.)),
        ("", None),
        ("-", None),
        ("-1", None),
        ("inf", None),
        ("Infinity", None),
        ("1_0", None),
    ] {
        assert_eq!(numeric(text), value, "{text:?}");
    }
    // `toFixed` rounds an exact tie up and keeps the binary value's side otherwise.
    assert_eq!(fixed(0.125), 0.13);
    assert_eq!(fixed(1.005), 1.0);
    assert_eq!(fixed(8.0 + 7.75), 15.75);
    assert_eq!(fixed(0.1 + 0.2), 0.3);
}
