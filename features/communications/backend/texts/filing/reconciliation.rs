//! Route-change recovery and conversation invalidation. Days are noted before refiling so a
//! failure or restart leaves durable work to retry. `Heard` tracks live phones/routes already
//! announced to listeners; an unknown snapshot invalidates conservatively.
use super::super::{
    began, date,
    live_day::{self, LiveDay},
    model::Filed,
    storage::{DayStore, TextStore},
    zone,
};
use super::{failed, placement::Filer};
use crate::backend::{live, storage::DOMAIN};
use chrono::NaiveDate;
use dispatch_core::{
    Error, Result, State,
    db::{Store, now},
    foundation::clock,
};
use dispatch_routes_contract as routes;
use serde_json::json;
use std::{
    collections::{BTreeSet, HashMap},
    sync::{Arc, LazyLock, Mutex, MutexGuard},
};

/// Two days, in milliseconds: how far back the work conversations today's texts may wait in are
/// looked for, further than a day can be long.
const TWO_DAYS_MS: i64 = 2 * 24 * 60 * 60 * 1000;

/// The most days whose filing again failed one upkeep files again.
const MOST_REFILING: usize = 4;

/// Notes `day`, YYYY-MM-DD in the DSP's time, to be filed again by the upkeep: never one that
/// isn't a day.
pub(super) async fn note(state: &Arc<State>, dsp: &str, day: &str) -> Result<()> {
    let (at, day) = (dsp.to_owned(), date(day)?.to_string());
    state
        .run_scoped(dsp, DOMAIN, move |db| {
            db.note_communications_refiling(&at, &day, now())
        })
        .await
}

/// Files again the days noted to be, the earliest `MOST_REFILING`, each on its own, while Routes
/// is on for the DSP: one that fails again is logged, and stays noted for the next upkeep.
pub(super) async fn noted(state: &Arc<State>, dsp: &str) -> Result<()> {
    let asked = dsp.to_owned();
    let days = state
        .read(move |db| match routes::for_dsp(db, &asked)? {
            Some(_) => db.communications_refiling(&asked, MOST_REFILING),
            None => Ok(Vec::new()),
        })
        .await?;
    for day in days {
        if let Err(error) = refile_day(state, dsp, &day).await {
            let of = json!({"dsp": dsp, "day": day});
            failed("communications_texts_refile_failed", &error, of);
        }
    }
    Ok(())
}

