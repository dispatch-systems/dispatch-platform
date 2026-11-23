//! What a shifts collection brings: every shift assignment Amazon's Scheduling lists at the
//! DSP's station for the week holding the day it ran, as its "Shifts & Availability" sheet
//! lists them. Every field is untrusted input, bounded here.
use crate::discovery::{CollectionRequest, Discovery};
use dispatch_core::{Error, Result, ensure, foundation::validate::token};
use jiff::{ToSpan, civil::Date};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

pub const JOB_KIND: &str = "cortex.shifts.collect";
/// Far more assignments than one station's week holds: one DSP's came to 482.
pub const MAX_ASSIGNMENTS: usize = 20_000;
/// Far more drivers than one DSP's roster lists: one DSP's came to 127.
pub const MAX_DRIVERS: usize = 5_000;
/// The largest assignment kept as Amazon sent it.
const MAX_ROW_BYTES: usize = 16 * 1024;
/// The longest note kept. One DSP's longest was 68 characters.
const MAX_NOTE: usize = 2000;
/// The latest a shift may start or last, in minutes: two days.
const MAX_MINUTES: i64 = 2 * 24 * 60;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Collection {
    #[serde(rename = "shifts")]
    Shifts,
}

/// The week a job asks for: the one holding `date`, at the DSP's station.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub collection: Collection,
    /// The day it is at the station when the job is bound to run.
    pub date: String,
    pub station: String,
    pub timezone: String,
    pub dsp_name: String,
    pub dsp_abbreviation: String,
}
impl Request {
    /// Whether a job request or a collection names the shifts collection.
    pub fn is(value: &Value) -> bool {
        matches!(
            value.get("collection").and_then(Value::as_str),
            Some("shifts")
        )
    }
    pub fn parse(value: &Value) -> Result<Option<Self>> {
        if !Self::is(value) {
            return Ok(None);
        }
        let request: Self = dispatch_core::foundation::wire::request(value)?;
        request.validate()?;
        Ok(Some(request))
    }
    /// What discovery resolves the station's service area from.
    pub fn scope_request(&self) -> CollectionRequest {
        CollectionRequest::Discover(Discovery {
            date: self.date.clone(),
            station: self.station.clone(),
            timezone: self.timezone.clone(),
            dsp_name: self.dsp_name.clone(),
            dsp_abbreviation: self.dsp_abbreviation.clone(),
        })
    }
    /// The first and last day of the week it reads.
    pub fn week(&self) -> Result<(Date, Date)> {
        Ok(week(dispatch_core::foundation::clock::date(&self.date)?))
    }
    pub fn validate(&self) -> Result<()> {
        ensure(token(&self.dsp_abbreviation, 32), "invalid_input", 400)?;
        let CollectionRequest::Discover(discovery) = self.scope_request() else {
            unreachable!()
        };
        self.scope_request()
            .validate_scope(&discovery.scope("discovery", "discovery")?)
    }
}

/// The Sunday-to-Saturday week holding `date`, as Amazon's Scheduling counts its weeks.
pub fn week(date: Date) -> (Date, Date) {
    let sunday = date - i64::from(date.weekday().to_sunday_zero_offset()).days();
    (sunday, sunday + 6.days())
}

