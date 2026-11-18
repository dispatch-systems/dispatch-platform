//! Conversation attribution, reading and replies for texts kept by `phone`.
//! Work-phone conversations follow drivers across days; other numbers have their own.
pub(crate) mod contacts;
pub(crate) mod filing;
pub(crate) mod live_day;
pub(crate) mod model;
pub(crate) mod numbers;
pub(crate) mod picking;
pub(crate) mod reading;
pub(crate) mod reply;
pub(crate) mod storage;
pub(crate) mod todays_routes;
pub(crate) mod workdays;

use crate::{
    api::types::DispatchPhone,
    backend::phone::{
        model::Health,
        storage::{Device, DeviceStore},
    },
};
use dispatch_core::{
    Error, Result,
    db::{self, Store},
};
use jiff::ToSpan;
use jiff::civil::Date;
use jiff::tz::TimeZone;

/// The minutes in a day: how far past its midnight a day may begin, as one clocks skip whole.
const DAY_MINUTES: i64 = 24 * 60;

/// The DSP's phones of the line `line`, Dispatch Phone's own for none, and whether each is
/// online: none until one is paired.
pub fn phones(db: &Store, dsp: &str, line: Option<&str>) -> Result<Vec<DispatchPhone>> {
    let now = db::now();
    Ok(db
        .phones_devices(dsp, line)?
        .into_iter()
        .map(|device| dispatch_phone(device, now))
        .collect())
}
/// The DSP's phones of the line `line` an app holds, leaving out the demo's stand-in.
pub fn held_phones(db: &Store, dsp: &str, line: Option<&str>) -> Result<Vec<DispatchPhone>> {
    let now = db::now();
    Ok(db
        .phones_devices(dsp, line)?
        .into_iter()
        .filter(|device| !device.stand_in)
        .map(|device| dispatch_phone(device, now))
        .collect())
}
fn dispatch_phone(device: Device, now: i64) -> DispatchPhone {
    let online = device.online(now);
    DispatchPhone {
        id: device.id,
        name: device.name,
        online,
        last_seen_at: device.last_seen_at,
        cant_send: device.health.as_ref().is_some_and(Health::cant_send),
    }
}
/// The DSP's own timezone, which its days are in.
pub fn zone(db: &Store, dsp: &str) -> Result<TimeZone> {
    dispatch_core::foundation::clock::zone(&db.find_dsp(dsp)?.timezone)
}
/// `day`, YYYY-MM-DD: refused otherwise, as `invalid_date`.
pub fn date(day: &str) -> Result<Date> {
    dispatch_core::foundation::clock::date(day)
}
/// When `day` began in `zone`, in milliseconds: its first instant, at midnight, or where clocks
/// went forward over midnight, at the first minute they showed after; a day they skipped whole
/// began as the next did.
pub fn began(zone: &TimeZone, day: Date) -> Result<i64> {
    let midnight = day.at(0, 0, 0, 0);
    (0..=DAY_MINUTES)
        .find_map(|minute| {
            dispatch_core::foundation::clock::instant(zone, midnight + minute.minutes())
        })
        .ok_or_else(|| Error::new("invalid_date", 400))
}

#[cfg(test)]
#[path = "../../tests/backend/texts/lines.rs"]
mod lines;
