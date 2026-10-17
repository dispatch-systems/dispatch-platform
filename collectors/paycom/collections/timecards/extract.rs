//! A Paycom timecard response read in this process. It follows `timecard.js` on a
//! detached document: the same fields, the same validation and no layout. Parsing
//! matches `DOMParser` with scripting off, so no provider script runs. Anything a
//! browser would read differently fails closed; each job still proves these records
//! equal rendered ones before relying on them.
use chrono::{Datelike, NaiveDate};
use ego_tree::NodeRef;
use html5ever::{
    ParseOpts, QualName, local_name, ns,
    tendril::TendrilSink,
    tree_builder::{QuirksMode, TreeBuilderOpts},
};
use scraper::{ElementRef, Html, HtmlTreeSink, Node};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;

mod dom;
mod page;
mod text;

use dom::*;
use page::*;
use text::*;
/// Why a response was not read.
#[derive(Debug, PartialEq)]
pub(crate) enum Unreadable {
    /// The page names another employee: a rendered read would refuse it too.
    WrongEmployee,
    /// One of `timecard.js`'s validation codes, or a form this reader does not know.
    Invalid(&'static str),
}
type Read<T> = std::result::Result<T, Unreadable>;
fn invalid<T>(code: &'static str) -> Read<T> {
    Err(Unreadable::Invalid(code))
}

/// The timecard a response should hold.
pub(crate) struct Source<'a> {
    pub employee: &'a str,
    pub period: &'a Value,
    pub url: &'a str,
}

