//! What fixture mode collects: a small made-up week at the station, a few drivers on the
//! shifts a DSP runs, a backup and a day off.
use super::capture::{Assignment, Capture, Collection, Request};
use dispatch_core::{Result, db::now};
use jiff::ToSpan;
use serde_json::json;

/// Transporter ID, name, and each day's shift from Sunday: its name, start in minutes after
/// midnight and kind, or none.
type Day = Option<(&'static str, i64, &'static str)>;
const ROSTER: [(&str, &str, [Day; 7]); 4] = [
    (
        "A1FIXTURE00001",
        "Avery Example",
        [
            None,
            Some(("Standard Parcel", 600, "PRIMARY")),
            Some(("Standard Parcel", 600, "PRIMARY")),
            Some(("Standard Parcel", 600, "PRIMARY")),
            Some(("Standard Parcel", 600, "PRIMARY")),
            None,
            Some(("Day off", 0, "TIMEOFF")),
        ],
    ),
    (
        "A1FIXTURE00002",
        "Blake Example",
        [
            Some(("Standard Parcel", 630, "PRIMARY")),
            Some(("Standard Parcel", 630, "PRIMARY")),
            None,
            Some(("Standard Parcel", 630, "BACKUP")),
            Some(("Standard Parcel", 630, "PRIMARY")),
            Some(("Standard Parcel", 630, "PRIMARY")),
            None,
        ],
    ),
    (
        "A1FIXTURE00003",
        "Casey Example",
        [
            Some(("Nursery Route", 660, "PRIMARY")),
            None,
            Some(("Nursery Route", 660, "PRIMARY")),
            Some(("Nursery Route", 660, "PRIMARY")),
            None,
            Some(("Nursery Route", 660, "PRIMARY")),
            Some(("Nursery Route", 660, "PRIMARY")),
        ],
    ),
    (
        "A1FIXTURE00004",
        "Drew Example",
        [
            None,
            Some(("Dispatcher AM", 420, "PRIMARY")),
            Some(("Dispatcher AM", 420, "PRIMARY")),
            Some(("Dispatcher AM", 420, "PRIMARY")),
            Some(("Dispatcher AM", 420, "PRIMARY")),
            Some(("Dispatcher AM", 420, "PRIMARY")),
            None,
        ],
    ),
];

pub fn fixture(request: &Request) -> Result<Capture> {
    request.validate()?;
    let started_at = now();
    let (start, end) = request.week()?;
    let service_area = "area-fixture";
    let mut assignments = Vec::new();
    for (driver_index, (transporter, name, days)) in ROSTER.iter().enumerate() {
        let driver = json!({"driverPersonId": transporter, "driverName": name});
        for (day_index, day) in days.iter().enumerate() {
            let Some((shift, start_at, kind)) = day else {
                continue;
            };
            let date = (start + (day_index as i64).days()).to_string();
            let off = *kind == "TIMEOFF";
            let item = json!({
                "mappingId": format!("mapping-fixture-{driver_index}-{day_index}"),
                "mappingDate": date,
                "serviceAreaId": service_area,
                "employeeId": format!("E{:04}", driver_index + 1),
                "assignmentType": kind,
                "shiftId": format!("shift-fixture-{}", shift.to_ascii_lowercase().replace(' ', "-")),
                "shiftName": shift,
                "shiftType": if off { "UNAVAILABLE" } else { "REGULAR" },
                "shiftPosition": if off { None } else if shift.starts_with("Dispatcher") { Some("DISPATCHER") } else { Some("DRIVER") },
                "startTimeMin": start_at,
                "startTimeMax": start_at,
                "durationMin": if off { 1440 } else { 600 },
                "durationMax": if off { 1440 } else { 600 },
                "associateQualification": "STANDARD",
                "mappingReason": "COPY",
                "mappingStatus": "ACTIVE",
                "state": "PUBLISH",
            });
            assignments.push(Assignment::read(&driver, &date, item, service_area)?);
        }
    }
    let capture = Capture {
        version: 1,
        collection: Collection::Shifts,
        station: request.station.clone(),
        service_area_id: service_area.into(),
        week_start: start.to_string(),
        week_end: end.to_string(),
        started_at,
        finished_at: now().max(started_at),
        drivers: ROSTER.len(),
        assignments,
    };
    capture.validate(request)?;
    Ok(capture)
}
