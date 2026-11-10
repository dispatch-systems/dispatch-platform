//! Capture shaping, prepared itinerary models, JSON field conversions and bounded compression.
use super::{
    rows::{
        BreakRow, DayRow, StopRow, TaskRow, UnknownStopRow, break_rows, flatten, inactive_rows,
        stop_rows, unknown_stop_rows,
    },
    wire,
    work_phones::digits,
};
use dispatch_core::{Error, Result, ensure};
use dispatch_cortex::routes::{Capture, ItineraryCapture, MAX_BODY};
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::io::Write;

/// One list response compressed before its database write.
pub(super) struct PreparedResponse {
    pub name: &'static str,
    pub bytes: usize,
    pub body: Vec<u8>,
}
pub(super) fn prepare_lists(capture: &Capture) -> Result<Vec<PreparedResponse>> {
    [
        ("summaries", &capture.summaries),
        ("route_summaries", &capture.route_summaries),
    ]
    .into_iter()
    .map(|(name, list)| {
        let bytes = serde_json::to_vec(list)?;
        Ok(PreparedResponse {
            name,
            bytes: bytes.len(),
            body: gzip(&bytes)?,
        })
    })
    .collect()
}

/// One itinerary's rows, shaped and compressed ahead of its insert.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedItinerary {
    pub(super) id: String,
    pub(super) transporter_id: String,
    pub(super) columns: Vec<(String, Value)>,
    pub(super) stops: Vec<StopRow>,
    pub(super) tasks: Vec<TaskRow>,
    /// Tasks Amazon removed from the route, kept for the record.
    pub(super) inactive: Vec<TaskRow>,
    pub(super) breaks: Vec<BreakRow>,
    pub(super) unknown_stops: Vec<UnknownStopRow>,
    pub(super) day: DayRow,
    pub(super) addresses: Vec<Value>,
    pub(super) person: Value,
    pub(super) raw_bytes: usize,
    /// The detail gzip-compressed.
    #[serde(skip)]
    pub(super) raw: Vec<u8>,
}
/// An identifier a row can carry: printable text of a bounded length. Amazon's task and
/// stop ids mix letters, digits and punctuation.
pub fn identifier(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && !value.chars().any(|c| c.is_control())
        && !value.trim().is_empty()
}
/// An epoch time as the API sends it: milliseconds, or seconds for older fields.
pub fn stamp(value: &Value) -> Option<i64> {
    let number = value.as_f64()?;
    if !number.is_finite() || number <= 0.0 {
        return None;
    }
    Some(if number < 100_000_000_000.0 {
        (number * 1000.0).round() as i64
    } else {
        number.round() as i64
    })
}
pub(super) fn text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) if !text.trim().is_empty() => Some(text.chars().take(512).collect()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}
pub(super) fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}
pub(super) fn integer(value: &Value) -> Option<i64> {
    number(value).map(|n| n.round() as i64)
}
pub(super) fn flag(value: &Value) -> Option<i64> {
    match value {
        Value::Bool(flag) => Some(i64::from(*flag)),
        _ => None,
    }
}
pub(super) fn gzip(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(bytes)?;
    Ok(encoder.finish()?)
}
/// A stored response, inflated.
pub fn gunzip(bytes: &[u8]) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(bytes)
        .take(MAX_BODY as u64 + 1)
        .read_to_end(&mut out)?;
    ensure(out.len() <= MAX_BODY, "routes_source_too_large", 502)?;
    Ok(out)
}

