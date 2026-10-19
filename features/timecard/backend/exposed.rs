//! What Timecard exposes to every other feature and agent tool: `timecards`, each driver's
//! days as its own pages read them, through its contract (`dispatch_timecard_contract`).
use super::punches::daily_sources;
use crate::api::{
    assessment::AssessedClock,
    types::{EmployeeTimecard, Timecard},
};
use chrono::NaiveDate;
use chrono_tz::Tz;
use dispatch_core::{
    Error, Result, db::Store, ensure, foundation::validate, manifest::people::Source,
};
use dispatch_paycom as paycom;
use dispatch_timecard_contract::{Collected, Day, Days, Meal, Punch, PunchKind, Timecards};

/// The most days one read covers.
const LONGEST: i64 = 31;
/// The most IDs one driver's read names.
const IDS: usize = 16;

/// Timecard's timecards, as `Timecards` reads them.
pub(crate) struct Exposed;
pub(crate) static EXPOSED: Exposed = Exposed;

impl Timecards for Exposed {
    fn source(&self) -> Source {
        paycom::SOURCE
    }
    fn collected(&self, db: &Store, dsp: &str) -> Result<Option<Collected>> {
        let (_, zone) = zone(db, dsp)?;
        // Each period as its newest collection of everyone left it: one employee's sync speaks
        // only for them.
        let periods = db
            .collector(dsp, paycom::PROVIDER)?
            .query_as::<(String, String, String)>(
                "SELECT period_from,period_to,max(collected_at) FROM publications \
                 GROUP BY period_from,period_to",
                [],
            )?;
        Ok(periods
            .into_iter()
            .filter_map(|(from, to, at)| {
                let through = to.min(local_day(&at, zone)?);
                (through >= from).then_some(Collected { from, through })
            })
            .reduce(|one, other| Collected {
                from: one.from.min(other.from),
                through: one.through.max(other.through),
            }))
    }
    fn timecards(
        &self,
        db: &Store,
        dsp: &str,
        ids: &[String],
        from: &str,
        to: &str,
    ) -> Result<Days> {
        let day = |date: &str| {
            validate::date(date)?;
            NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|_| Error::new("invalid_date", 400))
        };
        let (first, last) = (day(from)?, day(to)?);
        ensure(
            (0..LONGEST).contains(&(last - first).num_days()),
            "invalid_input",
            400,
        )?;
        ensure(
            ids.len() <= IDS && ids.iter().all(|id| (1..=128).contains(&id.chars().count())),
            "invalid_input",
            400,
        )?;
        let (timezone, zone) = zone(db, dsp)?;
        let mut read = Days {
            days: vec![],
            unknown: vec![],
            partial: vec![],
        };
        for (date, source) in daily_sources(db, dsp, from, to, Some(ids))? {
            // A day after the collection that covers it is still to come in Paycom's period,
            // and the collection's own day was under way.
            let reached = source
                .collected_at
                .as_deref()
                .and_then(|at| local_day(at, zone));
            match reached {
                Some(reached) if date < reached => {}
                Some(reached) if date == reached => read.partial.push(date),
                _ => {
                    read.unknown.push(date);
                    continue;
                }
            }
            for row in source.rows.into_values() {
                let card = serde_json::from_value::<Timecard>(row)?;
                if card.punches.is_empty() && card.hours == 0.0 {
                    continue;
                }
                read.days.push(as_day(card.assessed(&timezone)));
            }
        }
        Ok(read)
    }
}

/// The DSP's timezone, as its timecards' clocks are written in.
fn zone(db: &Store, dsp: &str) -> Result<(String, Tz)> {
    let timezone = db.find_dsp(dsp)?.timezone;
    let zone = timezone
        .parse()
        .map_err(|_| Error::new("invalid_timezone", 400))?;
    Ok((timezone, zone))
}

/// The day a collection's time falls on in the DSP's own time.
fn local_day(at: &str, zone: Tz) -> Option<String> {
    let at = chrono::DateTime::parse_from_rfc3339(at).ok()?;
    Some(at.with_timezone(&zone).date_naive().to_string())
}

fn as_day(assessed: EmployeeTimecard) -> Day {
    let EmployeeTimecard {
        card,
        assessment,
        regular_minutes,
        overtime_minutes,
    } = assessed;
    Day {
        date: card.date,
        id: card.employee_code,
        status: card.status,
        hours: card.hours,
        worked_minutes: regular_minutes
            .zip(overtime_minutes)
            .map(|(regular, overtime)| regular + overtime),
        punches: assessment
            .events
            .iter()
            .map(|event| Punch {
                kind: match event.kind.as_str() {
                    "IN DAY" => PunchKind::InDay,
                    "OUT LUNCH" => PunchKind::OutLunch,
                    "IN LUNCH" => PunchKind::InLunch,
                    "OUT DAY" => PunchKind::OutDay,
                    _ => PunchKind::Unlabeled,
                },
                time: event.time.as_ref().map(clock),
                next_day: event.time.as_ref().is_some_and(|time| time.day > 0),
            })
            .collect(),
        meals: assessment
            .lunches
            .iter()
            .map(|lunch| Meal {
                out: lunch.out.as_ref().map(clock),
                back: lunch.clock_in.as_ref().map(clock),
                minutes: lunch
                    .out
                    .as_ref()
                    .zip(lunch.clock_in.as_ref())
                    .map(|(out, back)| back.minute - out.minute)
                    .filter(|minutes| *minutes >= 0),
            })
            .collect(),
        review: assessment.review,
    }
}

/// A punch's clock, HH:MM, whatever day it fell on.
fn clock(time: &AssessedClock) -> String {
    let minute = time.minute - time.day * 1440;
    format!("{:02}:{:02}", minute / 60, minute % 60)
}

#[cfg(test)]
#[path = "../tests/backend/exposed.rs"]
mod tests;
