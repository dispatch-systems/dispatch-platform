//! Weekly Scorecard keeps the weeks Cortex's performance API answers with.
use super::{
    settings,
    storage::{DOMAIN, STORAGE, WeeklyScorecardStore},
};
use chrono::NaiveDate;
use dispatch_core::{
    Error, Result,
    collection::{browser::Collected, registry::AddedStorage},
    db::{Store, at, now, s},
    ensure,
    foundation::weeks::last_completed_week,
    manifest::{Keeper, Schedules, schedules},
    server::cache::DataDomain,
};
use dispatch_cortex::{
    discovery::Scope,
    weekly_scorecard::{Collection, JOB_KIND, Request},
};
use serde_json::{Value, json};

pub struct WeeklyScorecard;
impl Keeper for WeeklyScorecard {
    fn keeps(&self) -> &'static str {
        JOB_KIND
    }
    fn permission(&self) -> &'static str {
        "weekly_scorecard.collect"
    }
    fn schedules(&self) -> Schedules {
        schedules("weekly_scorecard.manage")
    }
    fn domain(&self) -> DataDomain {
        DOMAIN
    }
    fn bind(&self, store: &Store, dsp: &str, request: &Value) -> Result<Value> {
        let week = request["week"]
            .as_str()
            .ok_or_else(|| Error::new("invalid_input", 400))?;
        let station = request["station"]
            .as_str()
            .ok_or_else(|| Error::new("invalid_input", 400))?;
        let bound = bind_weekly_scorecard_request(store, dsp, week)?;
        ensure(
            bound["station"] == station,
            "weekly_scorecard_scope_mismatch",
            409,
        )?;
        Ok(bound)
    }
    /// Where the last publication of `station` read the API, with the company it named.
    fn kept(&self, store: &Store, dsp: &str, question: &Value) -> Result<Value> {
        let address = store.weekly_scorecard_address(dsp, s(question, "station"))?;
        Ok(serde_json::to_value(address)?)
    }
    fn publish(&self, store: &Store, dsp: &str, job: &str, collected: Collected) -> Result<()> {
        let Collected { data, scope } = collected;
        store.publish_weekly_scorecard(
            dsp,
            job,
            &serde_json::from_value(data)?,
            &Scope::collected(scope)?,
        )
    }
    fn schedule_ready(&self, store: &Store, dsp: &str) -> Result<()> {
        weekly_scorecard_schedule_ready(store, dsp)
    }
    fn scheduled(&self, store: &Store, dsp: &str, _: &str) -> Result<Vec<(String, Value)>> {
        store.weekly_scorecard_jobs(dsp)
    }
    fn storages(&self) -> &'static [&'static AddedStorage] {
        static STORAGES: [&AddedStorage; 1] = [&STORAGE];
        &STORAGES
    }
}

/// Refresh recent completed weeks, including posted weeks whose outcomes can change.
pub(super) fn jobs(store: &Store, id: &str) -> Result<Vec<(String, Value)>> {
    let policy: settings::Policy = dispatch_core::settings::read(store, id, &settings::COLLECTION)?;
    let latest = last_completed_week(weekly_scorecard_today(store, id)?);
    let (_, saturday) = dispatch_core::foundation::weeks::week_days(&latest)?;
    let station = weekly_scorecard_station(store, id)?;
    let data = store.weekly_scorecard_db(id)?;
    let cutoff = at(now() - i64::from(policy.refresh_hours) * 3_600_000);
    let mut jobs = Vec::new();
    for offset in 0..policy.lookback_weeks {
        let day = saturday - chrono::Duration::weeks(i64::from(offset));
        let iso = chrono::Datelike::iso_week(&day);
        let week = format!("{}-W{:02}", iso.year(), iso.week());
        if data.one("SELECT week FROM weekly_scorecard_weeks WHERE week=? AND station=? AND scope_verified=1 AND checked_at>=?",
            [&week,&station,&cutoff])?.is_none() {
            jobs.push((format!("weekly_scorecard:{week}"),weekly_scorecard_request(store,id,&week)?));
        }
    }
    Ok(jobs)
}

/// Where the DSP is.
pub(super) fn weekly_scorecard_timezone(store: &Store, id: &str) -> Result<chrono_tz::Tz> {
    store
        .find_dsp(id)?
        .timezone
        .parse()
        .map_err(|_| Error::new("invalid_timezone", 400))
}
/// Today, where the DSP is.
pub(super) fn weekly_scorecard_today(store: &Store, id: &str) -> Result<NaiveDate> {
    Ok(dispatch_core::foundation::clock::today(
        weekly_scorecard_timezone(store, id)?,
    ))
}
/// The station the DSP's profile names, which every scorecard request carries.
fn weekly_scorecard_station(store: &Store, id: &str) -> Result<String> {
    let station = store.profile(id)?.station_code;
    ensure(
        !station.is_empty(),
        "weekly_scorecard_station_required",
        409,
    )?;
    Ok(station)
}
pub(crate) fn bind_weekly_scorecard_request(store: &Store, id: &str, week: &str) -> Result<Value> {
    let dsp = store.find_dsp(id)?;
    let profile = store.profile(id)?;
    ensure(
        !profile.station_code.is_empty(),
        "weekly_scorecard_station_required",
        409,
    )?;
    let request = Request {
        collection: Collection::WeeklyScorecard,
        week: week.into(),
        station: profile.station_code,
        timezone: dsp.timezone,
        dsp_name: dsp.name,
        dsp_abbreviation: profile.abbreviation,
    };
    request.validate()?;
    Ok(serde_json::to_value(request)?)
}
pub(crate) fn weekly_scorecard_request(store: &Store, id: &str, week: &str) -> Result<Value> {
    let request = bind_weekly_scorecard_request(store, id, week)?;
    // Bind live tenant context immediately before collection and publication.
    Ok(json!({"collection":"weekly_scorecard","week":request["week"],"station":request["station"]}))
}
pub(crate) fn weekly_scorecard_schedule_ready(store: &Store, id: &str) -> Result<()> {
    weekly_scorecard_station(store, id).map(|_| ())
}
