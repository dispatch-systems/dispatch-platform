//! Published daily cards from an extracted Paycom record: reconcile reported hours,
//! then pair each day's punches without crossing pay-code rows.
use dispatch_core::{Error, Result, db::s, ensure};
use serde_json::{Value, json};

pub(crate) fn project(record: &Value, employee: &str) -> Result<Vec<Value>> {
    let error = || Error::new("invalid_timecard_hours", 409);
    let days = record["days"].as_array().ok_or_else(error)?;
    let additional = record["additionalRows"].as_array().ok_or_else(error)?;
    ensure(
        days.len() == 14 && additional.len() <= 200,
        "invalid_timecard_hours",
        409,
    )?;
    let values = reconciled_hours(record, days, additional)?;
    let mut cards = Vec::new();
    for (index, day) in days.iter().enumerate() {
        let punches = paired_punches(day)?;
        let status = if day["missingPunch"] == true {
            "Missing punch"
        } else if punches.is_empty() {
            "No punches"
        } else {
            "Complete"
        };
        cards.push(json!({
            "employeeCode": employee,
            "date": day["date"],
            "hours": (values[index] * 100.).round() / 100.,
            "status": status,
            "punches": punches,
        }));
    }
    Ok(cards)
}

fn hours(value: &Value) -> Option<f64> {
    value["totalHours"]
        .as_f64()
        .or_else(|| value["hours"].as_f64())
}

fn leading_hours(day: &Value) -> Option<f64> {
    if let Some(hours) = hours(day) {
        return Some(hours);
    }
    // DOM extraction folds all pay-code punches into the day.
    // Only row zero determines whether the leading row is empty;
    // additional rows contribute their own reported hours below.
    let no_missing_leading = day["missingPunch"] == false
        || day["unresolvedSlots"].as_array().is_some_and(|slots| {
            !slots.is_empty()
                && slots
                    .iter()
                    .all(|slot| slot.as_str().is_some_and(|slot| slot.contains(':')))
        });
    if no_missing_leading
        && day["punches"].as_array().is_some_and(|punches| {
            punches
                .iter()
                .all(|punch| punch["rowIndex"].as_u64().is_some_and(|row| row > 0))
        })
    {
        Some(0.)
    } else {
        None
    }
}

fn reconciled_hours(record: &Value, days: &[Value], additional: &[Value]) -> Result<Vec<f64>> {
    let base = days
        .iter()
        .map(|day| leading_hours(day).ok_or_else(|| Error::new("invalid_timecard_hours", 409)))
        .collect::<Result<Vec<_>>>()?;
    let rows = days
        .iter()
        .enumerate()
        .map(|(index, day)| {
            let additional_hours = additional
                .iter()
                .filter(|row| row["date"] == day["date"])
                .map(|row| hours(row).unwrap_or(0.))
                .sum::<f64>();
            base[index] + additional_hours
        })
        .collect::<Vec<_>>();
    let reported = days
        .iter()
        .enumerate()
        .map(|(index, day)| {
            let totals = std::iter::once(day)
                .chain(additional.iter().filter(|row| row["date"] == day["date"]))
                .filter_map(|row| row["totalHours"].as_f64())
                .collect::<Vec<_>>();
            if totals.is_empty() {
                rows[index]
            } else {
                totals.iter().sum()
            }
        })
        .collect::<Vec<_>>();
    [rows, base, reported]
        .into_iter()
        .find(|values| matches_totals(record, values))
        .ok_or_else(|| Error::new("provider_hours_mismatch", 409))
}

fn matches_totals(record: &Value, values: &[f64]) -> bool {
    if !values.iter().all(|value| value.is_finite() && *value >= 0.) {
        return false;
    }
    let Some(totals) = record["weeklyTotals"].as_array() else {
        return false;
    };
    if totals.len() != 2 {
        return false;
    }
    for (index, total) in totals.iter().enumerate() {
        let matches = total.as_f64().is_some_and(|total| {
            (values[index * 7..index * 7 + 7].iter().sum::<f64>() - total).abs() < 0.011
        });
        if !matches {
            return false;
        }
    }
    record["periodTotalHours"]
        .as_f64()
        .is_some_and(|total| (values.iter().sum::<f64>() - total).abs() < 0.011)
}

fn paired_punches(day: &Value) -> Result<Vec<Value>> {
    let source = day["punches"]
        .as_array()
        .ok_or_else(|| Error::new("invalid_timecard_hours", 409))?;
    let mut punches = Vec::new();
    let mut pending = Value::Null;
    let mut pending_kind = Value::Null;
    let mut row = Value::Null;
    for punch in source {
        if row != punch["rowIndex"] && !pending.is_null() {
            punches.push(json!({
                "in": pending,
                "out": null,
                "hours": null,
                "inKind": pending_kind,
                "outKind": null,
            }));
            pending = Value::Null;
            pending_kind = Value::Null;
        }
        row = punch["rowIndex"].clone();
        let kind = if s(punch, "kind").is_empty() {
            Value::Null
        } else {
            punch["kind"].clone()
        };
        if s(punch, "slot").starts_with('i') {
            if !pending.is_null() {
                punches.push(json!({
                    "in": pending,
                    "out": null,
                    "hours": null,
                    "inKind": pending_kind,
                    "outKind": null,
                }));
            }
            pending = punch["displayTime"].clone();
            pending_kind = kind;
        } else {
            punches.push(json!({
                "in": pending,
                "out": punch["displayTime"],
                "hours": null,
                "inKind": pending_kind,
                "outKind": kind,
            }));
            pending = Value::Null;
            pending_kind = Value::Null;
        }
    }
    if !pending.is_null() {
        punches.push(json!({
            "in": pending,
            "out": null,
            "hours": null,
            "inKind": pending_kind,
            "outKind": null,
        }));
    }
    Ok(punches)
}
