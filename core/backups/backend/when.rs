//! When a backup rule runs, where the DSP is: on chosen days of the week or dates of the month,
//! at set times or every so often between two times. A date a month doesn't have, as the 31st of
//! a 30-day month, runs on its last day.
use crate::{
    Error, Result, ensure,
    foundation::{timing, validate as v},
};
use chrono::{Datelike, Duration, NaiveDate};
use chrono_tz::Tz;
use serde_json::Value;

pub use crate::foundation::timing::{EVERY_DAY, Timing, text, time};

/// The days a rule runs on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum On {
    /// These days of the week, Monday the lowest bit.
    Week(i64),
    /// These dates of the month, the 1st bit 1, and its last day if `last`.
    Month { dates: i64, last: bool },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct When {
    pub on: On,
    pub timing: Timing,
}

/// The numbers in `value[key]`, each once, each within `range`, as one bit each.
fn mask(value: &Value, key: &str, range: std::ops::RangeInclusive<i64>) -> Result<i64> {
    let invalid = || Error::new("invalid_schedule_days", 400);
    let mut mask = 0;
    for item in value[key].as_array().ok_or_else(invalid)? {
        let item = item
            .as_i64()
            .filter(|item| range.contains(item))
            .ok_or_else(invalid)?;
        ensure(mask & (1 << item) == 0, "invalid_schedule_days", 400)?;
        mask |= 1 << item;
    }
    Ok(mask)
}

impl When {
    /// A rule as a request states it: `on`, `week` with `days`, Monday 0 to Sunday 6, or `month`
    /// with `dates`, 1 to 31, and `lastDay`; then `kind`, `times` or `repeat` with `every`
    /// minutes `from` `to`.
    pub fn parse(value: &Value) -> Result<Self> {
        let on = match v::choice(value, "on", &["week", "month"])? {
            "week" => {
                let days = mask(value, "days", 0..=6)?;
                ensure(days != 0, "invalid_schedule_days", 400)?;
                On::Week(days)
            }
            _ => {
                let dates = mask(value, "dates", 1..=31)?;
                let last = v::boolean(value, "lastDay")?;
                ensure(dates != 0 || last, "invalid_schedule_days", 400)?;
                On::Month { dates, last }
            }
        };
        let timing = Timing::parse(value)?;
        Ok(Self { on, timing })
    }

    /// Whether it runs on `day`: one of its days of the week, or one of its dates, a date past
    /// the month's end on its last day.
    pub fn runs_on(&self, day: NaiveDate) -> bool {
        match self.on {
            On::Week(days) => days & (1 << day.weekday().num_days_from_monday()) != 0,
            On::Month { dates, last } => {
                let date = day.day() as i64;
                let length = day.num_days_in_month() as i64;
                dates & (1 << date) != 0 || (date == length && (last || dates >> (length + 1) != 0))
            }
        }
    }

    /// The first time it runs after `after` (milliseconds since the epoch) in `tz`. A local time
    /// a clock change repeats runs once, at its first; one it skips doesn't run.
    pub fn next(&self, tz: Tz, after: i64) -> Option<i64> {
        // Through two months and a week, so a date only some months have, or a time a clock
        // change skips, finds its next.
        self.timing.next(tz, after, 70, |day| self.runs_on(day))
    }
}

/// The next `count` times `rules` run after `after`, in `tz`, each once.
pub fn upcoming(rules: &[When], tz: Tz, after: i64, count: usize) -> Vec<i64> {
    timing::upcoming(after, count, |from| {
        rules.iter().filter_map(|rule| rule.next(tz, from)).min()
    })
}

/// Which of `rules` run on each of the `days` days from `today` where the DSP is: each day's
/// date, and the places of the rules that run on it.
pub fn calendar(rules: &[When], today: NaiveDate, days: i64) -> Vec<(NaiveDate, Vec<usize>)> {
    (0..days)
        .map(|offset| today + Duration::days(offset))
        .map(|day| {
            let running = rules
                .iter()
                .enumerate()
                .filter(|(_, rule)| rule.runs_on(day))
                .map(|(place, _)| place)
                .collect();
            (day, running)
        })
        .collect()
}

#[cfg(test)]
#[path = "../tests/backend/when.rs"]
mod tests;
