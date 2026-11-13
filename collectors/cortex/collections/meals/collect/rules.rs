//! The meal rules as `scripts/meal_rules.js` and `scripts/meal_hook.js` have them for the
//! page, here for a day read with no page: how a route's meal breaks are read from the day's
//! list, and which of its deliveries bound each one in its itinerary's response. The two must
//! answer alike; a change to one is a change to both, and each has the other's tests.
use super::*;
use crate::meals::Meal;
use serde_json::value::RawValue;
use std::collections::HashMap;

/// Why a list or an itinerary was not read: the code the collection knows, and a fixed label
/// for the job's metrics, never page content.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Fault {
    pub(crate) code: &'static str,
    pub(crate) reason: String,
}
impl Fault {
    /// A response that could not be read as text.
    pub(crate) fn unread() -> Self {
        fault(INCOMPLETE, "response_text")
    }
}
fn fault(code: &'static str, reason: &str) -> Fault {
    Fault {
        code,
        reason: reason.into(),
    }
}
const INCOMPLETE: &str = "cortex_content_incomplete";
const MISMATCH: &str = "cortex_scope_mismatch";
const CHANGED: &str = "cortex_source_changed";
const TOO_LARGE: &str = "cortex_source_too_large";
const IDENTITY: &str = "cortex_invalid_identity";

/// How far apart two starts of one meal may be and still be one start pressed twice.
const DOUBLE_START: i64 = 5 * 60000;

fn token(value: &Value) -> Option<&str> {
    value.as_str().filter(|text| {
        (1..=256).contains(&text.len())
            && text
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.:#-".contains(&b))
    })
}
/// A time Cortex sends in seconds or in milliseconds, as milliseconds.
fn stamp(value: &Value) -> Option<i64> {
    let number = value.as_f64().filter(|n| n.is_finite() && *n > 0.0)?;
    Some(
        if number < 100_000_000_000.0 {
            number * 1000.0
        } else {
            number
        }
        .round() as i64,
    )
}
/// A whole number, as `Number.isInteger` has it.
fn whole(value: &Value) -> Option<f64> {
    value.as_f64().filter(|n| n.is_finite() && n.fract() == 0.0)
}

/// Why a route's punches were not read: its list was no list, or a punch broke the rule named.
enum Unread {
    Incomplete,
    Rule(&'static str),
}
struct Record {
    id: String,
    start: i64,
    end: Option<i64>,
    sequence: Value,
}
/// The meals a route's break punches tell of, in the order they began.
fn meals(values: &Value) -> std::result::Result<Vec<Punch>, Unread> {
    let values = values.as_array().ok_or(Unread::Incomplete)?;
    let mut records: Vec<Record> = Vec::new();
    for punch in values {
        if punch.is_null() {
            return Err(Unread::Incomplete);
        }
        if punch["type"] != "MEAL" {
            continue;
        }
        let (start, end) = (stamp(&punch["timeStampOn"]), stamp(&punch["timeStampOff"]));
        let id = token(&punch["breakId"]).ok_or(Unread::Rule("meal_break_id"))?;
        token(&punch["punchId"]).ok_or(Unread::Rule("meal_punch_id"))?;
        let start = start.ok_or(Unread::Rule("meal_start"))?;
        let off = match punch["state"].as_str() {
            Some("ON") => false,
            Some("OFF") => true,
            _ => return Err(Unread::Rule("meal_state")),
        };
        match (off, end) {
            (true, None) => return Err(Unread::Rule("meal_off_without_end")),
            (true, Some(end)) if end < start => return Err(Unread::Rule("meal_end_before_start")),
            (false, Some(_)) => return Err(Unread::Rule("meal_on_with_end")),
            _ => (),
        }
        let current = Record {
            id: id.to_owned(),
            start,
            end,
            sequence: punch["sequenceNumber"].clone(),
        };
        let Some(prior) = records.iter_mut().find(|record| record.id == id) else {
            records.push(current);
            continue;
        };
        if whole(&current.sequence).is_none()
            || prior.sequence.as_f64() != current.sequence.as_f64()
        {
            return Err(Unread::Rule("meal_repeat_sequence"));
        }
        if prior.end.is_none() == end.is_none() {
            if prior.start == start && prior.end == end {
                continue;
            }
            // A start pressed twice: Cortex keeps both starts open until the meal ends, then
            // completes the first and keeps the second inside it. While the meal runs, it
            // began at the first.
            if end.is_none() && (prior.start - start).abs() <= DOUBLE_START {
                if start < prior.start {
                    *prior = current;
                }
                continue;
            }
            return Err(Unread::Rule("meal_repeat_conflict"));
        }
        // One logical break can retain its ON punch alongside the completed OFF record. The
        // completed pair is authoritative only for that same break and sequence, and an ON
        // punch inside its recorded interval.
        let (completed, opened) = if end.is_none() {
            (&*prior, &current)
        } else {
            (&current, &*prior)
        };
        if opened.start < completed.start || completed.end.is_some_and(|end| opened.start > end) {
            return Err(Unread::Rule("meal_repeat_outside"));
        }
        if end.is_some() {
            *prior = current;
        }
    }
    let mut meals: Vec<Punch> = records
        .into_iter()
        .map(|record| Punch {
            id: record.id,
            start: record.start,
            end: record.end,
        })
        .collect();
    meals.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| a.id.cmp(&b.id)));
    Ok(meals)
}
/// A route's meals, or, when its punches break a rule, none and the rule's name: its meals are
/// then unknown, not absent, and only that route goes without them.
fn route_meals(
    values: &Value,
    reason: &str,
) -> std::result::Result<(Vec<Punch>, Option<String>), Fault> {
    match meals(values) {
        Ok(meals) => Ok((meals, None)),
        Err(Unread::Rule(rule)) => Ok((Vec::new(), Some(rule.to_owned()))),
        Err(Unread::Incomplete) => Err(fault(INCOMPLETE, reason)),
    }
}

