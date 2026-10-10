//! DVIC short inspections: the drivers whose pre-trip inspections were shorter than the
//! minimum time, at one DSP, for chosen drivers, a day, or a range of days. DVIC records only
//! the inspections that fell short, so a driver it never names was never short in what it has
//! collected; and a day outside what it has collected is unknown, which every answer says.
use crate::toolbox::{Answer, Cx, Refusal, Reply, Tool};
use chrono::{Datelike, Duration, NaiveDate};
use dispatch_core::db::Store;
use dispatch_dvic::{DvicCoverage, DvicDriver, DvicInspection, DvicStore};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The short inspections an answer lists at most; it counts the rest.
const LISTED: usize = 100;
/// The short inspections one search reads at most.
const READ: usize = 5000;
/// The days one search covers at most.
const LONGEST: i64 = 366;

/// What to search for: some drivers, every driver, or both, over one way of naming the days.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Search {
    /// Drivers, each by name, part of a name, or Amazon transporter ID; none for everyone.
    pub drivers: Option<Vec<String>>,
    /// One day, YYYY-MM-DD, in the DSP's own time.
    pub date: Option<String>,
    /// The first day of a range, YYYY-MM-DD; the range ends today unless `to` says otherwise.
    pub from: Option<String>,
    /// The last day of a range, YYYY-MM-DD; only with `from`.
    pub to: Option<String>,
    /// The last this many days, today included, 1 to 366: 3 is today and the two days before.
    pub last_days: Option<i64>,
    /// A week, Sunday to Saturday: `this` one so far, or `last` one.
    pub week: Option<Week>,
}
/// A week, Sunday to Saturday, as the DVIC page shows it.
#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Week {
    This,
    Last,
}

/// What the search found.
#[derive(Serialize, JsonSchema)]
pub struct Found {
    /// The DSP searched.
    pub dsp: String,
    /// The first day searched, YYYY-MM-DD, in the DSP's own time.
    pub from: String,
    /// The last day searched, YYYY-MM-DD, in the DSP's own time.
    pub to: String,
    /// The days DVIC has collected, or null for none yet; a day outside is unknown, not clean.
    pub collected: Option<Collected>,
    /// Whether every day searched is within what is collected.
    pub complete: bool,
    /// Each driver short of the minimum on the days searched, the most short inspections first.
    pub drivers: Vec<Driver>,
    /// The short inspections, newest first, at most 100.
    pub inspections: Vec<Inspection>,
    /// How many more short inspections there are than listed: ask for fewer days to see them.
    pub more: usize,
    /// The drivers asked for whom DVIC has never recorded short of the minimum.
    pub not_found: Vec<String>,
}
/// The days DVIC's collected reports cover.
#[derive(Serialize, JsonSchema)]
pub struct Collected {
    pub from: String,
    pub through: String,
}
/// A driver short of the minimum on the days searched.
#[derive(Serialize, JsonSchema)]
pub struct Driver {
    pub name: String,
    /// Their Amazon transporter ID.
    pub id: String,
    pub short_inspections: usize,
    /// The days they were short, oldest first.
    pub days: Vec<String>,
    /// How many seconds short their shortest inspection was of its minimum.
    pub most_short_by_seconds: f64,
}
/// An inspection shorter than its minimum.
#[derive(Serialize, JsonSchema)]
pub struct Inspection {
    pub date: String,
    /// When it started, as Amazon's report gives it, with no time zone.
    pub start_time: String,
    pub driver: String,
    /// The driver's Amazon transporter ID.
    pub driver_id: String,
    /// CV, CDV or SV: 90 seconds is the minimum for the first two, 300 for a step van.
    pub vehicle_type: String,
    pub vin: String,
    pub duration_seconds: f64,
    pub minimum_seconds: usize,
    pub short_by_seconds: f64,
}

