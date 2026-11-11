//! When a collection runs: its schedule's rules, each on chosen days of the week, at set times
//! or every so often between two times, where the DSP is. A rule is one row of
//! `collection_schedules`, which also keeps the single time and interval the previous release
//! reads it by.
use crate::{Error, Result, ensure, foundation::timing};
use chrono::{Datelike, NaiveDate, NaiveTime};
use chrono_tz::Tz;
use serde_json::Value;

pub use crate::foundation::timing::{EVERY, EVERY_DAY, Timing, text, time};

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

/// One rule of a collection's schedule: the days of the week it runs on, Monday the lowest
/// bit of `days`, and when on each.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    pub days: i64,
    pub timing: Timing,
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
        let timing = Timing::parse(value)?;
        Ok(Self { days: mask, timing })
    }

    fn runs_on(&self, day: NaiveDate) -> bool {
        self.days & (1 << day.weekday().num_days_from_monday()) != 0
    }

    /// The first time it runs after `after` (milliseconds since the epoch) in `tz`. A local
    /// time a clock change repeats runs once, at its first; one it skips doesn't run.
    pub fn next(&self, tz: Tz, after: i64) -> Option<i64> {
        // To a day past next week's, in case a clock change skips the one time it has.
        self.timing.next(tz, after, 15, |day| self.runs_on(day))
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
    timing::upcoming(after, count, |from| next(rules, tz, from))
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
