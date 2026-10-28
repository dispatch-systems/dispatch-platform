//! A whole read of a day that another feature collected itself, saved into historical route
//! data as the standard collection saves its own: staged one itinerary at a time, then the
//! day's read if it was collected later than the one the day has.
use super::{work_phones::Stored, *};
use dispatch_core::{
    State,
    manifest::events::{self, PeopleChanged},
};
use dispatch_cortex::routes::{MAX_ITINERARIES, token};
use dispatch_routes_contract::{RouteDataChanged, WorkPhonesChanged};
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex, PoisonError},
};

/// One driver's hold on one of the company's work phones on a day: from when, and until when
/// once it ended, in milliseconds since the epoch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkPhone {
    pub transporter_id: String,
    pub phone: String,
    pub since: i64,
    pub until: Option<i64>,
}
/// The most work phone holds a day keeps: ten for every itinerary a day may list.
const MOST_PHONES: usize = 10 * MAX_ITINERARIES;

/// The saves staging now, by DSP and key. Their publications are no job's, so the sweep
/// would take them for abandoned; it keeps these. Gone with the process, so a save a
/// restart cut short is swept.
static SAVING: Mutex<BTreeSet<(String, String)>> = Mutex::new(BTreeSet::new());

/// The keys a DSP's saves are staging under.
pub(super) fn running(dsp: &str) -> Vec<String> {
    SAVING
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .filter(|(of, _)| of == dsp)
        .map(|(_, key)| key.clone())
        .collect()
}
/// A save's place among those staging, given up however it ends.
#[derive(Debug)]
pub(super) struct Saving((String, String));
impl Saving {
    pub(super) fn start(dsp: &str, key: &str) -> Result<Self> {
        let place = (dsp.to_owned(), key.to_owned());
        let mut saving = SAVING.lock().unwrap_or_else(PoisonError::into_inner);
        ensure(saving.insert(place.clone()), "routes_save_running", 409)?;
        Ok(Self(place))
    }
}
impl Drop for Saving {
    fn drop(&mut self) {
        SAVING
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.0);
    }
}

/// Saves `capture`, a whole read of its day another feature collected, into historical route
/// data under `key`, as the standard collection saves a job's: its two lists and every
/// itinerary they list, checked whole against what a job reading that day in its mode asks of
/// the DSP's station, in the scope its days were read in, before anything is written, then
/// staged one itinerary at a time. It becomes the day's read only if it was collected (its
/// `finished_at`) later than the one the day has, replacing it whole, down to the day's work
/// phone holds, which become `phones`, but for the phones the one it replaces stored for its
/// itineraries that it shows none for (`work_phones`); an earlier read changes nothing and is
/// swept. Once it's the
/// day's read it tells Driver Match of its drivers (`people.changed`), then every feature that
/// hears it of the day's holds (`routes.work_phones_changed`, naming the day) and of its route data
/// changed (`routes.data_changed`). Answers whether it is the day's read now.
///
/// `key` is the read's own, unique among the DSP's reads: a token of up to 128 characters,
/// such as the saving feature's name and the read's time. A key already the day's read
/// answers so without saving again, and one still saving is refused (`routes_save_running`).
/// It refuses phones that aren't each one driver's hold on one phone from a time, until none
/// or a time no earlier, at most 10,000 of them (`invalid_input`); a read in a mode its day
/// didn't allow when it was read, where the DSP is: a final read of a day not yet ended, or a
/// snapshot of another day than it was then (`routes_day_not_available`); a DSP without a station
/// (`routes_station_required`); a capture that isn't one of its station's day in that scope
/// as listed (`routes_scope_mismatch`, `routes_capture_invalid`, `routes_source_too_large`);
/// and saving while Routes is off for the DSP (`routes_off`).
pub async fn save_day(
    state: &Arc<State>,
    dsp: &str,
    key: &str,
    capture: Capture,
    phones: Vec<WorkPhone>,
) -> Result<bool> {
    save_keeping(state, dsp, key, capture, phones, Stored::default()).await
}
/// Saves a read as `save_day` does, each of its itineraries that shows no work phone keeping the
/// one `stored` had for it, none if none, where it stores the itinerary, else the one the day's
/// read it replaces stored: `stored` is live route data's, as its day is saved.
pub(super) async fn save_keeping(
    state: &Arc<State>,
    dsp: &str,
    key: &str,
    capture: Capture,
    phones: Vec<WorkPhone>,
    stored: Stored,
) -> Result<bool> {
    ensure(token(key, 128) && held(&phones), "invalid_input", 400)?;
    let _saving = Saving::start(dsp, key)?;
    let d = dsp.to_owned();
    let on = move |db: &Store| ensure(db.feature_enabled(&d, COLLECTION)?, "routes_off", 409);
    let saved_day = capture.scope.date.clone();
    // Like a job's work, a save has no one to try it again.
    State::patiently(async {
        let (d, k, day, check) = (
            dsp.to_owned(),
            key.to_owned(),
            saved_day.clone(),
            on.clone(),
        );
        let mode = capture.mode;
        let finished_at = capture.finished_at;
        let (request, saved) = state
            .read(move |db| {
                check(db)?;
                let saved = db.routes_db(&d)?.one(
                    "SELECT 1 FROM route_publications WHERE job_id=? AND active=1",
                    [&k],
                )?;
                // A day is read in the mode it allowed when it was read, as a job's is when queued.
                day_allowed(mode, &day, routes_day(db, &d, finished_at)?)?;
                Ok((day_request(db, &d, &day, mode)?, saved.is_some()))
            })
            .await?;
        if saved {
            return Ok(true);
        }
        // A job's capture has the scope its request resolves to: the one the DSP's days were
        // read in, once one was. A save's has it too, so it reads as the day's other reads.
        request
            .scope_request()
            .validate_scope(&capture.scope)
            .map_err(|_| Error::new("routes_scope_mismatch", 502))?;
        let staged = stage_capture(state, dsp, key, request, capture, on.clone()).await?;
        let (d, k) = (dsp.to_owned(), key.to_owned());
        let current = state
            .run_scoped(dsp, DOMAIN, move |db| {
                on(db)?;
                publish_day(&*db.routes_db(&d)?, &k, &staged, Some(&phones), stored)
            })
            .await?;
        if current {
            // Drivers the standard collection hasn't read are told apart at once.
            events::publish(state, dsp, PeopleChanged).await;
            let day = saved_day.clone();
            events::publish(state, dsp, WorkPhonesChanged { day }).await;
            events::publish(state, dsp, RouteDataChanged { day: saved_day }).await;
        }
        Ok(current)
    })
    .await
}

/// Whether each of `phones` is one driver's hold on one phone, from a time until none or a
/// time no earlier, and there are no more than a day keeps.
fn held(phones: &[WorkPhone]) -> bool {
    let mut seen = BTreeSet::new();
    phones.len() <= MOST_PHONES
        && phones.iter().all(|hold| {
            token(&hold.transporter_id, 64)
                && identifier(&hold.phone, 32)
                && hold.since > 0
                && hold.until.is_none_or(|until| until >= hold.since)
                && seen.insert((&hold.transporter_id, &hold.phone, hold.since))
        })
}