/// Searches DVIC for inspections shorter than the minimum.
pub struct DvicShortInspections;
impl Tool for DvicShortInspections {
    const NAME: &'static str = "dvic_short_inspections";
    const TITLE: &'static str = "DVIC short inspections";
    const DESCRIPTION: &'static str = "Find the drivers whose DVIC pre-trip inspections were \
        shorter than the minimum time (90 seconds, 300 for a step van), at one DSP: for some \
        drivers, a day, or a range such as the last 3 days or last week. DVIC records only the \
        inspections that fell short. Answers each driver's count and days, the inspections, and \
        how far DVIC is collected, since a day it hasn't collected is unknown.";
    const FEATURES: &'static [&'static str] = &["dvic"];
    type Input = Search;
    type Output = Found;
    async fn call(cx: Cx, input: Search) -> Answer<Reply<Found>> {
        let dsp = cx.dsp().clone();
        let today = today(&dsp.timezone);
        let found = cx
            .read(move |db| search(db, &dsp.id, &dsp.name, today, &input))
            .await?;
        let text = summary(&found);
        Ok(Reply::new(found).text(text))
    }
}

/// The DSP's own date today, in its time zone.
fn today(timezone: &str) -> NaiveDate {
    let now = chrono::Utc::now();
    match timezone.parse::<chrono_tz::Tz>() {
        Ok(zone) => now.with_timezone(&zone).date_naive(),
        Err(_) => now.date_naive(),
    }
}

/// The search, read at once.
fn search(db: &Store, dsp: &str, name: &str, today: NaiveDate, input: &Search) -> Answer<Found> {
    let coverage = db.dvic_coverage(dsp)?;
    let known = db.dvic_drivers(dsp)?;
    let asked = input.drivers.as_deref().unwrap_or_default();
    let (matched, not_found) = matched(&known, asked)?;
    let (from, to) = match range(input, today)? {
        Some(range) => range,
        // Every day DVIC has for the drivers asked for, or the last 7.
        None if !asked.is_empty() => {
            let first = matched
                .iter()
                .map(|driver| driver.first_date.as_str())
                .min();
            (first.and_then(parse).unwrap_or(today).min(today), today)
        }
        None => (today - Duration::days(6), today),
    };
    let (from, to) = (from.to_string(), to.to_string());
    let mut read = Vec::new();
    if asked.is_empty() {
        read_all(db, dsp, &from, &to, None, &mut read)?;
    } else {
        for driver in &matched {
            read_all(db, dsp, &from, &to, Some(&driver.id), &mut read)?;
        }
    }
    read.sort_by(|a, b| {
        (b.start_date.as_str(), b.start_time.as_str())
            .cmp(&(a.start_date.as_str(), a.start_time.as_str()))
    });
    let complete = coverage.as_ref().is_some_and(
        |DvicCoverage {
             from: first,
             through,
         }| *first <= from && to <= *through,
    );
    let drivers = drivers(&read);
    let more = read.len().saturating_sub(LISTED);
    Ok(Found {
        dsp: name.to_owned(),
        from,
        to,
        collected: coverage.map(|DvicCoverage { from, through }| Collected { from, through }),
        complete,
        drivers,
        inspections: read.into_iter().take(LISTED).map(inspection).collect(),
        more,
        not_found,
    })
}

/// Every short inspection from `from` to `to`, of `driver` or everyone, read a page at a time.
fn read_all(
    db: &Store,
    dsp: &str,
    from: &str,
    to: &str,
    driver: Option<&str>,
    read: &mut Vec<DvicInspection>,
) -> Answer<()> {
    let mut after = String::new();
    loop {
        let page = db.dvic_inspections(dsp, from, to, driver, &after, 500)?;
        read.extend(page.inspections);
        if read.len() > READ {
            let message =
                format!("More than {READ} short inspections match. Ask for fewer days or drivers.");
            return Err(Refusal::new("too_many", message).into());
        }
        match page.next_cursor {
            Some(next) => after = next,
            None => return Ok(()),
        }
    }
}

