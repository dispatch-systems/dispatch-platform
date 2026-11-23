use super::*;
use crate::shifts::fixture;
use serde_json::json;

fn request(date: &str) -> Request {
    Request {
        collection: Collection::Shifts,
        date: date.into(),
        station: "TST1".into(),
        timezone: "America/Los_Angeles".into(),
        dsp_name: "Fixture Logistics".into(),
        dsp_abbreviation: "FXTR".into(),
    }
}

#[test]
fn a_run_reads_amazons_sunday_to_saturday_week() {
    let week = |date: &str| {
        let (start, end) = request(date).week().unwrap();
        (start.to_string(), end.to_string())
    };
    // 2026-11-15 is a Sunday: the week starts on it and ends on Saturday.
    let expected = ("2026-11-15".to_owned(), "2026-11-21".to_owned());
    for date in ["2026-11-15", "2026-11-18", "2026-11-21"] {
        assert_eq!(week(date), expected, "{date}");
    }
    assert_eq!(week("2026-11-22").0, "2026-11-22");
}

#[test]
fn an_assignment_reads_bounded_and_stays_at_its_service_area_and_day() {
    let driver = json!({"driverPersonId":"A1FIXTURE00001","driverFirstName":"Avery","driverLastName":" Example "});
    let item = json!({
        "mappingId":"m-1","mappingDate":"2026-11-23","serviceAreaId":"area-1","employeeId":"E0001",
        "assignmentType":"PRIMARY","shiftId":"s-1","shiftName":"Standard  Parcel","shiftType":"REGULAR",
        "shiftPosition":null,"startTimeMin":585,"startTimeMax":585,"durationMin":500,"durationMax":500,
        "mappingStatus":"ACTIVE","comment":"","shiftMappingCreationTime":1793655646931_i64,
    });
    let read = |item: Value| Assignment::read(&driver, "2026-11-23", item, "area-1");
    let assignment = read(item.clone()).unwrap();
    assert_eq!(
        (
            assignment.transporter_id.as_str(),
            assignment.driver_name.as_deref(),
            assignment.shift_name.as_str(),
            assignment.position.as_deref(),
            assignment.comment.as_deref(),
        ),
        (
            "A1FIXTURE00001",
            Some("Avery Example"),
            "Standard Parcel",
            None,
            None
        )
    );
    assert_eq!(assignment.row, item);
    let with = |key: &str, value: Value| {
        let mut item = item.clone();
        item[key] = value;
        read(item).unwrap_err().code
    };
    // Another service area's, or another day's, is a scope that no longer holds.
    assert_eq!(
        with("serviceAreaId", json!("area-2")),
        "cortex_scope_mismatch"
    );
    assert_eq!(
        with("mappingDate", json!("2026-11-24")),
        "cortex_scope_mismatch"
    );
    // A row Amazon changed fails rather than keeping half of it.
    assert_eq!(with("mappingId", Value::Null), "shifts_row_invalid");
    assert_eq!(with("startTimeMax", json!(500)), "shifts_row_invalid");
    assert_eq!(with("durationMin", json!(-1)), "shifts_row_invalid");
}

#[test]
fn a_capture_holds_its_requests_week_each_assignment_once() {
    let request = request("2026-11-18");
    let mut capture = fixture(&request).unwrap();
    capture.validate(&request).unwrap();
    let mut twice = capture.clone();
    twice.assignments.push(twice.assignments[0].clone());
    assert_eq!(
        twice.validate(&request).unwrap_err().code,
        "shifts_capture_invalid"
    );
    capture.assignments[0].date = "2026-11-22".into();
    assert_eq!(
        capture.validate(&request).unwrap_err().code,
        "shifts_capture_invalid"
    );
    let mut moved = fixture(&request).unwrap();
    moved.week_start = "2026-11-08".into();
    assert_eq!(
        moved.validate(&request).unwrap_err().code,
        "shifts_scope_mismatch"
    );
}
