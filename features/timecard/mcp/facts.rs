//! Timecard's facts for agents: Paycom's timecards and the meal-break comparison, by
//! driver and day, when each was last collected, and how they join a driver's days and the
//! team's table.
use super::{MEAL_BREAKS, TIMECARDS};
use crate::{
    api::{assessment::MealStatus, types::DailyTimecard},
    backend::{TimecardStore, punches::assessment::paycom_day},
};
use dispatch_core::{
    Result,
    accounts::api::types::Dsp,
    db::{Store, s},
    mcp::{
        api::types::{AgentArea, DriverSource},
        data::{
            facts::{Coverage, Daily, Facts, total_minutes, zone},
            scope::{People, Period, Person},
            shape::hours,
        },
    },
};
use dispatch_cortex as cortex;
use dispatch_paycom as paycom;
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// One employee's timecard for one day, as Paycom recorded it.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimecardDay {
    pub date: String,
    pub employee_code: String,
    pub name: String,
    pub hours: f64,
    pub clock_in: Option<String>,
    pub clock_out: Option<String>,
    pub lunch_minutes: Option<i64>,
    pub status: String,
    /// Punches Paycom recorded that do not read as a whole day.
    pub needs_review: bool,
}
fn clock_of(minute: i32, day: i32) -> String {
    let minute = minute.rem_euclid(1440);
    let next = match day {
        0 => String::new(),
        1 => " +1 day".into(),
        day => format!(" +{day} days"),
    };
    format!("{:02}:{:02}{next}", minute / 60, minute % 60)
}
fn timecard_day(row: DailyTimecard) -> TimecardDay {
    let card = row.card;
    let assessed = paycom_day(&card.status, &card.punches);
    let lunch = total_minutes(assessed.lunches.iter().map(|l| {
        let (out, back) = (l.out.as_ref()?, l.clock_in.as_ref()?);
        Some(i64::from(back.minute - out.minute))
    }));
    TimecardDay {
        date: card.date,
        employee_code: card.employee_code,
        name: row.name,
        hours: card.hours,
        clock_in: assessed.in_day.as_ref().map(|c| clock_of(c.minute, c.day)),
        clock_out: assessed.out_day.as_ref().map(|c| clock_of(c.minute, c.day)),
        lunch_minutes: lunch,
        status: card.status,
        needs_review: assessed.review,
    }
}

/// Timecards in a period, every employee's or only `codes`'.
pub fn timecards(
    db: &Store,
    dsp: &Dsp,
    period: &Period,
    codes: Option<&[String]>,
) -> Result<(Vec<TimecardDay>, Coverage)> {
    let mut found = vec![];
    let mut held = BTreeSet::new();
    // Raw source rows include punch arrays and source metadata. Convert each
    // bounded batch before loading the next, even for a whole-team question.
    let days = period.days();
    for chunk in days.chunks(7) {
        for (day, daily) in db.daily_timecards_range(
            &dsp.id,
            &chunk[0],
            chunk.last().unwrap(),
            "employeeName",
            false,
            codes,
        )? {
            if daily.available {
                held.insert(day.clone());
            }
            found.extend(daily.rows.into_iter().map(timecard_day));
        }
    }
    Ok((found, Coverage::of(true, held, period)))
}

/// One driver's meal breaks on one day: Cortex's meals beside Paycom's lunch punches.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MealDay {
    pub date: String,
    /// `paycom:<code>` or `cortex:<transporter ID>`, as the comparison joins them.
    pub source: String,
    pub name: String,
    pub status: MealStatus,
    pub meals: Vec<MealTaken>,
    pub late_clock_in: bool,
    pub long_gap: bool,
    /// Whether Cortex had a route for the person that day. Without one, as for office
    /// staff, the status says nothing about a missed meal.
    pub cortex_route: bool,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MealTaken {
    pub start: Option<String>,
    pub end: Option<String>,
    pub minutes: Option<i64>,
}
/// Every day's meal-break comparison in a period.
pub fn meal_breaks_for(
    db: &Store,
    dsp: &Dsp,
    period: &Period,
    sources: Option<&[String]>,
) -> Result<(Vec<MealDay>, Coverage)> {
    let mut found = vec![];
    let mut held = BTreeSet::new();
    let zone = zone(dsp);
    let local = |at: &str| {
        chrono::DateTime::parse_from_rfc3339(at)
            .ok()
            .map(|t| t.with_timezone(&zone))
    };
    let days = period.days();
    for chunk in days.chunks(7) {
        for (day, comparison) in db.meal_comparisons(
            &dsp.id,
            &chunk[0],
            chunk.last().unwrap(),
            &dsp.timezone,
            sources,
        )? {
            if !comparison.cortex_publications.is_empty() {
                held.insert(day.clone());
            }
            for row in comparison.rows {
                let meals = row
                    .source
                    .cortex
                    .iter()
                    .map(|meal| {
                        let (start, end) =
                            (local(&meal.start), meal.end.as_deref().and_then(local));
                        MealTaken {
                            start: start.map(|t| t.format("%H:%M").to_string()),
                            end: end.map(|t| t.format("%H:%M").to_string()),
                            minutes: start.zip(end).map(|(a, b)| (b - a).num_minutes()),
                        }
                    })
                    .collect();
                found.push(MealDay {
                    date: day.clone(),
                    source: row.source.id,
                    name: row.source.name,
                    status: row.assessment.status,
                    meals,
                    late_clock_in: row.assessment.late_in,
                    long_gap: row.assessment.long_gap,
                    cortex_route: false,
                });
            }
        }
    }
    Ok((found, Coverage::of(true, held, period)))
}

