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
use chrono::{Duration, NaiveDate, NaiveTime, TimeZone};
use chrono_tz::Tz;
use dispatch_core::{
    Error, Result,
    db::{self, Store},
};

/// The minutes in a day: how far past its midnight a day may begin, as one clocks skip whole.
const DAY_MINUTES: i64 = 24 * 60;

/// The DSP's dispatch phones, and whether each is online: none until one is paired.
pub fn phones(db: &Store, dsp: &str) -> Result<Vec<DispatchPhone>> {
    let now = db::now();
    Ok(db
        .phones_devices(dsp)?
        .into_iter()
        .map(|device| dispatch_phone(device, now))
        .collect())
}
/// The DSP's dispatch phones an app holds, leaving out the demo's stand-in.
pub fn held_phones(db: &Store, dsp: &str) -> Result<Vec<DispatchPhone>> {
    let now = db::now();
    Ok(db
        .phones_devices(dsp)?
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
pub fn zone(db: &Store, dsp: &str) -> Result<Tz> {
    db.find_dsp(dsp)?
        .timezone
        .parse()
        .map_err(|_| Error::new("invalid_timezone", 400))
}
/// `day`, YYYY-MM-DD: refused otherwise, as `invalid_date`.
pub fn date(day: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(day, "%Y-%m-%d").map_err(|_| Error::new("invalid_date", 400))
}
/// When `day` began in `zone`, in milliseconds: its first instant, at midnight, or where clocks
/// went forward over midnight, at the first minute they showed after; a day they skipped whole
/// began as the next did.
pub fn began(zone: Tz, day: NaiveDate) -> Result<i64> {
    let midnight = day.and_time(NaiveTime::MIN);
    (0..=DAY_MINUTES)
        .find_map(|minute| {
            zone.from_local_datetime(&(midnight + Duration::minutes(minute)))
                .earliest()
        })
        .map(|began| began.timestamp_millis())
        .ok_or_else(|| Error::new("invalid_date", 400))
}