const HEADERS: &[&str] = &[
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
    "waiver",
    "comment",
    "missing-punch",
    "delete",
];
const SLOTS: [&str; 4] = ["i1", "o1", "i2", "o2"];
const KINDS: [&str; 4] = ["IN DAY", "OUT LUNCH", "IN LUNCH", "OUT DAY"];
const LABELS: [&str; 7] = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Change {
    change_operation: Option<&'static str>,
    current_kind: Option<&'static str>,
    current_time: Option<String>,
    requested_kind: Option<&'static str>,
    requested_time: Option<String>,
    change_note: Option<String>,
    change_detail_state: &'static str,
}
impl Change {
    fn empty(state: &'static str) -> Self {
        Self {
            change_operation: None,
            current_kind: None,
            current_time: None,
            requested_kind: None,
            requested_time: None,
            change_note: None,
            change_detail_state: state,
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Punch {
    ordinal: usize,
    row_index: usize,
    slot: &'static str,
    kind: String,
    display_time: String,
    actual_time: String,
    rounded_time: String,
    clock_name: String,
    clock_code: String,
    comment: String,
    provenance_available: bool,
    change_request_status: Option<&'static str>,
    approved: bool,
    #[serde(flatten)]
    change: Change,
}
/// What one table row holds, before it is placed as a day or an additional row.
struct Row {
    pay_code: String,
    allocation1: String,
    allocation2: String,
    hours: Option<f64>,
    total_hours: Option<f64>,
    dollars: Option<f64>,
    exception_text: String,
    waiver_checked: Option<bool>,
    comments: Vec<String>,
    unresolved_slots: Vec<&'static str>,
    punches: Vec<Punch>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Day {
    date: String,
    label: String,
    pay_code: String,
    allocation1: String,
    allocation2: String,
    hours: Option<f64>,
    total_hours: Option<f64>,
    dollars: Option<f64>,
    exception_text: String,
    waiver_checked: Option<bool>,
    comments: Vec<String>,
    missing_punch: bool,
    unresolved_slots: Vec<String>,
    punches: Vec<Punch>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AdditionalRow {
    date: String,
    row_index: usize,
    row_class: String,
    pay_code: String,
    allocation1: String,
    allocation2: String,
    hours: Option<f64>,
    total_hours: Option<f64>,
    dollars: Option<f64>,
    exception_text: String,
    waiver_checked: Option<bool>,
    comments: Vec<String>,
    unresolved_slots: Vec<&'static str>,
    punch_ordinals: Vec<usize>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    version: u8,
    source_format: &'static str,
    employee_code: String,
    period_start: String,
    period_end: String,
    period_key: String,
    source_url: String,
    page_title: String,
    headers: Vec<String>,
    days: Vec<Day>,
    additional_rows: Vec<AdditionalRow>,
    weekly_totals: Vec<Option<f64>>,
    period_total_hours: Option<f64>,
    approvals: Vec<Vec<String>>,
    attestations: Vec<Vec<String>>,
    meal_waivers: Vec<Vec<String>>,
}

/// The record `timecard.js` returns for this response, validated the same way.
pub(crate) fn timecard(html: &str, source: &Source) -> Read<Value> {
    let options = ParseOpts {
        tree_builder: TreeBuilderOpts {
            scripting_enabled: false,
            ..Default::default()
        },
        ..Default::default()
    };
    let document =
        html5ever::parse_document(HtmlTreeSink::new(Html::new_document()), options).one(html);
    let page = Page {
        document: &document,
        quirks: document.quirks_mode == QuirksMode::Quirks,
    };
    let record = page.record(source)?;
    validate(&record, source)?;
    serde_json::to_value(record).map_err(|_| Unreadable::Invalid("timecard_identity_invalid"))
}

/// The period a key names: fourteen days from a Sunday to a Saturday.
fn period(key: &str) -> Read<Vec<String>> {
    let (start, end) = key
        .split_once('_')
        .ok_or(Unreadable::Invalid("invalid_period"))?;
    let parse = |value: &str| {
        NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .ok()
            .filter(|d| d.to_string() == value)
            .ok_or(Unreadable::Invalid("invalid_period"))
    };
    let (start, end) = (parse(start)?, parse(end)?);
    if start.weekday() != chrono::Weekday::Sun
        || end.weekday() != chrono::Weekday::Sat
        || (end - start).num_days() != 13
    {
        return invalid("invalid_period");
    }
    Ok((0..14)
        .map(|i| (start + chrono::Duration::days(i)).to_string())
        .collect())
}
fn number_or_null(value: Option<f64>) -> bool {
    value.is_none_or(|v| v.is_finite() && v >= 0.)
}
fn comments_valid(comments: &[String]) -> bool {
    comments.len() <= 20 && comments.iter().all(|c| bounded(c, 2000))
}
fn generic_valid(rows: &[Vec<String>]) -> bool {
    rows.len() <= 500
        && rows
            .iter()
            .all(|row| row.len() <= 20 && row.iter().all(|cell| bounded(cell, 1000)))
}
/// `/^(?:[1-9]|1[0-6]):(?:i1|o1|i2|o2)$|^(?:i1|o1|i2|o2)$/`.
fn unresolved_valid(slot: &str) -> bool {
    match slot.split_once(':') {
        Some((row, name)) => SLOTS.contains(&name) && (1..=16).any(|n| n.to_string() == row),
        None => SLOTS.contains(&slot),
    }
}
/// `timecard.js`'s `validatePunch`, for what construction does not already guarantee.
fn punch_valid(punch: &Punch) -> Read<()> {
    if punch.row_index > 16 || !time(&punch.display_time) {
        return invalid("timecard_punch_invalid");
    }
    if !bounded(&punch.clock_name, 200)
        || !bounded(&punch.clock_code, 50)
        || !bounded(&punch.comment, 2000)
        || punch.provenance_available
            && (!KINDS.contains(&punch.kind.as_str())
                || !time(&punch.actual_time)
                || !time(&punch.rounded_time))
    {
        return invalid("timecard_provenance_invalid");
    }
    Ok(())
}
/// `timecard.js`'s `validateTimecardRecord`, for what construction does not guarantee.
fn validate(record: &Record, source: &Source) -> Read<()> {
    if source.employee.len() != 4 || !source.employee.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return invalid("invalid_employee_code");
    }
    let dates = period(source.period["key"].as_str().unwrap_or(""))?;
    if !record.employee_code.is_empty() && record.employee_code != source.employee {
        return Err(Unreadable::WrongEmployee);
    }
    if record.employee_code != source.employee
        || record.period_start != dates[0]
        || record.period_end != dates[13]
        || record.period_key != format!("{}_{}", dates[0], dates[13])
        || record.source_url != source.url
        || record.page_title != "Timecard Editor"
    {
        return invalid("timecard_identity_invalid");
    }
    let without_waiver = HEADERS.iter().filter(|h| **h != "waiver");
    if !(record
        .headers
        .iter()
        .map(String::as_str)
        .eq(HEADERS.iter().copied())
        || record
            .headers
            .iter()
            .map(String::as_str)
            .eq(without_waiver.copied()))
    {
        return invalid("timecard_header_invalid");
    }
    if record.days.len() != 14 {
        return invalid("timecard_day_count_invalid");
    }
    if record.additional_rows.len() > 200 {
        return invalid("timecard_additional_row_invalid");
    }
    for (index, day) in record.days.iter().enumerate() {
        if day.date != dates[index] {
            return invalid("timecard_date_sequence_invalid");
        }
        if day.label != LABELS[index % 7] {
            return invalid("timecard_day_label_invalid");
        }
        if !bounded(&day.pay_code, 100) {
            return invalid("timecard_pay_code_invalid");
        }
        if !bounded(&day.allocation1, 300) || !bounded(&day.allocation2, 300) {
            return invalid("timecard_allocation_invalid");
        }
        if !bounded(&day.exception_text, 2000) {
            return invalid("timecard_exception_invalid");
        }
        if ![day.hours, day.total_hours, day.dollars]
            .into_iter()
            .all(number_or_null)
        {
            return invalid("timecard_day_number_invalid");
        }
        if !comments_valid(&day.comments) {
            return invalid("timecard_day_comments_invalid");
        }
        if day.unresolved_slots.len() > 32
            || !day.unresolved_slots.iter().all(|s| unresolved_valid(s))
            || day.unresolved_slots.iter().collect::<HashSet<_>>().len()
                != day.unresolved_slots.len()
        {
            return invalid("timecard_missing_punch_invalid");
        }
        if day.punches.len() > 32 {
            return invalid("timecard_punch_invalid");
        }
        day.punches.iter().try_for_each(punch_valid)?;
    }
    let mut rows = HashSet::new();
    for row in &record.additional_rows {
        if !dates.contains(&row.date)
            || !(1..=16).contains(&row.row_index)
            || !rows.insert((row.date.as_str(), row.row_index))
            || !bounded(&row.row_class, 300)
            || !bounded(&row.pay_code, 2000)
            || !bounded(&row.allocation1, 300)
            || !bounded(&row.allocation2, 300)
            || ![row.hours, row.total_hours, row.dollars]
                .into_iter()
                .all(number_or_null)
            || !bounded(&row.exception_text, 2000)
            || !comments_valid(&row.comments)
        {
            return invalid("timecard_additional_row_invalid");
        }
    }
    let [first, second] = record.weekly_totals.as_slice() else {
        return invalid("timecard_weekly_total_invalid");
    };
    let (Some(first), Some(second)) = (*first, *second) else {
        return invalid("timecard_weekly_total_invalid");
    };
    if !number_or_null(Some(first)) || !number_or_null(Some(second)) {
        return invalid("timecard_weekly_total_invalid");
    }
    match record.period_total_hours {
        Some(total)
            if total.is_finite()
                && total >= 0.
                && (total - (0. + first + second)).abs() <= 0.011 => {}
        _ => return invalid("timecard_period_total_invalid"),
    }
    if !generic_valid(&record.approvals) {
        return invalid("timecard_approval_invalid");
    }
    if !generic_valid(&record.attestations) {
        return invalid("timecard_attestation_invalid");
    }
    if !generic_valid(&record.meal_waivers) {
        return invalid("timecard_waiver_invalid");
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/backend/collections/timecards/extract.rs"]
mod tests;
