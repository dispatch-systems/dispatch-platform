//! Filing the texts the DSP's dispatch phones relay, as Phones offers them (`texting`): each in
//! its conversation, read on from the last change filed whenever Phones says texts changed
//! (`TextsChanged`), and every minute in case a hearing was missed or failed, or Communications
//! came on after they came; and today's filed again every minute too, as who held the work phones
//! may have changed unheard (`reconcile`).
//!
//! A work phone passes from driver to driver, so a text with one is filed under whoever held it
//! as it was written, by their ID of the source that names who held the DSP's work phones, as
//! Routes knows it: live route data's holds while its live day is the text's, else that day's
//! saved holds. Of the number's holds that day, the one covering when it was written; written
//! before any, the day's first holder, as it came before the day's first live read or before they
//! signed in; written once nobody held it, the work phone's own. A work phone nobody held that
//! day is still one (`was_work_phone`). Where someone at the DSP picked a driver for the phone
//! that day (`picking`), a text no hold covers is that driver's, before the day's first holder.
//! Any other number's texts are its own. While Routes is off for the DSP, nothing tells whether a
//! number was a work phone, so a text that comes then is placed under its number until it can be
//! filed. A text is placed as it first comes; how it went, or its files coming, never moves it.
//! Who held which phone settles as the day goes on, and a day's holds may reach historical route
//! data after its texts, as a text a phone relayed late; so a day's texts are filed again each
//! time Routes says its holds changed (`WorkPhonesChanged`): today's, the pages waiting on changes
//! hearing of the work conversations waiting, which wait only on the live day, and of every
//! driver's, whose phone may have changed or been withdrawn; an earlier day's while Routes keeps
//! holds of it, as a day it emptied unsaved keeps its texts where its holds had them.
use super::{
    began,
    model::{Filed, Incoming, TextFile, TextStatus},
    numbers::ten_digits,
    storage::{PickStore, TextStore},
    zone,
};
use crate::backend::{live, model::Direction, storage::DOMAIN};
use chrono::NaiveDate;
use chrono_tz::Tz;
use dispatch_core::{
    Error, Result, State,
    db::{Store, now},
    foundation::{clock, observability},
    manifest::{Maintenance, Upkeep},
};
use dispatch_phones_contract::{self as texting, Origin, Status, Text, TextsChanged};
use dispatch_routes_contract::{
    self as routes, DriverRoutes, LiveRouteData, WorkPhoneHold, WorkPhonesChanged,
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

/// Phones says the DSP's texts changed: files them.
pub fn heard(state: Arc<State>, dsp: String, _: TextsChanged) -> Upkeep {
    Box::pin(async move {
        if let Err(error) = read_on(&state, &dsp).await {
            failed("communications_texts_failed", &error);
        }
    })
}
/// Routes says who held the DSP's work phones on a day changed: files that day's texts again.
pub fn refile(state: Arc<State>, dsp: String, changed: WorkPhonesChanged) -> Upkeep {
    Box::pin(async move {
        if let Err(error) = refile_day(&state, &dsp, &changed.day).await {
            failed("communications_texts_refile_failed", &error);
        }
    })
}

fn pass(state: Arc<State>, _: bool) -> Upkeep {
    Box::pin(async move {
        let texting = state
            .read(|db| {
                let mut found = Vec::new();
                for dsp in db.kept_dsps()? {
                    if db.feature_enabled(&dsp, "communications")?
                        && texting::for_dsp(db, &dsp)?.is_some()
                    {
                        found.push(dsp);
                    }
                }
                Ok(found)
            })
            .await;
        match texting {
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

/// Files the DSP's texts changed since the last filed, a page at a time while texting says more
/// remain, and wakes the pages waiting on its conversations once any changed.
pub async fn read_on(state: &Arc<State>, dsp: &str) -> Result<()> {
    loop {
        let at = dsp.to_owned();
        let (changed, more) = state
            .run_scoped(dsp, DOMAIN, move |db| {
                let Some(texting) = texting::for_dsp(db, &at)? else {
                    return Ok((false, false));
                };
                let after = db.communications_texts_read_to(&at)?;
                let changes = texting.changes(db, &at, after)?;
                if changes.next == after {
                    return Ok((false, false));
                }
                let mut filer = Filer::new(db, &at)?;
                let incoming = changes
                    .texts
                    .iter()
                    .map(|text| Ok(kept(text, filer.placed(&text.number, text.at)?)))
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
/// hold may have changed, or been withdrawn from them. An earlier day's only while Routes keeps
/// holds of it.
pub async fn refile_day(state: &Arc<State>, dsp: &str, day: &str) -> Result<()> {
    let day =
        NaiveDate::parse_from_str(day, "%Y-%m-%d").map_err(|_| Error::new("invalid_date", 400))?;
    let at = dsp.to_owned();
    let changed = state
        .run_scoped(dsp, DOMAIN, move |db| {
            let mut filer = Filer::new(db, &at)?;
            if day == clock::today(filer.zone) {
                refiled_by(db, &at, &mut filer, day)?;
                db.changed_communications_work(&at, now() - TWO_DAYS_MS)?;
                return Ok(true);
            }
            if filer.holds(&day.to_string())?.is_none_or(<[_]>::is_empty) {
                return Ok(false);
            }
            Ok(!refiled_by(db, &at, &mut filer, day)?.is_empty())
        })
        .await?;
    if changed {
        live::changed(state, dsp);
    }
    Ok(())
}
/// Files every text of today, the DSP's day, again, where it belongs now, as who held its work
/// phones may have changed unheard: while Communications was off, or as a hearing failed. Wakes the
/// pages waiting on its conversations only once one moved.
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

/// `text` as Communications keeps it, filed as `filed`.
fn kept(text: &Text, filed: Filed) -> Incoming {
    Incoming {
        id: text.id.clone(),
        phone: text.phone.clone(),
        direction: match text.direction {
            texting::Direction::In => Direction::In,
            texting::Direction::Out => Direction::Out,
        },
        number: text.number.clone(),
        body: text.body.clone(),
        at: text.at,
        sent_by: text.sent_by.clone(),
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

/// Where the DSP's texts are filed, by who held its work phones on the days they were written,
/// each day's holds read once.
struct Filer<'a> {
    db: &'a Store,
    dsp: &'a str,
    zone: Tz,
    live: Option<&'static dyn LiveRouteData>,
    live_day: Option<String>,
    routes: Option<&'static dyn DriverRoutes>,
    /// Each day's holds, each with the source of the ID that held it.
    holds: HashMap<String, Vec<(String, WorkPhoneHold)>>,
    /// Each day's picks: the driver picked for each work phone, by its ten digits.
    picked: HashMap<String, HashMap<String, Filed>>,
    /// Whether each number, by its ten digits, was ever a work phone.
    work_phones: HashMap<String, bool>,
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
            holds: HashMap::new(),
            picked: HashMap::new(),
            work_phones: HashMap::new(),
        })
    }
    /// Where a text with `number`, written at `at`, is filed: under the driver whose hold covers
    /// it, else the driver picked for the phone that day, else the day's first holder when it came
    /// before any hold; else under the work phone, or the number, as itself. None while what would
    /// tell isn't on for the DSP: that day's holds, or whether the number was ever a work phone,
    /// as Routes knows them. Routes being off isn't the number not being a work phone.
    fn filed(&mut self, number: &str, at: i64) -> Result<Option<Filed>> {
        let Some(ten) = ten_digits(number) else {
            return Ok(Some(Filed::Number(number.to_owned())));
        };
        let day = clock::day(self.zone, at).to_string();
        let Some(holds) = self.holds(&day)? else {
            return Ok(None);
        };
        let held: Vec<(String, WorkPhoneHold)> = holds
            .iter()
            .filter(|(_, hold)| hold.phone == ten)
            .cloned()
            .collect();
        let driver = |(source, hold): &(String, WorkPhoneHold)| Filed::Driver {
            source: source.clone(),
            id: hold.driver_id.clone(),
        };
        let covering = held
            .iter()
            .find(|(_, hold)| hold.since <= at && hold.until.is_none_or(|until| at < until));
        if let Some(covering) = covering {
            return Ok(Some(driver(covering)));
        }
        if let Some(picked) = self.picked(&day)?.get(&ten) {
            return Ok(Some(picked.clone()));
        }
        let first = held
            .iter()
            .min_by_key(|(_, hold)| hold.since)
            .filter(|(_, hold)| at < hold.since);
        if let Some(first) = first {
            return Ok(Some(driver(first)));
        }
        let work_phone = if held.is_empty() {
            self.was_work_phone(&ten)?
        } else {
            Some(true)
        };
        Ok(work_phone.map(|was| match was {
            true => Filed::WorkPhone(number.to_owned()),
            false => Filed::Number(number.to_owned()),
        }))
    }
    /// Where a text new to Communications, with `number`, written at `at`, is placed: where it's
    /// filed, else, while that can't be told, under its number, to be filed again once it can.
    fn placed(&mut self, number: &str, at: i64) -> Result<Filed> {
        Ok(self
            .filed(number, at)?
            .unwrap_or_else(|| Filed::Number(number.to_owned())))
    }
    /// Every hold on a work phone on `day`: live route data's while it holds that day, else the
    /// day's saved ones; none while neither is on for the DSP.
    fn holds(&mut self, day: &str) -> Result<Option<&[(String, WorkPhoneHold)]>> {
        if !self.holds.contains_key(day) {
            let read = match (self.live, self.routes) {
                (Some(live), _) if self.live_day.as_deref() == Some(day) => {
                    let source = live.source().as_str().to_owned();
                    live.work_phone_holds(self.db, self.dsp)?
                        .into_iter()
                        .map(|hold| (source.clone(), hold))
                        .collect()
                }
                (_, Some(routes)) => {
                    let source = routes.source().as_str().to_owned();
                    routes
                        .work_phone_holds(self.db, self.dsp, day)?
                        .into_iter()
                        .map(|hold| (source.clone(), hold))
                        .collect()
                }
                _ => return Ok(None),
            };
            self.holds.insert(day.to_owned(), read);
        }
        Ok(Some(&self.holds[day]))
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
    fn was_work_phone(&mut self, ten: &str) -> Result<Option<bool>> {
        let Some(routes) = self.routes else {
            return Ok(None);
        };
        if let Some(&was) = self.work_phones.get(ten) {
            return Ok(Some(was));
        }
        let was = routes.was_work_phone(self.db, self.dsp, ten)?;
        self.work_phones.insert(ten.to_owned(), was);
        Ok(Some(was))
    }
}

#[cfg(test)]
#[path = "../../tests/backend/texts/filing.rs"]
mod tests;
