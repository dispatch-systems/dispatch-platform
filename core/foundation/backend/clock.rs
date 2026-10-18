//! Today's date where a DSP is: the calendar its days are read on.
use chrono::{NaiveDate, Utc};
use chrono_tz::Tz;
use std::sync::RwLock;

/// The day a test server pins with `DISPATCH_FIXTURE_TODAY`, which `Config::load` sets: what
/// reads today has no config to hand, so the day holds for the whole process.
static PINNED: RwLock<Option<NaiveDate>> = RwLock::new(None);

/// Today in `zone`, or the day the server pins, so a test collects and seeds the same days
/// whatever the date.
pub fn today(zone: Tz) -> NaiveDate {
    let pinned = *PINNED.read().unwrap_or_else(|poisoned| poisoned.into_inner());
    pinned.unwrap_or_else(|| Utc::now().with_timezone(&zone).date_naive())
}

pub(crate) fn pin(day: Option<NaiveDate>) {
    *PINNED.write().unwrap_or_else(|poisoned| poisoned.into_inner()) = day;
}
