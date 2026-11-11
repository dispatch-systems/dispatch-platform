//! At what times of its days a scheduled thing runs, where the DSP is: at set times, or every so
//! often between two times. Collections' rules and backups' each say which days they run on.
use crate::{Error, Result, ensure, foundation::validate as v};
use chrono::{Duration, NaiveDate, NaiveTime, TimeZone, Timelike};
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

impl Timing {
    /// A rule's timing as a request states it: `kind`, `times` or `repeat` with `every` minutes
    /// `from` `to`.
    pub fn parse(value: &Value) -> Result<Self> {
        Ok(match v::choice(value, "kind", &["times", "repeat"])? {
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
                Self::At(parsed)
            }
            _ => {
                let (least, most, step) = EVERY;
                let every = v::integer(value, "every", least, most)
                    .map_err(|_| Error::new("invalid_schedule_interval", 400))?;
                ensure(every % step == 0, "invalid_schedule_interval", 400)?;
                Self::Repeat {
                    every,
                    from: time(v::text(value, "from", 5, 5)?)?,
                    to: time(v::text(value, "to", 5, 5)?)?,
                }
            }
        })
    }

    /// The minutes after the start of a day it runs on that it runs at, ascending: past a day's
    /// length, on the next day.
    fn offsets(&self) -> Vec<i64> {
        match self {
            Self::At(times) => times.iter().map(|at| minutes(*at)).collect(),
            Self::Repeat { every, from, to } => {
                let start = minutes(*from);
                let length = (minutes(*to) - start).rem_euclid(DAY);
                // All day runs up to the next day's first, which that day runs.
                let last = if length == 0 { DAY - 1 } else { length };
                (0..=last / every).map(|at| start + at * every).collect()
            }
        }
    }

    /// The first time it runs after `after` (milliseconds since the epoch) in `tz`, on the days
    /// `runs_on` takes, from yesterday, whose runs may carry past midnight, to `ahead` days from
    /// today. A local time a clock change repeats runs once, at its first; one it skips doesn't
    /// run.
    pub fn next(
        &self,
        tz: Tz,
        after: i64,
        ahead: i64,
        runs_on: impl Fn(NaiveDate) -> bool,
    ) -> Option<i64> {
        let today = chrono::DateTime::from_timestamp_millis(after)?
            .with_timezone(&tz)
            .date_naive();
        let offsets = self.offsets();
        // A day's runs all come before the next day's first, so the first day with one after
        // `after` holds the next.
        (-1..=ahead)
            .map(|days| today + Duration::days(days))
            .filter(|day| runs_on(*day))
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

/// The next `count` runs after `after`, each once, `next` giving the first after an instant.
pub fn upcoming(after: i64, count: usize, next: impl Fn(i64) -> Option<i64>) -> Vec<i64> {
    let mut runs = Vec::with_capacity(count);
    let mut from = after;
    while runs.len() < count {
        let Some(at) = next(from) else {
            break;
        };
        runs.push(at);
        from = at;
    }
    runs
}
