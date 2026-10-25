//! When a DSP's reading happens: its window's instants on each day, the grid of reads from the
//! window's start, the DSP's own place on it, and what its task does next.
use crate::manifest::Window;
use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, TimeZone};
use chrono_tz::Tz;
use sha2::{Digest, Sha256};

/// A DSP's reading as the server plans it: its reader's window there, where the DSP is, how
/// often it reads, and its own place after each slot of the grid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plan {
    pub window: Window,
    pub timezone: Tz,
    /// Milliseconds between reads.
    pub every: i64,
    /// Milliseconds after each slot of the grid that the DSP reads, and after its window ends
    /// that its day stops: its own, from its id and its reader's, so DSPs whose windows open
    /// and close together read and stop apart, and each keeps its place across restarts.
    pub offset: i64,
}

/// One day's window, in milliseconds since the epoch: open from `open` until `close`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Day {
    /// The day it opens on, where the DSP is.
    pub date: NaiveDate,
    pub open: i64,
    pub close: i64,
}

/// What a DSP's task does next.
#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    /// Reads for the day open now.
    Read(Day),
    /// Stops a day it started, whose window has ended.
    Stop(NaiveDate),
    /// Nothing until then, unless its plan changes.
    Wait(i64),
}

impl Plan {
    pub fn new(
        window: Window,
        timezone: Tz,
        every: std::time::Duration,
        reader: &str,
        dsp: &str,
    ) -> Self {
        let every = every.as_millis() as i64;
        let hash = Sha256::digest(format!("{reader}:{dsp}"));
        let seed = u64::from_be_bytes(hash[..8].try_into().expect("eight bytes"));
        Self {
            window,
            timezone,
            every,
            offset: (seed % (every as u64 / 1000)) as i64 * 1000,
        }
    }

    /// `date`'s window: from its start that day until its stop, the next day's when the stop is
    /// no later than the start.
    pub fn day(&self, date: NaiveDate) -> Day {
        let ends = if self.window.stop > self.window.start {
            date
        } else {
            date + Duration::days(1)
        };
        Day {
            date,
            open: self.instant(date.and_time(self.window.start)),
            close: self.instant(ends.and_time(self.window.stop)),
        }
    }

    /// The window open at `at`, if one is: today's, or yesterday's when it runs past midnight.
    pub fn open_at(&self, at: i64) -> Option<Day> {
        let today = self.date(at);
        [today - Duration::days(1), today]
            .into_iter()
            .map(|date| self.day(date))
            .find(|day| day.open <= at && at < day.close)
    }

    /// The first window to open after `at`.
    pub fn next(&self, at: i64) -> Day {
        let today = self.date(at);
        (0..3)
            .map(|days| self.day(today + Duration::days(days)))
            .find(|day| day.open > at)
            .expect("a window opens within two days")
    }

    /// The first of `day`'s slots after `after`: its first, `offset` past its open, then one
    /// every `every`. Some may be past the window's close.
    pub fn slot_after(&self, day: &Day, after: i64) -> i64 {
        let first = day.open + self.offset;
        if after < first {
            return first;
        }
        first + ((after - first) / self.every + 1) * self.every
    }

    /// What a DSP's slots go on from as its reading starts again at `now`, having read last at
    /// `last`: a read less than an interval old, as one that came late, keeps it to its next
    /// slot to come, so it reads at once only when its last read is over an interval old.
    pub fn resumed(&self, now: i64, last: Option<i64>) -> i64 {
        match last {
            Some(last) if now - last < self.every => last.max(now),
            Some(last) => last,
            None => i64::MIN,
        }
    }

    /// When `day` stops: `offset` past its window's close.
    pub fn stop(&self, day: &Day) -> i64 {
        day.close + self.offset
    }

    /// What a task does next at `now`, having read last at `after` (none: its first slot) and
    /// started the days `unstopped` without stopping them yet. A day other than the one open
    /// now stops once its stop is due, and the open day reads only once each earlier day it
    /// started has stopped. The open day reads at its first slot after `after`, at once when
    /// that has passed, while the slot is before its close.
    pub fn step(&self, now: i64, after: i64, unstopped: &[NaiveDate]) -> Step {
        let open = self.open_at(now);
        let mut until = match open {
            Some(day) => day.close,
            None => self.next(now).open,
        };
        for &date in unstopped {
            if open.is_some_and(|day| day.date == date) {
                continue;
            }
            let stop = self.stop(&self.day(date));
            if stop <= now {
                return Step::Stop(date);
            }
            if open.is_some_and(|day| date < day.date) {
                return Step::Wait(stop);
            }
            until = until.min(stop);
        }
        if let Some(day) = open {
            let due = self.slot_after(&day, after).max(now);
            if due < day.close {
                return if due <= now {
                    Step::Read(day)
                } else {
                    Step::Wait(due.min(until))
                };
            }
        }
        Step::Wait(until)
    }

    /// The day it is at `at` where the DSP is.
    fn date(&self, at: i64) -> NaiveDate {
        DateTime::from_timestamp_millis(at)
            .unwrap_or_default()
            .with_timezone(&self.timezone)
            .date_naive()
    }

    /// A local time where the DSP is, in milliseconds since the epoch: one a clock change
    /// repeats at its first, and one it skips as the clock resumes.
    fn instant(&self, local: NaiveDateTime) -> i64 {
        (0..=180)
            .find_map(|minutes| {
                self.timezone
                    .from_local_datetime(&(local + Duration::minutes(minutes)))
                    .earliest()
            })
            .map_or_else(
                || local.and_utc().timestamp_millis(),
                |at| at.timestamp_millis(),
            )
    }
}

#[cfg(test)]
#[path = "../../tests/backend/readers/plan.rs"]
mod tests;
