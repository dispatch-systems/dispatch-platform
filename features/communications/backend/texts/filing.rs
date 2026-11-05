//! Filing the texts the DSP's dispatch phones relay, as the phone side keeps them (`phone`): each
//! in its conversation, on the text's own row, in the same write and transaction as the phone side
//! keeps or changes it (`changing`), so a text and its conversation never disagree; the pages
//! waiting on its conversation hear once both are committed. A text that can't be placed yet, as
//! reading where it belongs failed, stays kept, waiting to be filed, and every minute's upkeep
//! places those waiting (`unfiled`); one still waiting ten minutes after it first couldn't be, it
//! places under its number, as a day Routes can't read does (`overdue`). It files again the days
//! whose filing again failed, the earliest first (`noted`), and today's, as its route data may have
//! changed unheard (`reconcile`); keeps when the live day ends, so work conversations wait the two
//! hours after, and the pages hear them stop (`watch_live_day`); and fails the answers still
//! waiting on an offline phone at their expiry (`expire`).
//!
//! A work phone passes from driver to driver, so a text with one is filed by the day it was
//! written, where the DSP is, under the driver whose latest work phone that day it is, by their ID
//! of the source naming the drivers, as Routes reads the day (`day_phones`): live route data's while
//! it holds that day, else historical route data's. The whole day's texts with a number go to that
//! driver, both ways, so a phone handed to another driver takes the day's conversation with it.
//! Where the day names no one driver, as the number is nobody's latest phone that day or two
//! drivers', the driver someone at the DSP picked for it that day (`picking`); else the work phone's
//! own, a number nobody had that day but ever had as one (`ever_work_phone`) too; and any other
//! number's own. A day Routes can't read, as one not collected, gone, emptied unsaved, or not read
//! since Routes kept its phones, or while Routes is off, says nothing: a text that comes then is
//! placed under its number, the work phone's own or the number's, until it can be filed, and one
//! filed already stays where it is. A text is placed as it first comes; how it went, or its files
//! coming, never moves it. A day's route data changes as it goes on, and may reach historical route
//! data after its texts, as a text a phone relayed late; so a day's texts are filed again each time
//! Routes says its route data changed (`RouteDataChanged`): today's, the pages waiting on changes
//! hearing of the work conversations waiting as the live day, which they wait only on and the two
//! hours after it, begins or ends, and of each driver's whose work phone changed (`work_changed`).
//! Each day heard of is noted first, and let go once filed again while Routes can say anything of
//! it, so one that fails, or that a restart cuts short, is filed again by the upkeep until it's
//! done.
use super::{
    began, date,
    live_day::{self, LiveDay},
    model::{Filed, Unfiled},
    numbers::ten_digits,
    storage::{DayStore, PickStore, TextStore},
    zone,
};
use crate::backend::{
    live,
    phone::storage::{MOST_TEXTS, TextStore as _},
    sender,
    storage::DOMAIN,
};
use chrono::NaiveDate;
use chrono_tz::Tz;
use dispatch_core::{
    Error, Result, State,
    db::{Store, now},
    foundation::{clock, observability},
    manifest::{Maintenance, Upkeep},
};
use dispatch_routes_contract::{
    self as routes, DayPhones, DriverPhone, DriverRoutes, LiveRouteData, RouteDataChanged,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeSet, HashMap},
    sync::{Arc, LazyLock, Mutex, MutexGuard},
    time::Duration,
};

/// Two days, in milliseconds: how far back the work conversations today's texts may wait in are
/// looked for, further than a day can be long.
const TWO_DAYS_MS: i64 = 2 * 24 * 60 * 60 * 1000;
/// Ten minutes, in milliseconds: how long a text waits to be filed, once placing it first failed,
/// before it's placed under its number (`overdue`).
pub const UNFILED_MS: i64 = 10 * 60 * 1000;
/// The most days whose filing again failed one upkeep files again.
const MOST_REFILING: usize = 4;

