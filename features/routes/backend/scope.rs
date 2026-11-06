//! Route-day availability, DSP station scope, request binding and retention dates.
use super::{RoutesStore, settings};
use chrono::{Duration, NaiveDate};
use dispatch_core::{
    Error, Result,
    db::{Store, s},
    ensure,
};
use dispatch_cortex::routes::{Collection, Mode, Request};
use serde_json::Value;

/// The day `mode` reads by default: yesterday for a final read, today for a snapshot.
pub fn default_day(mode: Mode, today: NaiveDate) -> String {
    match mode {
        Mode::Final => (today - Duration::days(1)).to_string(),
        Mode::Snapshot => today.to_string(),
    }
}
/// Whether `day` can be read in `mode` on `today`: a final read needs a completed day,
/// a snapshot reads today only.
pub fn day_allowed(mode: Mode, day: &str, today: NaiveDate) -> Result<()> {
    let date =
        NaiveDate::parse_from_str(day, "%Y-%m-%d").map_err(|_| Error::new("invalid_date", 400))?;
    ensure(date.to_string() == day, "invalid_date", 400)?;
    ensure(
        match mode {
            Mode::Final => date < today,
            Mode::Snapshot => date == today,
        },
        "routes_day_not_available",
        400,
    )
}

/// Today, where the DSP is.
pub(super) fn routes_today(store: &Store, id: &str) -> Result<NaiveDate> {
    routes_day(store, id, dispatch_core::db::now())
}
/// The day it is where the DSP is at `at`, in milliseconds since the epoch.
pub(super) fn routes_day(store: &Store, id: &str, at: i64) -> Result<NaiveDate> {
    let tz: chrono_tz::Tz = store
        .find_dsp(id)?
        .timezone
        .parse()
        .map_err(|_| Error::new("invalid_timezone", 400))?;
    Ok(dispatch_core::foundation::clock::day(tz, at))
}
/// The request for one day: the DSP's station and names, which the driver resolves
/// to a service area and provider on Cortex.
pub(crate) fn routes_request(store: &Store, id: &str, day: &str, mode: Mode) -> Result<Value> {
    Ok(serde_json::to_value(day_request(store, id, day, mode)?)?)
}
/// What a job reading `day` in `mode` asks Cortex for, which a capture of it must match.
pub(super) fn day_request(store: &Store, id: &str, day: &str, mode: Mode) -> Result<Request> {
    let profile = store.profile(id)?;
    let dsp = store.find_dsp(id)?;
    ensure(
        !profile.station_code.is_empty(),
        "routes_station_required",
        409,
    )?;
    // A scope proven by an earlier day at this station spares discovery, which
    // starts from an address Cortex may send elsewhere.
    let known = store.routes_db(id)?.one(
        "SELECT service_area_id,provider FROM route_publications WHERE station=? AND active=1 \
         ORDER BY collected_at DESC LIMIT 1",
        [&profile.station_code],
    )?;
    let request = Request {
        collection: Collection::Routes,
        mode,
        date: day.into(),
        station: profile.station_code,
        timezone: dsp.timezone,
        dsp_name: dsp.name,
        dsp_abbreviation: profile.abbreviation,
        service_area_id: known.as_ref().map(|k| s(k, "service_area_id").to_owned()),
        provider: known.as_ref().map(|k| s(k, "provider").to_owned()),
    };
    request.validate()?;
    Ok(request)
}
/// The first day the DSP's window keeps, or none when every day is kept.
pub(super) fn retention_start(store: &Store, id: &str) -> Result<Option<NaiveDate>> {
    let retention: settings::Retention =
        dispatch_core::settings::read(store, id, &settings::RETENTION)?;
    (retention.days)
        .map(|days| first_kept(store, id, days, dispatch_core::db::now()))
        .transpose()
}
/// The first day a window of `days` keeps at `at`, in milliseconds since the epoch: that many
/// days back from the day it is then where the DSP is, as expiry counts it as it runs.
pub(super) fn first_kept(store: &Store, id: &str, days: i64, at: i64) -> Result<NaiveDate> {
    Ok(routes_day(store, id, at)? - Duration::days(days))
}