/// Each day's drivers Cortex had a route for, by transporter ID: those who could have
/// taken a meal break in it.
pub fn meal_routes(db: &Store, dsp: &Dsp, period: &Period) -> Result<BTreeSet<(String, String)>> {
    let rows = db.collector(&dsp.id, cortex::PROVIDER)?.all(
        "SELECT p.report_date day,i.transporter_id id FROM meal_itineraries i \
         JOIN meal_publications p ON p.id=i.publication_id AND p.active=1 \
         WHERE p.report_date BETWEEN ? AND ?",
        [&period.first(), &period.last()],
    )?;
    Ok(rows
        .iter()
        .map(|r| (s(r, "day").to_owned(), s(r, "id").to_owned()))
        .collect())
}

/// When Paycom last brought timecards in, for the status answer.
pub fn fresh_timecards(db: &Store, dsp: &Dsp, _station: &str) -> Result<Option<Value>> {
    let paycom = db.collector(&dsp.id, paycom::PROVIDER)?.one(
        "SELECT collected_at,period_from,period_to FROM publications WHERE active=1",
        [],
    )?;
    Ok(paycom.map(|r| {
        serde_json::json!({
            "collectedAt": r["collected_at"], "periodFrom": r["period_from"], "periodTo": r["period_to"]
        })
    }))
}
/// The latest day Cortex's meal breaks were collected for.
pub fn fresh_meals(db: &Store, dsp: &Dsp, _station: &str) -> Result<Option<Value>> {
    let meals = db.collector(&dsp.id, cortex::PROVIDER)?.one(
        "SELECT max(report_date) day,max(collected_at) collected_at FROM meal_publications WHERE active=1",
        [],
    )?;
    Ok(meals.filter(|r| !r["day"].is_null()).map(|r| {
        serde_json::json!({
            "latestDay": r["day"], "collectedAt": r["collected_at"]
        })
    }))
}

/// Marks each meal row whose person Cortex had a route for that day.
pub(super) fn mark_routes(
    db: &Store,
    dsp: &Dsp,
    period: &Period,
    people: &People,
    meals: &mut [MealDay],
) -> dispatch_core::Result<()> {
    let routes = meal_routes(db, dsp, period)?;
    for meal in meals {
        let (kind, id) = meal.source.split_once(':').unwrap_or(("", ""));
        let ids: Vec<String> = if kind == "cortex" {
            vec![id.to_owned()]
        } else {
            people
                .holder(DriverSource::Paycom, id)
                .map(|p| p.amazon.clone())
                .unwrap_or_default()
        };
        meal.cortex_route = ids
            .into_iter()
            .any(|id| routes.contains(&(meal.date.clone(), id)));
    }
    Ok(())
}
/// A meal break to look at: the comparison found something on a day the driver had a route.
pub(super) fn meal_issue(meal: &MealDay) -> bool {
    meal.cortex_route && meal.status != MealStatus::Same
}
pub(super) fn meal_span(meal: &MealDay) -> (Value, Value) {
    let spans: Vec<String> = meal
        .meals
        .iter()
        .map(|m| {
            format!(
                "{}–{}",
                m.start.as_deref().unwrap_or("?"),
                m.end.as_deref().unwrap_or("?")
            )
        })
        .collect();
    let minutes = total_minutes(meal.meals.iter().map(|m| m.minutes));
    (
        if spans.is_empty() {
            Value::Null
        } else {
            json!(spans.join(", "))
        },
        json!(minutes),
    )
}