/// One shift assignment: a driver on a shift on a day, a backup for one, or time off.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Assignment {
    /// Amazon's id for it, `mappingId`, which a change keeps.
    pub mapping_id: String,
    /// The day it is for.
    pub date: String,
    /// The driver's transporter ID, `driverPersonId`, which every Amazon source names them by.
    pub transporter_id: String,
    /// The driver's name, as the roster gives it.
    pub driver_name: Option<String>,
    /// The DSP's own ID for the driver.
    pub employee_id: Option<String>,
    /// `PRIMARY`, `BACKUP` or `TIMEOFF`, as Amazon writes it.
    pub kind: String,
    /// The shift, by Amazon's id, and its name.
    pub shift_id: String,
    pub shift_name: String,
    /// `REGULAR`, or `UNAVAILABLE` for the time a driver can't work.
    pub shift_type: String,
    /// `DRIVER`, `HELPER` or `DISPATCHER`, when the shift names one.
    pub position: Option<String>,
    /// When it starts, in minutes after the day's midnight: the earliest and latest.
    pub start_min: i64,
    pub start_max: i64,
    /// How long it lasts, in minutes: the shortest and longest.
    pub duration_min: i64,
    pub duration_max: i64,
    /// `STANDARD` or `DOT`: the driver's qualification it asks for.
    pub qualification: Option<String>,
    /// How it was made: `COPY` from an earlier week, or `ADHOC`.
    pub reason: Option<String>,
    /// `ACTIVE`, as Amazon writes its status.
    pub status: String,
    /// `PUBLISH` once the schedule is published.
    pub state: Option<String>,
    /// The note the DSP gave it.
    pub comment: Option<String>,
    /// When Amazon made it, in milliseconds since the epoch.
    pub created_at: Option<i64>,
    /// The assignment as Amazon sent it.
    pub row: Value,
}
impl Assignment {
    /// The assignment `item`, listed for `date` under the roster row `driver`, at
    /// `service_area`.
    pub fn read(driver: &Value, date: &str, item: Value, service_area: &str) -> Result<Self> {
        let invalid = || Error::new("shifts_row_invalid", 502);
        ensure(item.is_object(), "shifts_row_invalid", 502)?;
        ensure(
            item.to_string().len() <= MAX_ROW_BYTES,
            "shifts_source_too_large",
            502,
        )?;
        ensure(
            item["serviceAreaId"] == service_area && item["mappingDate"] == date,
            crate::codes::CORTEX_SCOPE_MISMATCH.as_str(),
            502,
        )?;
        let id = |value: &Value| {
            value
                .as_str()
                .filter(|id| token(id, 128))
                .map(str::to_owned)
        };
        let minutes = |value: &Value| {
            value
                .as_i64()
                .filter(|minutes| (0..=MAX_MINUTES).contains(minutes))
                .ok_or_else(invalid)
        };
        let (start_min, start_max) = (
            minutes(&item["startTimeMin"])?,
            minutes(&item["startTimeMax"])?,
        );
        let (duration_min, duration_max) = (
            minutes(&item["durationMin"])?,
            minutes(&item["durationMax"])?,
        );
        ensure(
            start_min <= start_max && duration_min <= duration_max,
            "shifts_row_invalid",
            502,
        )?;
        Ok(Self {
            mapping_id: id(&item["mappingId"]).ok_or_else(invalid)?,
            date: date.to_owned(),
            transporter_id: id(&driver["driverPersonId"]).ok_or_else(invalid)?,
            driver_name: text(&driver["driverName"], 200).or_else(|| {
                text(
                    &Value::String(format!(
                        "{} {}",
                        driver["driverFirstName"].as_str().unwrap_or(""),
                        driver["driverLastName"].as_str().unwrap_or("")
                    )),
                    200,
                )
            }),
            employee_id: id(&item["employeeId"]).or_else(|| id(&driver["employeeId"])),
            kind: code(&item["assignmentType"]).ok_or_else(invalid)?,
            shift_id: id(&item["shiftId"]).ok_or_else(invalid)?,
            shift_name: text(&item["shiftName"], 200).ok_or_else(invalid)?,
            shift_type: code(&item["shiftType"]).ok_or_else(invalid)?,
            position: code(&item["shiftPosition"]),
            start_min,
            start_max,
            duration_min,
            duration_max,
            qualification: code(&item["associateQualification"]),
            reason: code(&item["mappingReason"]),
            status: code(&item["mappingStatus"]).ok_or_else(invalid)?,
            state: code(&item["state"]),
            comment: note(&item["comment"])?,
            created_at: item["shiftMappingCreationTime"]
                .as_i64()
                .filter(|at| *at > 0),
            row: item,
        })
    }
}
/// Text as Amazon sent it, its runs of space made one: none when empty, too long or holding
/// a control character.
fn text(value: &Value, max: usize) -> Option<String> {
    let value = value.as_str()?.trim();
    (!value.is_empty() && value.chars().count() <= max && !value.chars().any(char::is_control))
        .then(|| value.split_whitespace().collect::<Vec<_>>().join(" "))
}
/// The note the DSP gave an assignment, as written, its lines kept: none when empty. One that
/// can't be kept whole fails the row, rather than reading as no note and losing its history.
fn note(value: &Value) -> Result<Option<String>> {
    let text = match value {
        Value::Null => return Ok(None),
        Value::String(text) => text.replace("\r\n", "\n"),
        _ => return Err(Error::new("shifts_row_invalid", 502)),
    };
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    ensure(
        text.chars().count() <= MAX_NOTE
            && !text
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t'),
        "shifts_row_invalid",
        502,
    )?;
    Ok(Some(text.to_owned()))
}
/// One of Amazon's codes: capitals, digits and underscores.
fn code(value: &Value) -> Option<String> {
    text(value, 64).filter(|code| {
        code.bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
    })
}

/// The station's week as collected, before publication.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Capture {
    pub version: u32,
    pub collection: Collection,
    pub station: String,
    pub service_area_id: String,
    pub week_start: String,
    pub week_end: String,
    pub started_at: i64,
    pub finished_at: i64,
    /// How many drivers the roster listed, with or without an assignment.
    pub drivers: usize,
    pub assignments: Vec<Assignment>,
}
impl Capture {
    pub fn validate(&self, request: &Request) -> Result<()> {
        request.validate()?;
        let (start, end) = request.week()?;
        ensure(self.version == 1, "shifts_capture_invalid", 502)?;
        ensure(
            self.station == request.station
                && token(&self.service_area_id, 128)
                && self.week_start == start.to_string()
                && self.week_end == end.to_string(),
            "shifts_scope_mismatch",
            502,
        )?;
        ensure(
            self.started_at > 0 && self.finished_at >= self.started_at,
            "shifts_capture_invalid",
            502,
        )?;
        ensure(
            self.drivers <= MAX_DRIVERS && self.assignments.len() <= MAX_ASSIGNMENTS,
            "shifts_source_too_large",
            502,
        )?;
        let mut ids = BTreeSet::new();
        for assignment in &self.assignments {
            let date = dispatch_core::foundation::clock::date(&assignment.date)?;
            ensure(
                (start..=end).contains(&date)
                    && date.to_string() == assignment.date
                    && ids.insert(assignment.mapping_id.as_str()),
                "shifts_capture_invalid",
                502,
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../tests/backend/collections/shifts/capture.rs"]
mod tests;
