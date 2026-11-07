//! When a backup rule runs, where the DSP is: on chosen days of the week or dates of the month,
//! at set times or every so often between two times. A date a month doesn't have, as the 31st of
//! a 30-day month, runs on its last day.
use crate::{Error, Result, ensure, foundation::validate as v};
use chrono::{Datelike, Duration, NaiveDate, NaiveTime, TimeZone, Timelike};
use chrono_tz::Tz;
use serde_json::Value;

/// Every day of the week, Monday the lowest bit.
pub const EVERY_DAY: i64 = 0b111_1111;
/// The most times one rule runs at.
pub const MAX_TIMES: usize = 12;
/// How often a repeat may run: every half hour at the most, every twelve hours at the least, in
/// steps of half an hour.
pub const EVERY: (i64, i64, i64) = (30, 720, 30);
const DAY: i64 = 24 * 60;

/// The days a rule runs on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum On {
    /// These days of the week, Monday the lowest bit.
    Week(i64),
    /// These dates of the month, the 1st bit 1, and its last day if `last`.
    Month { dates: i64, last: bool },
}

/// At what time of a day a rule runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Timing {
    /// At each of these, earliest first.
    At(Vec<NaiveTime>),
    /// Every `every` minutes from `from` until `to`, `to` included. A `to` before `from` runs on
    /// past midnight; the same as `from`, all day.
    Repeat {
        every: i64,
        from: NaiveTime,
        to: NaiveTime,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct When {
    pub on: On,
    pub timing: Timing,
}

/// `HH:MM` as a time of day.
pub fn time(text: &str) -> Result<NaiveTime> {
    let parsed = NaiveTime::parse_from_str(text, "%H:%M")
        .map_err(|_| Error::new("invalid_schedule_time", 400))?;
    ensure(
        parsed.format("%H:%M").to_string() == text,
        "invalid_schedule_time",
        400,
    )?;
    Ok(parsed)
}
pub fn text(at: NaiveTime) -> String {
    at.format("%H:%M").to_string()
}
fn minutes(at: NaiveTime) -> i64 {
    i64::from(at.hour() * 60 + at.minute())
}
/// The numbers in `value[key]`, each once, each within `range`, as one bit each.
fn mask(value: &Value, key: &str, range: std::ops::RangeInclusive<i64>) -> Result<i64> {
    let invalid = || Error::new("invalid_schedule_days", 400);
    let mut mask = 0;
    for item in value[key].as_array().ok_or_else(invalid)? {
        let item = item.as_i64().filter(|item| range.contains(item)).ok_or_else(invalid)?;
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
        let timing = match v::choice(value, "kind", &["times", "repeat"])? {
            "times" => {
                let times = value["times"]
                    .as_array()
                    .filter(|times| (1..=MAX_TIMES).contains(&times.len()))
                    .ok_or_else(|| Error::new("invalid_schedule_time", 400))?;
                let mut parsed = times
                    .iter()
                    .map(|at| time(at.as_str().unwrap_or("")))
                    .collect::<Result<Vec<_>>>()?;
                parsed.sort();
                parsed.dedup();
                ensure(parsed.len() == times.len(), "invalid_schedule_time", 400)?;
                Timing::At(parsed)
            }
            _ => {
                let (least, most, step) = EVERY;
                let every = v::integer(value, "every", least, most)
                    .map_err(|_| Error::new("invalid_schedule_interval", 400))?;
                ensure(every % step == 0, "invalid_schedule_interval", 400)?;
                Timing::Repeat {
                    every,
                    from: time(v::text(value, "from", 5, 5)?)?,
                    to: time(v::text(value, "to", 5, 5)?)?,
                }
            }
        };
        Ok(Self { on, timing })
    }

    /// The minutes after the start of a day it runs on that it runs at, ascending: past a day's
    /// length, on the next day.
    fn offsets(&self) -> Vec<i64> {
        match &self.timing {
            Timing::At(times) => times.iter().map(|at| minutes(*at)).collect(),
            Timing::Repeat { every, from, to } => {
                let start = minutes(*from);
                let length = (minutes(*to) - start).rem_euclid(DAY);
                // All day runs up to the next day's first, which that day runs.
                let last = if length == 0 { DAY - 1 } else { length };
                (0..=last / every).map(|at| start + at * every).collect()
            }
        }
    }

    /// Whether it runs on `day`: one of its days of the week, or one of its dates, a date past
    /// the month's end on its last day.
    pub fn runs_on(&self, day: NaiveDate) -> bool {
        match self.on {
            On::Week(days) => days & (1 << day.weekday().num_days_from_monday()) != 0,
            On::Month { dates, last } => {
                let date = day.day() as i64;
                let length = day.num_days_in_month() as i64;
                dates & (1 << date) != 0
                    || (date == length && (last || dates >> (length + 1) != 0))
            }
        }
    }

    /// The first time it runs after `after` (milliseconds since the epoch) in `tz`. A local time
    /// a clock change repeats runs once, at its first; one it skips doesn't run.
    pub fn next(&self, tz: Tz, after: i64) -> Option<i64> {
        let today = chrono::DateTime::from_timestamp_millis(after)?
            .with_timezone(&tz)
            .date_naive();
        let offsets = self.offsets();
        // From yesterday, whose runs may carry past midnight, through two months and a week, so a
        // date only some months have, or a time a clock change skips, finds its next.
        (-1..=70)
            .map(|days| today + Duration::days(days))
            .filter(|day| self.runs_on(*day))
            .find_map(|day| {
                let start = day.and_time(NaiveTime::MIN);
                offsets
                    .iter()
                    .filter_map(|offset| {
                        tz.from_local_datetime(&(start + Duration::minutes(*offset)))
                            .earliest()
                            .map(|at| at.timestamp_millis())
                    })
                    .filter(|at| *at > after)
                    .min()
            })
    }
}

/// The next `count` times `rules` run after `after`, in `tz`, each once.
pub fn upcoming(rules: &[When], tz: Tz, after: i64, count: usize) -> Vec<i64> {
    let mut runs = Vec::with_capacity(count);
    let mut from = after;
    while runs.len() < count {
        let Some(at) = rules.iter().filter_map(|rule| rule.next(tz, from)).min() else {
            break;
        };
        runs.push(at);
        from = at;
    }
    runs
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
