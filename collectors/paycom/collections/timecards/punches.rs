//! Daily cards from the punches Paycom's API lists for one employee. The API offers the
//! punches alone, not the hours Paycom's own timecard totals, so a day's hours here are the
//! time between each in and its out, and the hours entered without a punch.
use super::EmployeeTimecardPeriod;
use crate::codes;
use dispatch_core::{Error, Result, db::s, ensure, foundation::clock};
use jiff::{
    Timestamp, ToSpan,
    civil::{Date, DateTime},
    tz::TimeZone,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn unreadable() -> Error {
    Error::new(codes::API_UNREADABLE.as_str(), 502)
}

/// One punch, as the timecard shows it.
struct Stamp {
    at: Timestamp,
    kind: &'static str,
    /// Whether it clocks in.
    enters: bool,
}

/// A punch's kind, from Paycom's two-letter type or its name. A break reads as a lunch, as
/// it does on the timecard's pages.
fn kind(value: &str) -> Result<(&'static str, bool)> {
    Ok(match value.trim().to_ascii_uppercase().as_str() {
        "ID" | "IN DAY" => ("IN DAY", true),
        "OD" | "OUT DAY" => ("OUT DAY", false),
        "OL" | "OUT LUNCH" | "OB" | "OUT BREAK" => ("OUT LUNCH", false),
        "IL" | "IN LUNCH" | "IB" | "IN BREAK" => ("IN LUNCH", true),
        _ => return Err(unreadable()),
    })
}

/// When a punch was made: seconds since the epoch, as the API takes its dates, or a date and
/// time, which without an offset is the clock where the DSP is.
fn time(value: &Value, zone: &TimeZone) -> Result<Timestamp> {
    let seconds = value
        .as_i64()
        .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()));
    if let Some(seconds) = seconds {
        // Between 2000 and 2100: a count in another unit is refused, not misread.
        ensure(
            (946_684_800..4_102_444_800).contains(&seconds),
            codes::API_UNREADABLE.as_str(),
            502,
        )?;
        return Timestamp::from_second(seconds).map_err(|_| unreadable());
    }
    let text = value.as_str().ok_or_else(unreadable)?.trim();
    if let Ok(at) = text.parse() {
        return Ok(at);
    }
    ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M"]
        .iter()
        .find_map(|format| DateTime::strptime(format, text).ok())
        .and_then(|local| clock::instant(zone, local))
        .and_then(|at| Timestamp::from_millisecond(at).ok())
        .ok_or_else(unreadable)
}

/// A count Paycom sends as a number or as its text.
fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
        .filter(|number: &f64| number.is_finite())
}

fn deleted(row: &Value) -> bool {
    number(&row["isdeleted"]).is_some_and(|flag| flag != 0.) || row["isdeleted"] == true
}

fn pair(entered: Option<&Stamp>, left: Option<&Stamp>, zone: &TimeZone) -> Value {
    let shown = |stamp: &Stamp| zone.to_datetime(stamp.at).strftime("%I:%M %p").to_string();
    json!({
        "in": entered.map(shown),
        "out": left.map(shown),
        "hours": null,
        "inKind": entered.map(|stamp| stamp.kind),
        "outKind": left.map(|stamp| stamp.kind),
    })
}

/// The fourteen cards of `period` for the employee `code`, from the rows of their punch
/// history, read where the DSP is.
pub(crate) fn cards(
    rows: &[Value],
    code: &str,
    period: &EmployeeTimecardPeriod,
    zone: &TimeZone,
) -> Result<Vec<Value>> {
    let start = period.start()?;
    let mut stamps = Vec::new();
    // Hours entered without a punch, as paid leave is, by the day they are entered on.
    let mut entered: BTreeMap<Date, f64> = BTreeMap::new();
    for row in rows.iter().filter(|row| !deleted(row)) {
        let at = time(&row["punchtime"], zone)?;
        match number(&row["entrytype"]).ok_or_else(unreadable)? as i64 {
            1 => {
                let (kind, enters) = kind(s(row, "punchtype"))?;
                stamps.push(Stamp { at, kind, enters });
            }
            2 => {
                *entered.entry(zone.to_datetime(at).date()).or_default() +=
                    number(&row["hours"]).ok_or_else(unreadable)?;
            }
            // Earnings and units add no time to a day.
            3 | 4 => {}
            _ => return Err(unreadable()),
        }
    }
    stamps.sort_by_key(|stamp| stamp.at);
    // A punch belongs to the day its shift began: an IN DAY's own, and what follows it until
    // its OUT DAY, so a shift that ends after midnight stays on one card. A shift never closed
    // doesn't take the next day's punches with it.
    let mut days: BTreeMap<Date, Vec<&Stamp>> = BTreeMap::new();
    let mut shift: Option<Date> = None;
    for stamp in &stamps {
        let date = zone.to_datetime(stamp.at).date();
        let day = match shift {
            Some(began) if stamp.kind != "IN DAY" && (date - began).get_days() <= 1 => began,
            _ => date,
        };
        shift = (stamp.kind != "OUT DAY").then_some(day);
        days.entry(day).or_default().push(stamp);
    }
    (0..super::PERIOD_DAYS)
        .map(|offset| {
            let date = start + offset.days();
            let mut punches = Vec::new();
            let mut seconds = 0;
            let mut open: Option<&Stamp> = None;
            for &stamp in days.get(&date).map_or(&[][..], Vec::as_slice) {
                if stamp.enters {
                    if let Some(unclosed) = open.replace(stamp) {
                        punches.push(pair(Some(unclosed), None, zone));
                    }
                } else {
                    if let Some(entered) = open {
                        seconds += entered.at.duration_until(stamp.at).as_secs();
                    }
                    punches.push(pair(open.take(), Some(stamp), zone));
                }
            }
            if let Some(unclosed) = open {
                punches.push(pair(Some(unclosed), None, zone));
            }
            let missing = punches
                .iter()
                .any(|punch| punch["in"].is_null() || punch["out"].is_null());
            let hours = seconds as f64 / 3600. + entered.get(&date).copied().unwrap_or(0.);
            let hours = (hours * 100.).round() / 100.;
            ensure(
                (0.0..=48.0).contains(&hours),
                codes::INVALID_TIMECARD_HOURS.as_str(),
                409,
            )?;
            Ok(json!({
                "employeeCode": code,
                "date": date.to_string(),
                "hours": hours,
                "status": if missing {
                    "Missing punch"
                } else if punches.is_empty() {
                    "No punches"
                } else {
                    "Complete"
                },
                "punches": punches,
            }))
        })
        .collect()
}

#[cfg(test)]
#[path = "../../tests/backend/collections/timecards/punches.rs"]
mod tests;