/// What shaping takes from an itinerary's response, however it was read: its detail's
/// route-level fields, the rows of its stops and tasks, and the drivers and addresses it
/// names.
struct Read<'a> {
    detail: &'a Value,
    transporters: &'a Value,
    addresses: &'a Value,
    stops: Vec<StopRow>,
    tasks: Vec<TaskRow>,
    day: DayRow,
    inactive: Vec<TaskRow>,
}
/// What shaping an itinerary needs from the day's list: each itinerary's summary and
/// the drivers it names.
pub struct Shaper {
    summaries: Map<String, Value>,
    people: Vec<Value>,
}
impl Shaper {
    pub fn new(capture: &Capture) -> Self {
        Self {
            summaries: capture.summaries["itinerarySummaries"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| Some((v["itineraryId"].as_str()?.to_owned(), v.clone())))
                .collect(),
            people: capture.summaries["transporters"]
                .as_array()
                .cloned()
                .unwrap_or_default(),
        }
    }
    /// One itinerary's rows and its compressed response. Its stops and tasks, most of it, are
    /// read straight into what shaping reads of them (`wire`), without a tree; that reading
    /// lives only while this runs.
    pub fn shape(&self, captured: &ItineraryCapture) -> Result<PreparedItinerary> {
        ensure(
            captured.detail.len() <= MAX_BODY,
            "routes_source_too_large",
            502,
        )?;
        let invalid = || Error::new("routes_capture_invalid", 502);
        let response = wire::read(&captured.detail).map_err(|_| invalid())?;
        let read = response.detail.0.ok_or_else(invalid)?;
        let detail = Value::Object(read.rest);
        let (stops, tasks, day) = stop_rows(
            read.stops.0.iter(),
            &detail["stopProgress"],
            &captured.transporter_id,
        );
        let inactive = inactive_rows(read.inactive.0.iter());
        drop((read.stops, read.inactive));
        let read = Read {
            detail: &detail,
            transporters: &response.transporters,
            addresses: &response.addresses,
            stops,
            tasks,
            day,
            inactive,
        };
        self.shape_read(
            &captured.id,
            &captured.transporter_id,
            read,
            captured.detail.len(),
            gzip(captured.detail.as_bytes())?,
        )
    }
    /// One itinerary's rows from what was read of its response, with `raw`, the response
    /// compressed, kept beside them.
    fn shape_read(
        &self,
        id: &str,
        transporter_id: &str,
        read: Read,
        raw_bytes: usize,
        raw: Vec<u8>,
    ) -> Result<PreparedItinerary> {
        let detail = read.detail;
        let summary = self.summaries.get(id).cloned().unwrap_or(Value::Null);
        let person = self
            .people
            .iter()
            .chain(read.transporters.as_array().into_iter().flatten())
            .find(|t| t["transporterId"] == json!(transporter_id))
            .cloned()
            .unwrap_or(Value::Null);
        let driver_name = match (text(&person["firstName"]), text(&person["lastName"])) {
            (Some(first), Some(last)) => format!("{first} {last}"),
            _ => text(&detail["transporterName"]).unwrap_or_else(|| transporter_id.to_owned()),
        };
        let flat = flatten(&summary, detail, transporter_id, &driver_name);
        let work_phone = self.work_phone(transporter_id, read.transporters);
        Ok(PreparedItinerary {
            id: id.to_owned(),
            transporter_id: transporter_id.to_owned(),
            columns: (flat.into_iter())
                .chain([("work_phone", opt(work_phone))])
                .map(|(c, v)| (c.to_owned(), v))
                .collect(),
            stops: read.stops,
            tasks: read.tasks,
            inactive: read.inactive,
            breaks: break_rows(&summary, detail),
            unknown_stops: unknown_stop_rows(detail),
            day: DayRow {
                departed_at: stamp(&detail["transporterTimeAttributes"]["actualDepartureTime"])
                    .or_else(|| stamp(&summary["itineraryStartTime"]))
                    .or_else(|| stamp(&detail["itineraryStartTime"])),
                session_end_at: stamp(&summary["sessionEndTime"])
                    .or_else(|| stamp(&detail["sessionEndTime"])),
                route_code: text(&summary["routeCode"]).or_else(|| text(&detail["routeCode"])),
                breaks_secs: integer(&summary["totalBreaksDurationSecs"]),
                overtime_secs: integer(&summary["overtimeDurationSecs"])
                    .or_else(|| integer(&detail["overtimeDurationSecs"])),
                ..read.day
            },
            addresses: read.addresses.as_array().cloned().unwrap_or_default(),
            person,
            raw_bytes,
            raw,
        })
    }
    /// The work phone `driver`'s records show with their itinerary, by its last ten digits: the
    /// day's list's record of them, else `own`'s, the drivers the itinerary's response names; none
    /// when neither shows one.
    pub(super) fn work_phone(&self, driver: &str, own: &Value) -> Option<String> {
        let shown = |records: &[Value]| {
            let record = records
                .iter()
                .find(|r| r["transporterId"] == json!(driver))?;
            digits(record["workPhoneNumber"].as_str()?)
        };
        shown(&self.people).or_else(|| shown(own.as_array().map_or(&[], Vec::as_slice)))
    }
}

/// Every itinerary shaped: what a reprocess, which rebuilds a stored day at once, needs.
pub fn prepare(capture: &Capture) -> Result<Vec<PreparedItinerary>> {
    // Reject the aggregate before parsing any itinerary into a larger JSON tree.
    capture.validate_response_budget()?;
    let shaper = Shaper::new(capture);
    capture
        .itineraries
        .iter()
        .map(|i| shaper.shape(i))
        .collect()
}

pub(super) fn opt<T: Into<Value>>(value: Option<T>) -> Value {
    value.map(Into::into).unwrap_or(Value::Null)
}
/// A list as JSON text, `[]` when absent.
pub(super) fn list_text(value: &Value) -> String {
    match value.as_array() {
        Some(items) => Value::Array(items.clone()).to_string(),
        None => "[]".into(),
    }
}
/// A list as JSON text, or nothing when absent or empty.
pub(super) fn json_list(value: &Value) -> Value {
    match value.as_array() {
        Some(items) if !items.is_empty() => json!(Value::Array(items.clone()).to_string()),
        _ => Value::Null,
    }
}

#[cfg(test)]
#[path = "../tests/backend/shaping.rs"]
mod tests;