/// Files every text of `day`, YYYY-MM-DD in the DSP's time, again, where it belongs now, letting go
/// of the day as noted to be but while Routes is off, which says nothing of it, then wakes the
/// pages waiting on what changed. Today's, keeping what changed of the live day
/// (`live_day::watched`), and noting changed the work conversations whose live day or work phone
/// changed (`work_changed`). A day Routes can't read keeps its texts where they are.
pub async fn refile_day(state: &Arc<State>, dsp: &str, day: &str) -> Result<()> {
    let day = date(day)?;
    let at = dsp.to_owned();
    let changed = state
        .run_serving(dsp, DOMAIN, move |db| {
            db.dsp(&at)?.transaction(|| {
                let mut filer = Filer::new(db, &at)?;
                let moved = !refiled_by(db, &at, &mut filer, day)?.is_empty();
                if filer.reads(&day.to_string()) {
                    db.refiled_communications_day(&at, &day.to_string())?;
                }
                if day != clock::today(filer.zone) {
                    return Ok(moved);
                }
                if let Some(watched) = live_day::watched(db, &at, now())? {
                    live_day::keep(db, &at, &watched)?;
                }
                Ok(work_changed(db, &at)? || moved)
            })
        })
        .await?;
    if changed {
        live::changed(state, dsp);
    }
    Ok(())
}
/// The live day as the pages waiting on changes last heard it, by DSP: kept in memory, so a server
/// started since heard nothing yet.
#[derive(Default)]
struct Heard {
    /// The day live route data held, YYYY-MM-DD.
    day: Option<String>,
    /// The work phone each driver had then, by the source naming them and their ID.
    phones: HashMap<(String, String), String>,
    /// The drivers on a route then, by the source naming them and their ID, as whether they work
    /// today shows their Work chip (`elsewhere`).
    routed: BTreeSet<(String, String)>,
}
impl Heard {
    /// The DSP's live day, as it is now.
    fn now(db: &Store, dsp: &str) -> Result<Self> {
        let Some(live) = LiveDay::read(db, dsp)? else {
            return Ok(Self::default());
        };
        let phones = live
            .phones(db, dsp)?
            .into_iter()
            .map(|(driver, phone)| ((live.source.clone(), driver), phone))
            .collect();
        let routed = live
            .reader
            .day(db, dsp)?
            .routes
            .into_iter()
            .map(|route| (live.source.clone(), route.driver_id))
            .collect();
        Ok(Self {
            day: Some(live.day),
            phones,
            routed,
        })
    }
}
fn heard() -> MutexGuard<'static, HashMap<String, Heard>> {
    static HEARD: LazyLock<Mutex<HashMap<String, Heard>>> = LazyLock::new(Default::default);
    HEARD.lock().unwrap_or_else(|poison| poison.into_inner())
}
/// Notes changed, for the pages waiting on changes, the work conversations today's route data
/// changing changed, since the pages last heard it: as the live day began or ended, those waiting
/// by their texts, as whether they wait counts only on the live day; each driver's whose work
/// phone changed (`LiveDay::phones`), one they had before as much as one they have now, so one open
/// hears it, or none shown any more; and each driver's put on a route or taken off one, as their
/// Work chip shows only while they work, so their other conversations read it again. Every driver's while what they had isn't known, as since the
/// server started. Inside the write that refiles today's texts: whether any was.
fn work_changed(db: &Store, dsp: &str) -> Result<bool> {
    let current = Heard::now(db, dsp)?;
    // Taken out until this is noted, so a failure leaves it unknown.
    let before = heard().remove(dsp);
    let waiting = match &before {
        Some(before) if before.day == current.day => false,
        _ => db.changed_communications_waiting(dsp, now() - TWO_DAYS_MS)?,
    };
    let drivers = match &before {
        Some(before) => {
            let changed: Vec<(String, String)> = before
                .phones
                .keys()
                .chain(current.phones.keys())
                .filter(|driver| before.phones.get(*driver) != current.phones.get(*driver))
                .chain(before.routed.symmetric_difference(&current.routed))
                .cloned()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            !changed.is_empty() && db.changed_communications_drivers(dsp, Some(&changed))?
        }
        None => db.changed_communications_drivers(dsp, None)?,
    };
    heard().insert(dsp.to_owned(), current);
    Ok(waiting || drivers)
}
/// Files every text of today, the DSP's day, again, where it belongs now, as its route data may have
/// changed unheard: while Communications was off, or as a hearing failed. Written only once one
/// belongs elsewhere, and wakes the pages waiting on its conversations only once one moved; one
/// that fails is logged with the day.
pub async fn reconcile(state: &Arc<State>, dsp: &str) -> Result<()> {
    let asked = dsp.to_owned();
    if state.read(move |db| misfiled(db, &asked)).await?.is_empty() {
        return Ok(());
    }
    let at = dsp.to_owned();
    let today = state
        .read(move |db| Ok(clock::today(zone(db, &at)?).to_string()))
        .await?;
    let at = dsp.to_owned();
    match state
        .run_serving(dsp, DOMAIN, move |db| refiled(db, &at))
        .await
    {
        Ok(true) => live::changed(state, dsp),
        Ok(false) => {}
        Err(error) => {
            let of = json!({"dsp": dsp, "day": today});
            failed("communications_texts_refile_failed", &error, of);
        }
    }
    Ok(())
}
/// Files every text of today, the DSP's day, again, where it belongs now: whether any moved.
pub fn refiled(db: &Store, dsp: &str) -> Result<bool> {
    let moves = misfiled(db, dsp)?;
    Ok(!db.refile_communications_texts(dsp, &moves)?.is_empty())
}
/// Each text of today, the DSP's day, filed elsewhere than it belongs now, with where it does.
fn misfiled(db: &Store, dsp: &str) -> Result<Vec<(String, Filed)>> {
    let mut filer = Filer::new(db, dsp)?;
    let today = clock::today(filer.zone);
    elsewhere(db, dsp, &mut filer, today)
}
/// Files every text of `day` again where `filer` says it belongs now: the conversations changed.
fn refiled_by(
    db: &Store,
    dsp: &str,
    filer: &mut Filer,
    day: NaiveDate,
) -> Result<BTreeSet<String>> {
    let moves = elsewhere(db, dsp, filer, day)?;
    db.refile_communications_texts(dsp, &moves)
}
/// Each text of `day`, from when it began until the next day did, filed elsewhere than `filer`
/// says it belongs now, with where it does. One whose place can't be told yet stays where it is.
fn elsewhere(
    db: &Store,
    dsp: &str,
    filer: &mut Filer,
    day: NaiveDate,
) -> Result<Vec<(String, Filed)>> {
    let next = day
        .succ_opt()
        .ok_or_else(|| Error::new("invalid_date", 400))?;
    let (from, to) = (began(filer.zone, day)?, began(filer.zone, next)?);
    let mut conversations: HashMap<Filed, Option<String>> = HashMap::new();
    let mut moves = Vec::new();
    for text in db.communications_texts_between(dsp, from, to)? {
        let Some(filed) = filer.filed(&text.number, text.at)? else {
            continue;
        };
        if !conversations.contains_key(&filed) {
            let found = db.communications_filed(dsp, &filed)?;
            conversations.insert(filed.clone(), found);
        }
        if conversations[&filed] != text.conversation {
            moves.push((text.id, filed));
        }
    }
    Ok(moves)
}