/// The day's routes for `scope` as its list names them, each with its meals, by id.
pub(crate) fn candidates(
    summaries: &Value,
    scope: &Scope,
) -> std::result::Result<Vec<Candidate>, Fault> {
    if scope.provider == "ALL_DSPS" {
        return Err(fault("invalid_cortex_scope", "provider"));
    }
    let (Some(all), Some(transporters)) = (
        summaries["itinerarySummaries"].as_array(),
        summaries["transporters"].as_array(),
    ) else {
        return Err(fault(INCOMPLETE, "list_response"));
    };
    if all.len() > 1000 || transporters.len() > 10000 {
        return Err(fault(TOO_LARGE, "summaries"));
    }
    let name = |transporter: &Value| {
        [&transporter["firstName"], &transporter["lastName"]]
            .iter()
            .filter_map(|part| part.as_str())
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_owned()
    };
    let names: HashMap<&str, String> = transporters
        .iter()
        .filter_map(|t| Some((t["transporterId"].as_str()?, name(t))))
        .collect();
    let mut routes = Vec::new();
    for summary in all {
        if scope.provider != "ALL_DRIVERS" && summary["companyId"] != scope.provider.as_str() {
            continue;
        }
        let (Some(id), Some(transporter)) = (
            token(&summary["itineraryId"]),
            token(&summary["transporterId"]),
        ) else {
            return Err(fault(IDENTITY, "evidence"));
        };
        let driver = (names.get(transporter))
            .filter(|name| !name.is_empty())
            .ok_or_else(|| fault(IDENTITY, "evidence"))?;
        let (meals, unreadable) = route_meals(&summary["breaks"], "evidence")?;
        let route = match &summary["routeCode"] {
            Value::String(code) if !code.is_empty() => code.clone(),
            Value::Null | Value::String(_) => "UNASSIGNED".into(),
            _ => return Err(fault(INCOMPLETE, "evidence")),
        };
        let mut candidate = Candidate {
            id: id.to_owned(),
            transporter_id: transporter.to_owned(),
            driver: driver.clone(),
            route,
            route_complete: summary["executionStatus"] == "COMPLETE",
            meals,
            unreadable,
            revision: String::new(),
        };
        // Only published facts version a route. Delivery progress changes with every
        // package, so including it would fail every sync of a working day; each detail read
        // validates its own stops and deliveries instead.
        candidate.revision =
            serde_json::to_string(&candidate).map_err(|_| fault(INCOMPLETE, "evidence"))?;
        routes.push(candidate);
    }
    routes.sort_by(|a, b| a.id.cmp(&b.id));
    if routes.windows(2).any(|pair| pair[0].id == pair[1].id) {
        return Err(fault(IDENTITY, "identity"));
    }
    Ok(routes)
}