/// Every minute, each DSP's texts waiting to be filed, those waiting too long under their number,
/// the days whose filing again failed and today's filed again where they belong now; the live
/// day's end kept; and its answers still waiting on an offline phone failed once they expire.
pub const MAINTENANCE: Maintenance = Maintenance {
    every: Duration::from_secs(60),
    run: pass,
};

/// Makes the phone side's change `change` to the DSP's texts, and files the texts it kept or
/// changed, in one write and one transaction, so neither is kept without the other: a text that
/// can't be placed yet is kept waiting to be filed. Once both are committed, wakes the pages waiting
/// on a conversation that changed. `change` answers with what the caller is answered with, and the
/// IDs of the texts it kept or changed.
pub async fn changing<T: Send + 'static>(
    state: &Arc<State>,
    dsp: &str,
    change: impl FnOnce(&Store) -> Result<(T, Vec<String>)> + Send + 'static,
) -> Result<T> {
    let at = dsp.to_owned();
    let (answer, changed) = state
        .run_scoped(dsp, DOMAIN, move |db| {
            db.dsp(&at)?.transaction(|| {
                let (answer, texts) = change(db)?;
                Ok((answer, file(db, &at, &texts)?))
            })
        })
        .await?;
    if changed {
        live::changed(state, dsp);
    }
    Ok(answer)
}
/// Routes says the DSP's route data of a day changed: notes the day to be filed again, then files
/// its texts again, which lets go of it, so one that fails, or that a restart cuts short, is filed
/// again by the upkeep.
pub fn refile(state: Arc<State>, dsp: String, changed: RouteDataChanged) -> Upkeep {
    Box::pin(async move {
        let of = || json!({"dsp": dsp, "day": changed.day});
        if let Err(error) = note(&state, &dsp, &changed.day).await {
            failed("communications_texts_refile_failed", &error, of());
        }
        if let Err(error) = refile_day(&state, &dsp, &changed.day).await {
            failed("communications_texts_refile_failed", &error, of());
        }
    })
}
/// Notes `day`, YYYY-MM-DD in the DSP's time, to be filed again by the upkeep: never one that
/// isn't a day.
async fn note(state: &Arc<State>, dsp: &str, day: &str) -> Result<()> {
    let (at, day) = (dsp.to_owned(), date(day)?.to_string());
    state
        .run_scoped(dsp, DOMAIN, move |db| {
            db.note_communications_refiling(&at, &day, now())
        })
        .await
}

fn pass(state: Arc<State>, _: bool) -> Upkeep {
    Box::pin(async move {
        let on = state
            .read(|db| {
                let mut found = Vec::new();
                for dsp in db.kept_dsps()? {
                    if db.feature_enabled(&dsp, crate::FEATURE.switch.id)? {
                        found.push(dsp);
                    }
                }
                Ok(found)
            })
            .await;
        match on {
            Ok(dsps) => {
                for dsp in dsps {
                    let of = || json!({"dsp": dsp});
                    if let Err(error) = unfiled(&state, &dsp).await {
                        failed("communications_texts_failed", &error, of());
                    }
                    if let Err(error) = overdue(&state, &dsp, now()).await {
                        failed("communications_texts_failed", &error, of());
                    }
                    if let Err(error) = noted(&state, &dsp).await {
                        failed("communications_texts_refile_failed", &error, of());
                    }
                    if let Err(error) = reconcile(&state, &dsp).await {
                        failed("communications_texts_refile_failed", &error, of());
                    }
                    if let Err(error) = watch_live_day(&state, &dsp, now()).await {
                        failed("communications_texts_live_day_failed", &error, of());
                    }
                    if let Err(error) = expire(&state, &dsp, now()).await {
                        failed("communications_texts_expire_failed", &error, of());
                    }
                }
            }
            Err(error) => failed("communications_texts_failed", &error, json!({})),
        }
    })
}