/// The days a search names, or none for its default: one way at most, each day a real one,
/// none after today, and at most a year.
fn range(input: &Search, today: NaiveDate) -> Result<Option<(NaiveDate, NaiveDate)>, Refusal> {
    let invalid = |message: &str| Refusal::new("invalid_input", message);
    let ways = [
        input.date.is_some(),
        input.from.is_some() || input.to.is_some(),
        input.last_days.is_some(),
        input.week.is_some(),
    ];
    if ways.iter().filter(|given| **given).count() > 1 {
        return Err(invalid(
            "Give one of `date`, `from` and `to`, `last_days` or `week`.",
        ));
    }
    let day = |name: &str, value: &str| {
        parse(value).ok_or_else(|| invalid(&format!("`{name}` is a day, as YYYY-MM-DD.")))
    };
    let range = if let Some(date) = &input.date {
        let date = day("date", date)?;
        (date, date)
    } else if let Some(from) = &input.from {
        let from = day("from", from)?;
        let to = input.to.as_deref().map(|to| day("to", to)).transpose()?;
        (from, to.unwrap_or(today))
    } else if input.to.is_some() {
        return Err(invalid("`to` ends a range that `from` starts."));
    } else if let Some(days) = input.last_days {
        if !(1..=LONGEST).contains(&days) {
            return Err(invalid("`last_days` is 1 to 366."));
        }
        (today - Duration::days(days - 1), today)
    } else if let Some(week) = input.week {
        let sunday = today - Duration::days(i64::from(today.weekday().num_days_from_sunday()));
        match week {
            Week::This => (sunday, today),
            Week::Last => (sunday - Duration::days(7), sunday - Duration::days(1)),
        }
    } else {
        return Ok(None);
    };
    let (from, to) = range;
    if from > to {
        return Err(invalid("`from` comes after `to`."));
    }
    if to > today {
        return Err(invalid(&format!("The range ends after today, {today}.")));
    }
    if (to - from).num_days() >= LONGEST {
        return Err(invalid("Ask for a year at most."));
    }
    Ok(Some((from, to)))
}

fn parse(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d").ok()
}

/// The drivers each name asked for is, and the names DVIC has never recorded. A name that
/// could be several drivers is refused, with their names to choose from.
fn matched<'a>(
    known: &'a [DvicDriver],
    asked: &[String],
) -> Result<(Vec<&'a DvicDriver>, Vec<String>), Refusal> {
    let mut found: Vec<&DvicDriver> = Vec::new();
    let mut missing = Vec::new();
    for name in asked {
        let name = name.trim();
        if name.is_empty() || name.len() > 100 {
            return Err(Refusal::new(
                "invalid_input",
                "Name each driver, in 100 characters at most.",
            ));
        }
        let by_id: Vec<&DvicDriver> = known
            .iter()
            .filter(|driver| driver.id.eq_ignore_ascii_case(name))
            .collect();
        let candidates = if by_id.is_empty() {
            let named: Vec<&DvicDriver> = known
                .iter()
                .filter(|driver| names(name, &driver.name))
                .collect();
            // A name exactly as DVIC writes it is that driver, beside others it is part of.
            let exact: Vec<&DvicDriver> = named
                .iter()
                .copied()
                .filter(|driver| words(&driver.name) == words(name))
                .collect();
            if exact.len() == 1 { exact } else { named }
        } else {
            by_id
        };
        match candidates.as_slice() {
            [] => missing.push(name.to_owned()),
            [one] => {
                if !found.iter().any(|driver| driver.id == one.id) {
                    found.push(one);
                }
            }
            several => {
                let choices = several
                    .iter()
                    .map(|driver| format!("{} ({})", driver.name, driver.id))
                    .collect();
                let message = format!("`{name}` could be any of these drivers. Ask which one.");
                return Err(Refusal::new("driver_ambiguous", message).choices(choices));
            }
        }
    }
    Ok((found, missing))
}

