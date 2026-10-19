//! Today's date where a DSP is: the calendar its days are read on.
use chrono::{DateTime, NaiveDate, Utc};
use chrono_tz::Tz;
use std::sync::RwLock;

/// The day a test server pins with `DISPATCH_FIXTURE_TODAY`, which `Config::load` sets: what
/// reads today has no config to hand, so the day holds for the whole process, every test in it
/// included.
static PINNED: RwLock<Option<NaiveDate>> = RwLock::new(None);

/// Today in `zone`, or the day the server pins, so a test collects and seeds the same days
/// whatever the date.
pub fn today(zone: Tz) -> NaiveDate {
    day(zone, Utc::now().timestamp_millis())
}
/// The day it is in `zone` at `at`, in milliseconds since the epoch as `db::now` counts them,
/// or the day the server pins, whatever the hour: what `today` is then, as for work that runs
/// later.
pub fn day(zone: Tz, at: i64) -> NaiveDate {
    let pinned = *PINNED
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    pinned.unwrap_or_else(|| {
        let at = DateTime::from_timestamp_millis(at).unwrap_or_default();
        at.with_timezone(&zone).date_naive()
    })
}

pub(crate) fn pin(day: Option<NaiveDate>) {
    *PINNED
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = day;
}
