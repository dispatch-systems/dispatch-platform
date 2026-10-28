//! Filing the texts the DSP's dispatch phones relay, as the phone side keeps them (`phone`): each
//! in its conversation, read on from the last change filed whenever the phone side says texts
//! changed (`told`), once it's kept them, and every minute in case that failed, or Communications
//! came on after they came; and today's filed again every minute too, as its route data may have
//! changed unheard (`reconcile`).
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
//! hearing of the work conversations waiting, which wait only on the live day, and of every
//! driver's, whose phone may have changed.
use super::{
    began,
    model::{Filed, Incoming, TextFile, TextStatus},
    numbers::ten_digits,
    storage::{PickStore, TextStore},
    zone,
};
use crate::backend::{
    live,
    model::Direction,
    phone::{
        self,
        model::{Origin, Status, Text},
        storage::TextStore as _,
    },
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
use serde_json::json;
use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
    time::Duration,
};

/// Two days, in milliseconds: how far back the work conversations today's texts may wait in are
/// looked for, further than a day can be long.
const TWO_DAYS_MS: i64 = 2 * 24 * 60 * 60 * 1000;

/// Every minute, each DSP's texts changed since they were last filed, and today's filed again
/// where they belong now.
pub const MAINTENANCE: Maintenance = Maintenance {
    every: Duration::from_secs(60),
    run: pass,
};

/// The phone side says the DSP's texts changed, once it's kept them: files them. A failure is
/// logged, and the minute's catch-up files them instead.
pub async fn told(state: &Arc<State>, dsp: &str) {
    if let Err(error) = read_on(state, dsp).await {
        failed("communications_texts_failed", &error);
    }
}
/// Routes says the DSP's route data of a day changed: files that day's texts again.
pub fn refile(state: Arc<State>, dsp: String, changed: RouteDataChanged) -> Upkeep {
    Box::pin(async move {
        if let Err(error) = refile_day(&state, &dsp, &changed.day).await {
            failed("communications_texts_refile_failed", &error);
        }
    })
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
                    if let Err(error) = read_on(&state, &dsp).await {
                        failed("communications_texts_failed", &error);
                    }
                    if let Err(error) = reconcile(&state, &dsp).await {
                        failed("communications_texts_refile_failed", &error);
                    }
                }
            }
            Err(error) => failed("communications_texts_failed", &error),
        }
    })
}

fn failed(event: &'static str, error: &Error) {
    observability::event("error", event, json!({"error": error.code}));
}

/// Files the DSP's texts changed since the last filed, a page at a time while the phone side says
/// more remain, and wakes the pages waiting on its conversations once any changed.
pub async fn read_on(state: &Arc<State>, dsp: &str) -> Result<()> {
    loop {
        let at = dsp.to_owned();
        let (changed, more) = state
            .run_scoped(dsp, DOMAIN, move |db| {
                let after = db.communications_texts_read_to(&at)?;
                let changes = db.phones_changes(&at, after)?;
                if changes.next == after {
                    return Ok((false, false));
                }
                let mut filer = Filer::new(db, &at)?;
                let senders = senders(db, &changes.texts)?;
                let ids: Vec<String> = changes.texts.iter().map(|text| text.id.clone()).collect();
                let kept_already = db.communications_texts_kept(&at, &ids)?;
                let incoming = changes
                    .texts
                    .iter()
                    .map(|text| {
                        // Only a text new to Communications is placed: one kept already stays
                        // where it is until its day's texts are filed again.
                        let filed = (!kept_already.contains(&text.id))
                            .then(|| filer.placed(&text.number, text.at))
                            .transpose()?;
                        let sender = text.sent_by.as_ref().and_then(|by| senders.get(by));
                        Ok(kept(text, filed, sender.cloned()))
                    })
                    .collect::<Result<Vec<_>>>()?;
                let touched = db.file_communications_texts(&at, &incoming, changes.next)?;
                Ok((!touched.is_empty(), changes.more))
            })
            .await?;
        if changed {
            live::changed(state, dsp);
        }
        if !more {
            return Ok(());
        }
    }
}