/// Logs `event`, with `error`'s code beside what it was of, `of`: the DSP, and the text or day.
fn failed(event: &'static str, error: &Error, mut of: Value) {
    of["error"] = json!(error.code);
    observability::event("error", event, of);
}

/// Files the DSP's texts waiting to be, oldest first, a page in each write, then wakes the pages
/// waiting on what changed: one that can't be placed yet waits for the next pass, after the rest.
/// Written only once one waits.
pub async fn unfiled(state: &Arc<State>, dsp: &str) -> Result<()> {
    let asked = dsp.to_owned();
    if state
        .read(move |db| db.unfiled_communications_texts(&asked, None, 1))
        .await?
        .is_empty()
    {
        return Ok(());
    }
    let mut after: Option<(i64, String)> = None;
    loop {
        let (at, from) = (dsp.to_owned(), after.take());
        let (last, more) = changing(state, dsp, move |db| {
            let from = from.as_ref().map(|(at, id)| (*at, id.as_str()));
            let page = db.unfiled_communications_texts(&at, from, MOST_TEXTS)?;
            let last = page.last().cloned();
            let ids = page.into_iter().map(|(_, id)| id).collect::<Vec<_>>();
            Ok(((last, ids.len() == MOST_TEXTS), ids))
        })
        .await?;
        if !more {
            return Ok(());
        }
        after = last;
    }
}