/// Timecards, for a driver's days and the team's table.
pub struct TimecardDays;
struct Carded(Vec<TimecardDay>, Coverage);
impl Daily for TimecardDays {
    fn area(&self) -> AgentArea {
        TIMECARDS
    }
    fn coverage(&self) -> &'static str {
        "timecards"
    }
    fn records(&self) -> &'static str {
        "timecards"
    }
    fn columns(&self) -> &'static [&'static str] {
        &["hours", "in", "out"]
    }
    fn assessed(&self) -> bool {
        true
    }
    fn pooled(&self) -> &'static [&'static str] {
        &["lunch_minutes"]
    }
    fn gather(
        &self,
        db: &Store,
        dsp: &Dsp,
        period: &Period,
        _: &People,
        person: Option<&Person>,
    ) -> dispatch_core::Result<Box<dyn Facts>> {
        let (rows, coverage) = timecards(db, dsp, period, person.map(|p| p.paycom.as_slice()))?;
        Ok(Box::new(Carded(rows, coverage)))
    }
    fn none(&self) -> Box<dyn Facts> {
        Box::new(Carded(vec![], Coverage::default()))
    }
}
impl Facts for Carded {
    fn coverage(&self) -> &Coverage {
        &self.1
    }
    fn count(&self) -> usize {
        self.0.len()
    }
    fn whose(&self, record: usize) -> (&str, DriverSource, &str, &str) {
        let card = &self.0[record];
        (
            &card.date,
            DriverSource::Paycom,
            &card.employee_code,
            &card.name,
        )
    }
    fn record(&self, record: usize) -> Value {
        json!(self.0[record])
    }
    fn line(&self, _date: &str, records: &[usize]) -> Vec<Value> {
        let (mut worked, mut clock_in, mut clock_out) = (None, None, None);
        for card in records.iter().map(|&r| &self.0[r]) {
            worked = Some(worked.unwrap_or(0.0) + card.hours);
            clock_in = clock_in.take().or(card.clock_in.clone());
            clock_out = card.clock_out.clone().or(clock_out.take());
        }
        vec![
            worked.map(hours).unwrap_or(Value::Null),
            json!(clock_in),
            json!(clock_out),
        ]
    }
    fn totals(&self) -> Vec<(&'static str, Value)> {
        let worked: f64 = self.0.iter().map(|c| c.hours).sum();
        vec![
            ("hours_worked", hours(worked)),
            (
                "days_worked",
                json!(self.0.iter().filter(|c| c.hours > 0.0).count()),
            ),
        ]
    }
    fn metric(&self, name: &str, records: &[usize]) -> Option<Value> {
        let cards = || records.iter().map(|&r| &self.0[r]);
        match name {
            "hours_worked" => Some(hours(cards().map(|c| c.hours).sum())),
            "days_worked" => Some(json!(cards().filter(|c| c.hours > 0.0).count())),
            "lunch_minutes" => Some(json!(total_minutes(cards().map(|c| c.lunch_minutes)))),
            "clock_in" => self.0[*records.first()?].clock_in.clone().map(Value::from),
            "clock_out" => self.0[*records.last()?].clock_out.clone().map(Value::from),
            _ => None,
        }
    }
}

/// Meal breaks, for a driver's days and the team's table: each day's comparison, marked
/// where Cortex had a route for the person.
pub struct MealDays;
struct Compared(Vec<MealDay>, Coverage);
impl Daily for MealDays {
    fn area(&self) -> AgentArea {
        MEAL_BREAKS
    }
    fn coverage(&self) -> &'static str {
        "mealBreaks"
    }
    fn records(&self) -> &'static str {
        "mealBreaks"
    }
    fn columns(&self) -> &'static [&'static str] {
        &["meal"]
    }
    fn assessed(&self) -> bool {
        true
    }
    fn gather(
        &self,
        db: &Store,
        dsp: &Dsp,
        period: &Period,
        people: &People,
        person: Option<&Person>,
    ) -> dispatch_core::Result<Box<dyn Facts>> {
        let sources = person.map(|p| {
            p.paycom
                .iter()
                .map(|c| format!("paycom:{c}"))
                .chain(p.amazon.iter().map(|id| format!("cortex:{id}")))
                .collect::<Vec<_>>()
        });
        let (mut rows, coverage) = meal_breaks_for(db, dsp, period, sources.as_deref())?;
        mark_routes(db, dsp, period, people, &mut rows)?;
        Ok(Box::new(Compared(rows, coverage)))
    }
    fn none(&self) -> Box<dyn Facts> {
        Box::new(Compared(vec![], Coverage::default()))
    }
}
impl Facts for Compared {
    fn coverage(&self) -> &Coverage {
        &self.1
    }
    fn count(&self) -> usize {
        self.0.len()
    }
    fn whose(&self, record: usize) -> (&str, DriverSource, &str, &str) {
        let meal = &self.0[record];
        let (kind, id) = meal.source.split_once(':').unwrap_or(("", ""));
        let source = if kind == "paycom" {
            DriverSource::Paycom
        } else {
            DriverSource::Amazon
        };
        (&meal.date, source, id, &meal.name)
    }
    fn record(&self, record: usize) -> Value {
        json!(self.0[record])
    }
    fn lined(&self, record: usize) -> bool {
        self.0[record].cortex_route
    }
    fn line(&self, _date: &str, records: &[usize]) -> Vec<Value> {
        vec![json!(records.last().map(|&r| self.0[r].status))]
    }
    fn totals(&self) -> Vec<(&'static str, Value)> {
        vec![(
            "meal_issues",
            json!(self.0.iter().filter(|m| meal_issue(m)).count()),
        )]
    }
    fn metric(&self, name: &str, records: &[usize]) -> Option<Value> {
        let meals = || records.iter().map(|&r| &self.0[r]);
        match name {
            "meal_issues" => Some(json!(meals().filter(|m| meal_issue(m)).count())),
            "meal_status" => meals().next().map(|m| json!(m.status)),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "../tests/backend/mcp/facts.rs"]
mod tests;