/// Files every text of `day`, YYYY-MM-DD in the DSP's time, again, where it belongs now, then wakes
/// the pages waiting on what changed. Today's, noting changed the work conversations waiting, as
/// the live day may have begun or ended, and every driver's, texted in or not, as the phone they
/// have may have changed. A day Routes can't read keeps its texts where they are.
pub async fn refile_day(state: &Arc<State>, dsp: &str, day: &str) -> Result<()> {
    let day =
        NaiveDate::parse_from_str(day, "%Y-%m-%d").map_err(|_| Error::new("invalid_date", 400))?;
    let at = dsp.to_owned();
    let changed = state
        .run_scoped(dsp, DOMAIN, move |db| {
            let mut filer = Filer::new(db, &at)?;
            let moved = refiled_by(db, &at, &mut filer, day)?;
            if day == clock::today(filer.zone) {
                db.changed_communications_work(&at, now() - TWO_DAYS_MS)?;
                return Ok(true);
            }
            Ok(!moved.is_empty())
        })
        .await?;
    if changed {
        live::changed(state, dsp);
    }
    Ok(())
}
/// Files every text of today, the DSP's day, again, where it belongs now, as its route data may have
/// changed unheard: while Communications was off, or as a hearing failed. Wakes the pages waiting on
/// its conversations only once one moved.
pub async fn reconcile(state: &Arc<State>, dsp: &str) -> Result<()> {
    let at = dsp.to_owned();
    let moved = state
        .run_scoped(dsp, DOMAIN, move |db| refiled(db, &at))
        .await?;
    if moved {
        live::changed(state, dsp);
    }
    Ok(())
}
/// Files every text of today, the DSP's day, again, where it belongs now: whether any moved.
pub fn refiled(db: &Store, dsp: &str) -> Result<bool> {
    let mut filer = Filer::new(db, dsp)?;
    let today = clock::today(filer.zone);
    Ok(!refiled_by(db, dsp, &mut filer, today)?.is_empty())
}
/// Files every text of `day` again, from when it began until the next day did, where `filer` says
/// it belongs now: the conversations changed. One whose place can't be told yet stays where it is.
fn refiled_by(
    db: &Store,
    dsp: &str,
    filer: &mut Filer,
    day: NaiveDate,
) -> Result<BTreeSet<String>> {
    let next = day
        .succ_opt()
        .ok_or_else(|| Error::new("invalid_date", 400))?;
    let (from, to) = (began(filer.zone, day)?, began(filer.zone, next)?);
    let mut moves = Vec::new();
    for text in db.communications_texts_between(dsp, from, to)? {
        if let Some(filed) = filer.filed(&text.number, text.at)? {
            moves.push((text.id, filed));
        }
    }
    db.refile_communications_texts(dsp, &moves)
}

/// Where a text with `number` written now belongs, as filing would place it: what someone at the
/// DSP opens to text the number first, so their text and its answers go there.
pub fn belongs(db: &Store, dsp: &str, number: &str) -> Result<Filed> {
    Filer::new(db, dsp)?.placed(number, now())
}

/// The name each one at the DSP who sent a text of `texts` is shown by, by their account: their
/// full name as they sent it, as an answer from the page goes under.
fn senders(db: &Store, texts: &[Text]) -> Result<HashMap<String, String>> {
    let mut senders = HashMap::new();
    for by in texts.iter().filter_map(|text| text.sent_by.as_ref()) {
        if !senders.contains_key(by)
            && let Some(name) = db.platform_user_name(by)?
        {
            senders.insert(by.clone(), sender::full(&name));
        }
    }
    Ok(senders)
}

/// `text` as Communications keeps it, filed as `filed` if it's new, sent by someone at the DSP
/// shown by `sender_name`.
fn kept(text: &Text, filed: Option<Filed>, sender_name: Option<String>) -> Incoming {
    Incoming {
        id: text.id.clone(),
        phone: text.phone.clone(),
        direction: match text.direction {
            phone::model::Direction::In => Direction::In,
            phone::model::Direction::Out => Direction::Out,
        },
        number: text.number.clone(),
        body: text.body.clone(),
        at: text.at,
        sent_by: text.sent_by.clone(),
        sender_name,
        status: (text.origin == Origin::Dispatch)
            .then_some(text.status)
            .flatten()
            .map(|status| match status {
                Status::Waiting => TextStatus::Waiting,
                Status::Sent => TextStatus::Sent,
                Status::Delivered => TextStatus::Delivered,
                Status::Failed => TextStatus::Failed,
            }),
        error: text.error.clone(),
        files: text
            .files
            .iter()
            .map(|file| TextFile {
                id: file.id.clone(),
                name: file.name.clone(),
                mime: file.mime.clone(),
                size: file.size,
                kept: file.kept,
            })
            .collect(),
        filed,
    }
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