/// Places the DSP's texts that couldn't be placed for `UNFILED_MS` by `at` under their number,
/// as a day Routes can't read does (`unread`), the oldest page of them, noting each one's day to
/// be filed again, which moves it where it belongs once Routes can say: written only once one has
/// waited so long.
pub async fn overdue(state: &Arc<State>, dsp: &str, at: i64) -> Result<()> {
    let (asked, since) = (dsp.to_owned(), at - UNFILED_MS);
    if state
        .read(move |db| db.overdue_communications_texts(&asked, since, 1))
        .await?
        .is_empty()
    {
        return Ok(());
    }
    let at_dsp = dsp.to_owned();
    changing(state, dsp, move |db| {
        let texts = db.overdue_communications_texts(&at_dsp, since, MOST_TEXTS)?;
        // The day each is of, to be filed again, while the DSP's timezone can be read.
        let zone = zone(db, &at_dsp).ok();
        let mut senders = HashMap::new();
        for text in &texts {
            let sender = sender_name(db, &mut senders, text.sent_by.as_deref())?;
            let filed = unread(db, &at_dsp, &text.number);
            db.place_communications_text(&at_dsp, &text.id, &filed, sender.as_deref())?;
            if let Some(zone) = zone {
                let day = clock::day(zone, text.at).to_string();
                db.note_communications_refiling(&at_dsp, &day, at)?;
            }
        }
        Ok(((), texts.into_iter().map(|text| text.id).collect()))
    })
    .await
}
/// Files again the days noted to be, the earliest `MOST_REFILING`, each on its own, while Routes
/// is on for the DSP: one that fails again is logged, and stays noted for the next upkeep.
async fn noted(state: &Arc<State>, dsp: &str) -> Result<()> {
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
/// Keeps what changed of the DSP's live day at `at` (`live_day::watched`), as it began or ended;
/// once the two hours after it ended pass, wakes the pages waiting on the work conversations that
/// don't wait any more. Written only once something changed.
pub async fn watch_live_day(state: &Arc<State>, dsp: &str, at: i64) -> Result<()> {
    let asked = dsp.to_owned();
    if state
        .read(move |db| live_day::watched(db, &asked, at))
        .await?
        .is_none()
    {
        return Ok(());
    }
    let at_dsp = dsp.to_owned();
    let over = state
        .run_scoped(dsp, DOMAIN, move |db| {
            let Some(watched) = live_day::watched(db, &at_dsp, at)? else {
                return Ok(false);
            };
            live_day::keep(db, &at_dsp, &watched)?;
            Ok(matches!(watched, live_day::Watched::Over { .. }))
        })
        .await?;
    if over {
        live::changed(state, dsp);
    }
    Ok(())
}

/// Fails the answers waiting past their expiry at `at` on a phone offline then, as
/// `phones_offline`, each filed as it fails, so its conversation waits again: written only once
/// one has expired.
pub async fn expire(state: &Arc<State>, dsp: &str, at: i64) -> Result<()> {
    let asked = dsp.to_owned();
    if state
        .read(move |db| db.phones_expired(&asked, at))
        .await?
        .is_empty()
    {
        return Ok(());
    }
    let at_dsp = dsp.to_owned();
    changing(state, dsp, move |db| {
        Ok(((), db.phones_expire(&at_dsp, at)?))
    })
    .await
}

/// Files the texts `ids`, which the phone side kept or changed in this write: places each waiting
/// to be, then keeps whether the conversation each is filed in waits. Whether a conversation
/// changed.
fn file(db: &Store, dsp: &str, ids: &[String]) -> Result<bool> {
    if ids.is_empty() {
        return Ok(false);
    }
    let waiting = db.unfiled_communications_texts_of(dsp, ids)?;
    if !waiting.is_empty() {
        place(db, dsp, &waiting)?;
    }
    db.settle_communications_texts(dsp, ids)
}
/// Places each of `texts`, waiting to be filed, where it belongs, under the name its sender, if
/// someone at the DSP, is shown by as it's placed: each on its own, so one that can't be yet, as
/// reading what says where failed, is logged, kept as unplaced since now (`unplaced`), and waits
/// for the next upkeep, the rest placed.
fn place(db: &Store, dsp: &str, texts: &[Unfiled]) -> Result<()> {
    let mut filer = match Filer::new(db, dsp) {
        Ok(filer) => filer,
        Err(error) => return unplaced(db, dsp, texts, &error),
    };
    let mut senders = HashMap::new();
    for text in texts {
        let placed = filer.placed(&text.number, text.at).and_then(|filed| {
            let sender = sender_name(db, &mut senders, text.sent_by.as_deref())?;
            db.place_communications_text(dsp, &text.id, &filed, sender.as_deref())
        });
        if let Err(error) = placed {
            unplaced(db, dsp, std::slice::from_ref(text), &error)?;
        }
    }
    Ok(())
}
/// Logs that `texts` couldn't be placed, as `error` says, and keeps that they couldn't since now,
/// but those that couldn't before, which keep since when.
fn unplaced(db: &Store, dsp: &str, texts: &[Unfiled], error: &Error) -> Result<()> {
    for text in texts {
        let of = json!({"dsp": dsp, "text": text.id});
        failed("communications_texts_failed", error, of);
    }
    let ids: Vec<String> = texts.iter().map(|text| text.id.clone()).collect();
    db.unplaced_communications_texts(dsp, &ids, now())
}
/// Where a text with `number` goes while where it belongs can't be read: under the work phone's own
/// conversation while Routes says it ever was one, else the number's, as when Routes can't say.
fn unread(db: &Store, dsp: &str, number: &str) -> Filed {
    let ever = |ten: String| match routes::for_dsp(db, dsp)? {
        Some(routes) => routes.ever_work_phone(db, dsp, &ten),
        None => Ok(false),
    };
    match ten_digits(number).map(ever) {
        Some(Ok(true)) => Filed::WorkPhone(number.to_owned()),
        _ => Filed::Number(number.to_owned()),
    }
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
        .run_scoped(dsp, DOMAIN, move |db| {
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
        Ok(Self {
            day: Some(live.day),
            phones,
        })
    }
}
fn heard() -> MutexGuard<'static, HashMap<String, Heard>> {
    static HEARD: LazyLock<Mutex<HashMap<String, Heard>>> = LazyLock::new(Default::default);
    HEARD.lock().unwrap_or_else(|poison| poison.into_inner())
}
/// Notes changed, for the pages waiting on changes, the work conversations today's route data
/// changing changed, since the pages last heard it: as the live day began or ended, those waiting
/// by their texts, as whether they wait counts only on the live day; and each driver's whose work
/// phone changed (`LiveDay::phones`), one they had before as much as one they have now, so one open
/// hears it, or none shown any more. Every driver's while what they had isn't known, as since the
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
        .run_scoped(dsp, DOMAIN, move |db| refiled(db, &at))
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

/// Where a text with `number` written now belongs, as filing would place it: what someone at the
/// DSP opens to text the number first, so their text and its answers go there.
pub fn belongs(db: &Store, dsp: &str, number: &str) -> Result<Filed> {
    Filer::new(db, dsp)?.placed(number, now())
}

/// The name the one at the DSP who sent a text, `by`, is shown by, by their account: their full
/// name as they sent it, as an answer from the page goes under. Each read once, into `senders`.
fn sender_name(
    db: &Store,
    senders: &mut HashMap<String, Option<String>>,
    by: Option<&str>,
) -> Result<Option<String>> {
    let Some(by) = by else {
        return Ok(None);
    };
    if !senders.contains_key(by) {
        let name = db.platform_user_name(by)?.map(|name| sender::full(&name));
        senders.insert(by.to_owned(), name);
    }
    Ok(senders[by].clone())
}

/// Where the DSP's texts are filed, by each driver's latest work phone on the days they were
/// written, each day's read once.
struct Filer<'a> {
    db: &'a Store,
    dsp: &'a str,
    zone: Tz,
    live: Option<&'static dyn LiveRouteData>,
    live_day: Option<String>,
    routes: Option<&'static dyn DriverRoutes>,
    /// Each day's work phones, or none for a day Routes can't read.
    days: HashMap<String, Option<Phones>>,
    /// Each day's picks: the driver picked for each work phone, by its ten digits.
    picked: HashMap<String, HashMap<String, Filed>>,
    /// Whether each number, by its ten digits, was ever a work phone.
    work_phones: HashMap<String, bool>,
}
/// A day's work phones, as Routes reads them: the driver each is the latest phone of, by its ten
/// digits, with the source naming them; none for one that's two drivers' latest, and no one's.
struct Phones {
    source: String,
    drivers: HashMap<String, Option<String>>,
}
impl Phones {
    fn new(source: &str, phones: Vec<DriverPhone>) -> Self {
        let drivers = phones
            .into_iter()
            .map(|each| (each.phone, (!each.shared).then_some(each.driver_id)))
            .collect();
        Self {
            source: source.to_owned(),
            drivers,
        }
    }
}
impl<'a> Filer<'a> {
    fn new(db: &'a Store, dsp: &'a str) -> Result<Self> {
        let zone = zone(db, dsp)?;
        let live = routes::live_for_dsp(db, dsp)?;
        let live_day = match live {
            Some(live) => live.live_day(db, dsp)?,
            None => None,
        };
        Ok(Self {
            db,
            dsp,
            zone,
            live,
            live_day,
            routes: routes::for_dsp(db, dsp)?,
            days: HashMap::new(),
            picked: HashMap::new(),
            work_phones: HashMap::new(),
        })
    }
    /// Where a text with `number`, written at `at`, is filed: under the driver whose latest work
    /// phone it is that day, else the driver picked for it that day; else under the work phone, or
    /// the number, as itself. None while what would tell can't be read: that day's work phones, or
    /// whether the number was ever one, as Routes reads them. Routes being off isn't the number not
    /// being a work phone.
    fn filed(&mut self, number: &str, at: i64) -> Result<Option<Filed>> {
        let Some(ten) = ten_digits(number) else {
            return Ok(Some(Filed::Number(number.to_owned())));
        };
        let day = clock::day(self.zone, at).to_string();
        let Some(phones) = self.phones(&day)? else {
            return Ok(None);
        };
        let listed = match phones.drivers.get(&ten) {
            Some(Some(driver)) => {
                return Ok(Some(Filed::Driver {
                    source: phones.source.clone(),
                    id: driver.clone(),
                }));
            }
            Some(None) => true,
            None => false,
        };
        if let Some(picked) = self.picked(&day)?.get(&ten) {
            return Ok(Some(picked.clone()));
        }
        if listed {
            return Ok(Some(Filed::WorkPhone(number.to_owned())));
        }
        self.own(number, &ten)
    }
    /// Where a text new to Communications, with `number`, written at `at`, is placed: where it's
    /// filed, else, while that can't be told, under its number, the work phone's own while Routes
    /// says it ever was one, to be filed again once it can.
    fn placed(&mut self, number: &str, at: i64) -> Result<Filed> {
        if let Some(filed) = self.filed(number, at)? {
            return Ok(filed);
        }
        let own = match ten_digits(number) {
            Some(ten) => self.own(number, &ten)?,
            None => None,
        };
        Ok(own.unwrap_or_else(|| Filed::Number(number.to_owned())))
    }
    /// The conversation of `number`, with the ten digits `ten`, as itself: the work phone's while
    /// Routes says it ever was one, the number's while it says not; none while Routes is off.
    fn own(&mut self, number: &str, ten: &str) -> Result<Option<Filed>> {
        Ok(self.ever_work_phone(ten)?.map(|ever| match ever {
            true => Filed::WorkPhone(number.to_owned()),
            false => Filed::Number(number.to_owned()),
        }))
    }
    /// Whether Routes can say anything of `day`: live route data holds it, or historical route data
    /// is on for the DSP.
    fn reads(&self, day: &str) -> bool {
        self.routes.is_some() || (self.live.is_some() && self.live_day.as_deref() == Some(day))
    }
    /// Each driver's latest work phone on `day`: live route data's while it holds that day, else
    /// historical route data's; none while neither is on for the DSP, or it can't read the day.
    fn phones(&mut self, day: &str) -> Result<Option<&Phones>> {
        if !self.days.contains_key(day) {
            let read = match (self.live, self.routes) {
                (Some(live), _) if self.live_day.as_deref() == Some(day) => {
                    Some((live.source(), live.day_phones(self.db, self.dsp)?))
                }
                (_, Some(routes)) => {
                    Some((routes.source(), routes.day_phones(self.db, self.dsp, day)?))
                }
                _ => None,
            };
            let phones = match read {
                Some((source, DayPhones::Read(phones))) => {
                    Some(Phones::new(source.as_str(), phones))
                }
                Some((_, DayPhones::Uncollected | DayPhones::Unextracted)) | None => None,
            };
            self.days.insert(day.to_owned(), phones);
        }
        Ok(self.days[day].as_ref())
    }
    /// The drivers picked for work phones on `day`, by their ten digits.
    fn picked(&mut self, day: &str) -> Result<&HashMap<String, Filed>> {
        if !self.picked.contains_key(day) {
            let picked = self
                .db
                .communications_picked(self.dsp, day)?
                .into_iter()
                .collect();
            self.picked.insert(day.to_owned(), picked);
        }
        Ok(&self.picked[day])
    }
    /// Whether the number with the ten digits `ten` was ever a work phone, as Routes knows it:
    /// none while it's off for the DSP.
    fn ever_work_phone(&mut self, ten: &str) -> Result<Option<bool>> {
        let Some(routes) = self.routes else {
            return Ok(None);
        };
        if let Some(&ever) = self.work_phones.get(ten) {
            return Ok(Some(ever));
        }
        let ever = routes.ever_work_phone(self.db, self.dsp, ten)?;
        self.work_phones.insert(ten.to_owned(), ever);
        Ok(Some(ever))
    }
}

#[cfg(test)]
#[path = "../../tests/backend/texts/filing.rs"]
mod tests;