/// What the rules read of an itinerary's response: who and when it is, its punches, and its
/// stops and removed tasks left as text until the route is known to be the one asked for.
#[derive(Deserialize)]
struct Response<'a> {
    #[serde(rename = "itineraryDetails", borrow)]
    details: Details<'a>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Details<'a> {
    #[serde(default)]
    itinerary_id: Value,
    #[serde(default)]
    transporter_id: Value,
    #[serde(default)]
    service_area_id: Value,
    #[serde(default)]
    local_date: Value,
    #[serde(default)]
    breaks: Value,
    #[serde(default)]
    execution_status: Value,
    #[serde(default)]
    stop_progress: Value,
    #[serde(default, borrow)]
    stops: Option<&'a RawValue>,
    #[serde(default, borrow)]
    unknown_stops: Option<&'a RawValue>,
    #[serde(default, borrow)]
    inactive_tasks: Option<&'a RawValue>,
}
#[derive(Deserialize)]
struct Stop {
    tasks: Vec<Task>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Task {
    #[serde(default)]
    task_id: Value,
    #[serde(default)]
    task_type: Value,
    #[serde(default)]
    task_state: Value,
    #[serde(default)]
    execution_status: Value,
    #[serde(default)]
    actual_execution_time: Value,
    #[serde(default)]
    task_execution_time: Value,
    #[serde(default)]
    transporter_id: Value,
}
impl Task {
    fn time(&self) -> Option<i64> {
        stamp(if self.actual_execution_time.is_null() {
            &self.task_execution_time
        } else {
            &self.actual_execution_time
        })
    }
    fn delivered(&self) -> bool {
        self.task_state == "DELIVERED"
    }
    /// A delivery made, and when.
    fn drop_off(&self) -> Option<i64> {
        (self.task_type == "DROP_OFF" && self.execution_status == "COMPLETE")
            .then(|| self.time())
            .flatten()
    }
}

/// Candidate `c`'s evidence from its itinerary's response, as the hook reads it: its meals as
/// the list gave them, each with the deliveries that bound it when every delivery of the route
/// is accounted for.
pub(crate) fn detail(
    text: &str,
    c: &Candidate,
    scope: &Scope,
) -> std::result::Result<Itinerary, Fault> {
    let response: Response =
        serde_json::from_str(text).map_err(|_| fault(INCOMPLETE, "script_error"))?;
    let d = response.details;
    let local_date = match &d.local_date {
        Value::Array(parts) => parts
            .iter()
            .enumerate()
            .map(|(index, part)| {
                let part = match part {
                    Value::String(text) => text.clone(),
                    other => other.to_string(),
                };
                format!("{part:0>width$}", width = if index == 0 { 4 } else { 2 })
            })
            .collect::<Vec<_>>()
            .join("-"),
        Value::String(date) => date.clone(),
        _ => String::new(),
    };
    if d.itinerary_id != c.id.as_str() {
        return Err(fault(MISMATCH, "detail_itinerary"));
    }
    // In a response, another driver's is a route that changed hands after the list was read.
    if d.transporter_id != c.transporter_id.as_str() {
        return Err(fault(CHANGED, "route_changed"));
    }
    if d.service_area_id != scope.service_area_id.as_str() {
        return Err(fault(MISMATCH, "detail_area"));
    }
    if local_date != scope.date {
        return Err(fault(MISMATCH, "detail_date"));
    }
    let itinerary = |coverage, meals, unreadable| Itinerary {
        id: c.id.clone(),
        transporter_id: c.transporter_id.clone(),
        driver: c.driver.clone(),
        route: c.route.clone(),
        observed_at: now(),
        route_complete: c.route_complete,
        delivery_coverage: coverage,
        meals,
        source_url: None,
        unreadable,
    };
    let (breaks, unreadable) = route_meals(&d.breaks, "response_evidence")?;
    // Punches the rules can't read leave the route's meals unknown, whatever the list said.
    if unreadable.is_some() {
        return Ok(itinerary(Coverage::Unavailable, Vec::new(), unreadable));
    }
    let same = breaks.len() == c.meals.len()
        && (breaks.iter().zip(&c.meals))
            .all(|(a, b)| (a.id.as_str(), a.start, a.end) == (b.id.as_str(), b.start, b.end));
    if !same || (d.execution_status == "COMPLETE") != c.route_complete {
        return Err(fault(CHANGED, "meal_changed"));
    }
    let listed = |raw: Option<&RawValue>| raw.is_some_and(|raw| raw.get().starts_with('['));
    let (Some(stops), Some(inactive)) = (d.stops, d.inactive_tasks) else {
        return Err(fault(INCOMPLETE, "stops"));
    };
    if !listed(Some(stops)) || !listed(d.unknown_stops) || !listed(Some(inactive)) {
        return Err(fault(INCOMPLETE, "stops"));
    }
    let stops: Vec<Stop> =
        serde_json::from_str(stops.get()).map_err(|_| fault(INCOMPLETE, "stops"))?;
    let inactive: Vec<Task> =
        serde_json::from_str(inactive.get()).map_err(|_| fault(INCOMPLETE, "stops"))?;
    if stops.len() > 2000 || inactive.len() > 10000 {
        return Err(fault(INCOMPLETE, "stops"));
    }
    // unknownStops records unplanned dwell locations, not missing delivery tasks. It does not
    // invalidate the independently counted delivery evidence of the stops' tasks.
    let mut complete = whole(&d.stop_progress["total"]) == Some(stops.len() as f64);
    // Each task once, by its id, as it was first seen, and the deliveries among them in the
    // order they came: the place in the route, from 0, of the stop that first held each.
    let mut seen: HashMap<&str, [&Value; 3]> = HashMap::new();
    let mut times: HashMap<&str, (Option<i64>, &Value)> = HashMap::new();
    let mut deliveries: Vec<(&str, i64, u32)> = Vec::new();
    let mut count = 0;
    for (place, stop) in stops.iter().enumerate() {
        for task in &stop.tasks {
            count += 1;
            if count > 10000 {
                return Err(fault(TOO_LARGE, "tasks"));
            }
            let id = token(&task.task_id).ok_or_else(|| fault(INCOMPLETE, "tasks"))?;
            let time = task.time();
            let facts = [&task.task_type, &task.task_state, &task.execution_status];
            // Amazon can repeat a task across overlapping stop groups. Identical facts
            // represent one event; conflicting copies cannot establish gaps.
            if let Some(first) = seen.get(id) {
                if *first != facts || times[id] != (time, &task.transporter_id) {
                    complete = false;
                    deliveries.retain(|(delivered, ..)| *delivered != id);
                }
                continue;
            }
            seen.insert(id, facts);
            times.insert(id, (time, &task.transporter_id));
            if !task.delivered() {
                continue;
            }
            let own =
                task.transporter_id.is_null() || task.transporter_id == c.transporter_id.as_str();
            match task.drop_off().filter(|_| own) {
                Some(time) => deliveries.push((id, time, place as u32)),
                None => complete = false,
            }
        }
    }
    let mut meals: Vec<Meal> = breaks
        .into_iter()
        .map(|meal| {
            let mut bounded = Meal {
                id: meal.id,
                start: meal.start,
                end: meal.end,
                last_delivery: None,
                first_delivery: None,
                last_delivery_stop: None,
                first_delivery_stop: None,
            };
            if complete {
                for &(_, time, stop) in &deliveries {
                    // Package completion times matter, including different completions
                    // within a group stop: latest before OUT, earliest after IN.
                    if time <= bounded.start && bounded.last_delivery.is_none_or(|last| time > last)
                    {
                        bounded.last_delivery = Some(time);
                        bounded.last_delivery_stop = Some(stop);
                    }
                    if bounded.end.is_some_and(|end| time >= end)
                        && bounded.first_delivery.is_none_or(|first| time < first)
                    {
                        bounded.first_delivery = Some(time);
                        bounded.first_delivery_stop = Some(stop);
                    }
                }
            }
            bounded
        })
        .collect();
    // Removed tasks have uncertain ownership. Their times can only invalidate a boundary if
    // they could be closer than the selected active delivery. Never substitute them for this
    // driver's delivery or discard a verified boundary because an unrelated removed task
    // occurred hours away.
    for task in inactive.iter().filter(|task| task.delivered()) {
        let closer = |meal: &Meal| match task.drop_off() {
            None => true,
            Some(time) => {
                (time <= meal.start && meal.last_delivery.is_none_or(|last| time > last))
                    || (meal.end.is_some_and(|end| time >= end)
                        && meal.first_delivery.is_none_or(|first| time < first))
            }
        };
        if meals.iter().any(closer) {
            complete = false;
        }
    }
    if !complete {
        for meal in &mut meals {
            (meal.last_delivery, meal.first_delivery) = (None, None);
            (meal.last_delivery_stop, meal.first_delivery_stop) = (None, None);
        }
    }
    let coverage = if complete {
        Coverage::Complete
    } else {
        Coverage::Unavailable
    };
    Ok(itinerary(coverage, meals, None))
}

#[cfg(test)]
#[path = "../../../tests/backend/collections/meals/rules.rs"]
mod tests;
