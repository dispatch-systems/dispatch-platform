//! When a collection runs: its schedule's rules, each on chosen days of the week, at set times
//! or every so often between two times, where the DSP is. A rule is one row of
//! `collection_schedules`, which also keeps the single time and interval the previous release
//! reads it by.
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
const NAMES: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const FULL: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

/// At what time of a day a rule runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Timing {
    /// At each of these, earliest first.
    At(Vec<NaiveTime>),
    /// Every `every` minutes from `from` until `to`, `to` included. A `to` before `from` runs
    /// on past midnight; the same as `from`, all day.
    Repeat {
        every: i64,
        from: NaiveTime,
        to: NaiveTime,
    },
}

/// One rule of a collection's schedule: the days of the week it runs on, Monday the lowest
/// bit of `days`, and when on each.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    pub days: i64,
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
fn minutes(at: NaiveTime) -> i64 {
    i64::from(at.hour() * 60 + at.minute())
}
pub fn text(at: NaiveTime) -> String {
    at.format("%H:%M").to_string()
}

impl Rule {
    /// A rule as a request states it: `days`, Monday 0 to Sunday 6, and either `times` or
    /// `every` minutes `from` `to`, as `kind` says.
    pub fn parse(value: &Value) -> Result<Self> {
        let days = value["days"]
            .as_array()
            .filter(|days| (1..=7).contains(&days.len()))
            .ok_or_else(|| Error::new("invalid_schedule_days", 400))?;
        let mut mask = 0;
        for day in days {
            let day = day
                .as_i64()
                .filter(|day| (0..7).contains(day))
                .ok_or_else(|| Error::new("invalid_schedule_days", 400))?;
            ensure(mask & (1 << day) == 0, "invalid_schedule_days", 400)?;
            mask |= 1 << day;
        }
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
        Ok(Self { days: mask, timing })
    }

    /// The minutes after the start of a day it runs on that it runs at, ascending: past a
    /// day's length, on the next day.
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
    fn runs_on(&self, day: NaiveDate) -> bool {
        self.days & (1 << day.weekday().num_days_from_monday()) != 0
    }

    /// The first time it runs after `after` (milliseconds since the epoch) in `tz`. A local
    /// time a clock change repeats runs once, at its first; one it skips doesn't run.
    pub fn next(&self, tz: Tz, after: i64) -> Option<i64> {
        let today = chrono::DateTime::from_timestamp_millis(after)?
            .with_timezone(&tz)
            .date_naive();
        let offsets = self.offsets();
        // From yesterday, whose runs may carry past midnight, to a day past next week's, in
        // case a clock change skips the one time it has.
        (-1..=15)
            .map(|days| today + Duration::days(days))
            .filter(|day| self.runs_on(*day))
            .flat_map(|day| {
                let start = day.and_time(NaiveTime::MIN);
                offsets.iter().filter_map(move |offset| {
                    tz.from_local_datetime(&(start + Duration::minutes(*offset)))
                        .earliest()
                        .map(|at| at.timestamp_millis())
                })
            })
            .filter(|at| *at > after)
            .min()
    }

    /// How it reads: "Weekdays at 6:00 AM and 2:00 PM", or "Every day, every 30 min, 6:00 AM
    /// to 9:00 PM".
    pub fn describe(&self) -> String {
        let days = days_text(self.days);
        match &self.timing {
            Timing::At(times) => {
                let times: Vec<String> = times.iter().map(|at| clock(*at)).collect();
                format!("{days} at {}", list(&times))
            }
            Timing::Repeat { every, from, to } => {
                let window = if from == to {
                    "all day".to_owned()
                } else {
                    format!("{} to {}", clock(*from), clock(*to))
                };
                format!("{days}, every {}, {window}", duration(*every))
            }
        }
    }
}

/// The first time any of `rules` runs after `after`, in `tz`.
pub fn next(rules: &[Rule], tz: Tz, after: i64) -> Option<i64> {
    rules.iter().filter_map(|rule| rule.next(tz, after)).min()
}
/// The next `count` times `rules` run after `after`, in `tz`, each once.
pub fn upcoming(rules: &[Rule], tz: Tz, after: i64, count: usize) -> Vec<i64> {
    let mut runs = Vec::with_capacity(count);
    let mut from = after;
    while runs.len() < count {
        let Some(at) = next(rules, tz, from) else {
            break;
        };
        runs.push(at);
        from = at;
    }
    runs
}

fn clock(at: NaiveTime) -> String {
    at.format("%-I:%M %p").to_string()
}
fn duration(minutes: i64) -> String {
    match (minutes / 60, minutes % 60) {
        (0, minutes) => format!("{minutes} min"),
        (hours, 0) => format!("{hours} hr"),
        (hours, minutes) => format!("{hours} hr {minutes} min"),
    }
}
fn list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}
/// "Every day", "Weekdays", "Weekends", "Mondays", "Mon–Thu", or "Mon, Wed, Fri".
pub fn days_text(days: i64) -> String {
    let on: Vec<usize> = (0..7).filter(|day| days & (1 << day) != 0).collect();
    match days {
        EVERY_DAY => return "Every day".into(),
        0b001_1111 => return "Weekdays".into(),
        0b110_0000 => return "Weekends".into(),
        _ => {}
    }
    if let [day] = on.as_slice() {
        return format!("{}s", FULL[*day]);
    }
    let running = on.windows(2).all(|pair| pair[1] == pair[0] + 1);
    if running && on.len() > 2 {
        return format!("{}–{}", NAMES[on[0]], NAMES[on[on.len() - 1]]);
    }
    let names: Vec<&str> = on.iter().map(|day| NAMES[*day]).collect();
    names.join(", ")
}

#[cfg(test)]
#[path = "../tests/backend/rules.rs"]
mod tests;