/// A name's words, lowercase, without punctuation.
fn words(name: &str) -> Vec<String> {
    name.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}
/// Whether `asked` names `name`: each of its words is the start of one of the name's, or one
/// of the name's is the start of it, as an initial is, or it is within one, as a joined
/// surname holds both.
fn names(asked: &str, name: &str) -> bool {
    let asked = words(asked);
    let mut name = words(name);
    !asked.is_empty()
        && asked.iter().all(|word| {
            let at = name.iter().position(|part| {
                part.starts_with(word.as_str())
                    || word.starts_with(part.as_str())
                    || (word.len() >= 3 && part.contains(word.as_str()))
            });
            at.map(|at| name.remove(at)).is_some()
        })
}

/// Each driver's short inspections, the most first, then by name.
fn drivers(read: &[DvicInspection]) -> Vec<Driver> {
    let mut by: BTreeMap<&str, Driver> = BTreeMap::new();
    for row in read.iter().rev() {
        let driver = by.entry(row.driver_id.as_str()).or_insert_with(|| Driver {
            name: row.driver_name.clone(),
            id: row.driver_id.clone(),
            short_inspections: 0,
            days: vec![],
            most_short_by_seconds: 0.0,
        });
        // Read newest last, so the name is the latest.
        driver.name.clone_from(&row.driver_name);
        driver.short_inspections += 1;
        if driver.days.last() != Some(&row.start_date) {
            driver.days.push(row.start_date.clone());
        }
        driver.most_short_by_seconds = driver.most_short_by_seconds.max(row.short_by_seconds);
    }
    let mut drivers: Vec<Driver> = by.into_values().collect();
    drivers.sort_by(|a, b| {
        b.short_inspections
            .cmp(&a.short_inspections)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    drivers
}

fn inspection(row: DvicInspection) -> Inspection {
    Inspection {
        date: row.start_date,
        start_time: row.start_time,
        driver: row.driver_name,
        driver_id: row.driver_id,
        vehicle_type: row.fleet_type,
        vin: row.vin,
        duration_seconds: row.duration_seconds,
        minimum_seconds: row.minimum_seconds,
        short_by_seconds: row.short_by_seconds,
    }
}

/// A day as people read it: Oct 4.
fn day(date: &str) -> String {
    parse(date).map_or_else(|| date.to_owned(), |date| date.format("%b %-d").to_string())
}
/// What the answer says, in a sentence or two, for the agent to read out.
fn summary(found: &Found) -> String {
    let days = if found.from == found.to {
        day(&found.from)
    } else {
        format!("{} to {}", day(&found.from), day(&found.to))
    };
    let count: usize = found
        .drivers
        .iter()
        .map(|driver| driver.short_inspections)
        .sum();
    let mut text = match (count, found.drivers.len()) {
        (0, _) => format!("No short DVIC inspections at {}, {days}.", found.dsp),
        (1, _) => format!("1 short DVIC inspection at {}, {days}.", found.dsp),
        (count, 1) => format!(
            "{count} short DVIC inspections by 1 driver at {}, {days}.",
            found.dsp
        ),
        (count, drivers) => {
            format!(
                "{count} short DVIC inspections by {drivers} drivers at {}, {days}.",
                found.dsp
            )
        }
    };
    match &found.collected {
        None => text.push_str(" DVIC has collected nothing at this DSP yet, so nothing is known."),
        Some(collected) if !found.complete => text.push_str(&format!(
            " DVIC is collected from {} through {}; days outside that are not known.",
            day(&collected.from),
            day(&collected.through)
        )),
        Some(_) => {}
    }
    if !found.not_found.is_empty() {
        text.push_str(&format!(
            " DVIC has never recorded {} short.",
            found.not_found.join(", ")
        ));
    }
    text
}

#[cfg(test)]
#[path = "../tests/backend/tools/dvic_short_inspections.rs"]
mod tests;
