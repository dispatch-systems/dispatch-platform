//! The calendar and the clock where a DSP is: its timezone by name, today's date there, and the
//! instant a time its clocks show is.
use crate::{Error, Result};
use jiff::{
    Timestamp,
    civil::{Date, DateTime},
    tz::{AmbiguousOffset, TimeZone},
};
use std::sync::RwLock;

/// The day a test server pins with `DISPATCH_FIXTURE_TODAY`, which `Config::load` sets: what
/// reads today has no config to hand, so the day holds for the whole process, every test in it
/// included.
static PINNED: RwLock<Option<Date>> = RwLock::new(None);

/// The edition of the IANA database's rules built in, as `2026f`: what every timezone here is
/// read by.
pub fn rules() -> &'static str {
    jiff_tzdb::VERSION
}
/// The timezone the IANA database names `name`, as `America/Chicago`, written as it writes it:
/// refused for a name it doesn't hold or one in another case, as `invalid_timezone`.
pub fn zone(name: &str) -> Result<TimeZone> {
    TimeZone::get(name)
        .ok()
        .filter(|zone| zone.iana_name() == Some(name))
        .ok_or_else(|| Error::new("invalid_timezone", 400))
}
/// A date written `YYYY-MM-DD`, of a year from 1 to 9000: refused otherwise, as `invalid_date`.
/// The calendar ends with 9999, and counting past its end panics, so no date is taken so near
/// either end that the days, weeks or periods a request counts on from it could leave it.
pub fn date(text: &str) -> Result<Date> {
    Date::strptime("%Y-%m-%d", text)
        .ok()
        .filter(|date| (1..=9000).contains(&date.year()))
        .ok_or_else(|| Error::new("invalid_date", 400))
}
/// Today in `zone`, or the day the server pins, so a test collects and seeds the same days
/// whatever the date.
pub fn today(zone: &TimeZone) -> Date {
    day(zone, Timestamp::now().as_millisecond())
}
/// The day it is in `zone` at `at`, in milliseconds since the epoch as `db::now` counts them,
/// or the day the server pins, whatever the hour: what `today` is then, as for work that runs
/// later.
pub fn day(zone: &TimeZone, at: i64) -> Date {
    let pinned = *PINNED
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    pinned.unwrap_or_else(|| local(zone, at).date())
}
/// What the clocks in `zone` show at `at`, in milliseconds since the epoch: the epoch's, for an
/// instant no calendar reaches.
pub fn local(zone: &TimeZone, at: i64) -> DateTime {
    zone.to_datetime(Timestamp::from_millisecond(at).unwrap_or(Timestamp::UNIX_EPOCH))
}
/// The instant the clocks in `zone` show `local`, in milliseconds since the epoch: the first of
/// the two a clock change repeats, and none for a time one skips.
pub fn instant(zone: &TimeZone, local: DateTime) -> Option<i64> {
    let found = zone.to_ambiguous_timestamp(local);
    match found.offset() {
        AmbiguousOffset::Gap { .. } => None,
        _ => found.earlier().ok().map(Timestamp::as_millisecond),
    }
}
/// The instant clocks on UTC show `local`, in milliseconds since the epoch.
pub fn utc(local: DateTime) -> i64 {
    instant(&TimeZone::UTC, local).unwrap_or_default()
}
/// The one instant the clocks in `zone` show `local`, in milliseconds since the epoch: none for
/// a time a clock change repeats or skips.
pub fn only_instant(zone: &TimeZone, local: DateTime) -> Option<i64> {
    zone.to_ambiguous_timestamp(local)
        .unambiguous()
        .ok()
        .map(Timestamp::as_millisecond)
}

pub(crate) fn pin(day: Option<Date>) {
    *PINNED
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = day;
}
