//! When a DSP's reading happens: its window's instants on each day, the days it opens on, the
//! grid of reads from the window's start, and what its task does next.
use crate::manifest::Schedule;
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, TimeZone};
use chrono_tz::Tz;

/// A DSP's reading as the server plans it: its reader's schedule there, and where the DSP is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plan {
    pub schedule: Schedule,
    pub timezone: Tz,
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
    /// Milliseconds between reads.
    pub fn every(&self) -> i64 {
        self.schedule.every().as_millis() as i64
    }
    /// How long a read waits for its turn at most, and then runs at most: its interval, and
    /// `LONGEST_WAIT` at most, so a read that hangs holds its DSP and its turn no longer than
    /// that, however seldom it reads.
    pub fn timeout(&self) -> std::time::Duration {
        self.schedule.every().min(super::LONGEST_WAIT)
    }

    /// `date`'s window, whether or not it opens that day, as a day started stops at its own
    /// close: from its start that day until its stop, the next day's when the stop is no later
    /// than the start.
    pub fn day(&self, date: NaiveDate) -> Day {
        let (start, stop) = (self.schedule.start(), self.schedule.stop());
        let ends = if stop > start {
            date
        } else {
            date + Duration::days(1)
        };
        Day {
            date,
            open: self.instant(date.and_time(start)),
            close: self.instant(ends.and_time(stop)),
        }
    }

    /// The window open at `at`, if one is: today's, or yesterday's when it runs past midnight,
    /// on a day it opens on.
    pub fn open_at(&self, at: i64) -> Option<Day> {
        let today = self.date(at);
        [today - Duration::days(1), today]
            .into_iter()
            .filter(|date| self.opens_on(*date))
            .map(|date| self.day(date))
            .find(|day| day.open <= at && at < day.close)
    }

    /// The first window to open after `at`, on a day it opens on.
    pub fn next(&self, at: i64) -> Day {
        let today = self.date(at);
        (0..=7)
            .map(|days| today + Duration::days(days))
            .filter(|date| self.opens_on(*date))
            .map(|date| self.day(date))
            .find(|day| day.open > at)
            .expect("a window opens on a day of each week")
    }

    /// The first of `day`'s slots after `after`: its first at its open, then one every
    /// interval. Some may be past the window's close.
    pub fn slot_after(&self, day: &Day, after: i64) -> i64 {
        if after < day.open {
            return day.open;
        }
        day.open + ((after - day.open) / self.every() + 1) * self.every()
    }

    /// What a DSP's slots go on from as its reading starts again at `now`, having read last at
    /// `last`: a read less than an interval old, as one that came late, keeps it to its next
    /// slot to come, so it reads at once only when its last read is over an interval old.
    pub fn resumed(&self, now: i64, last: Option<i64>) -> i64 {
        match last {
            Some(last) if now - last < self.every() => last.max(now),
            Some(last) => last,
            None => i64::MIN,
        }
    }

    /// What a task does next at `now`, having read last at `after` (none: its first slot),
    /// started the days `unstopped` without stopping them yet, and ended the day `ended` last,
    /// its stop begun or done. A day other than the one open now stops once its window has
    /// closed, at once when its stop began already, and the open day reads only once each
    /// earlier day it started has stopped. The open day reads at its first slot after `after`,
    /// at once when that has passed, while the slot is before its close; never once it has
    /// ended, though its window was moved to end later: the next day's window is the next to
    /// read.
    pub fn step(
        &self,
        now: i64,
        after: i64,
        unstopped: &[NaiveDate],
        ended: Option<NaiveDate>,
    ) -> Step {
        let open = self.open_at(now).filter(|day| !ended_by(ended, day.date));
        let mut until = match open {
            Some(day) => day.close,
            None => self.next(now).open,
        };
        for &date in unstopped {
            if open.is_some_and(|day| day.date == date) {
                continue;
            }
            // A day's window closes before a later day's opens, so it stops before that reads.
            let stop = if ended_by(ended, date) {
                now
            } else {
                self.day(date).close
            };
            if stop <= now {
                return Step::Stop(date);
            }
            until = until.min(stop);
        }
        if let Some(day) = open
            && let Some(due) = self.due(&day, now, after)
        {
            return if due <= now {
                Step::Read(day)
            } else {
                Step::Wait(due.min(until))
            };
        }
        Step::Wait(until)
    }

    /// When it reads next at `now`, having read last at `after` and ended the day `ended` last,
    /// as `step` reads: the open day's first slot after `after`, now when that has passed, while
    /// it's before the day's close and the day hasn't ended; otherwise the start of the next day
    /// it opens on, as a day that ended never reads again.
    pub fn next_read(&self, now: i64, after: i64, ended: Option<NaiveDate>) -> i64 {
        let open = self.open_at(now).filter(|day| !ended_by(ended, day.date));
        if let Some(due) = open.and_then(|day| self.due(&day, now, after)) {
            return due;
        }
        let mut next = self.next(now);
        while ended_by(ended, next.date) {
            next = self.next(next.open);
        }
        next.open
    }

    /// When the open `day` reads next at `now`, having read last at `after`: its first slot
    /// after then, now when that has passed, while that's before its close.
    fn due(&self, day: &Day, now: i64, after: i64) -> Option<i64> {
        let due = self.slot_after(day, after).max(now);
        (due < day.close).then_some(due)
    }

    /// Whether its window opens on `date`, where the DSP is.
    fn opens_on(&self, date: NaiveDate) -> bool {
        self.schedule.opens_on(date.weekday())
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

/// Whether `date` has ended, `ended` being the last day that did, its stop begun or done.
fn ended_by(ended: Option<NaiveDate>, date: NaiveDate) -> bool {
    ended.is_some_and(|last| date <= last)
}

#[cfg(test)]
#[path = "../../tests/backend/readers/plan.rs"]
mod tests;
