//! Phone/day attribution: live route phones while they hold the day, otherwise historical
//! phones, with the ever-work-phone fallback for a number no one in Driver Match has. A handoff moves the whole day's texts in
//! both directions. DSP-local days include the prior stop's window when it crosses midnight.
//!
//! An unread day says nothing: existing placements stay put, but a known number's under a work
//! phone nobody had (`reconciliation::elsewhere`); new texts use the work phone's own
//! conversation when known, otherwise the number's. Status and attachment updates never move a
//! text already filed. Actor names are read once per placement batch. Only Dispatch Phone's own
//! line's texts are filed so: another line's are each under its number, on that line.
use super::super::{
    live_day,
    model::{Filed, Unfiled},
    numbers::ten_digits,
    storage::TextStore,
    zone,
};
use super::failed;
use crate::backend::sender;
use dispatch_core::{
    Error, Result, State,
    db::{Store, now},
    foundation::clock,
    manifest::people::{self, Everyone},
};
use dispatch_routes_contract::{
    self as routes, DayPhones, DriverPhone, DriverRoutes, LiveRouteData,
};
use jiff::civil::Date;
use jiff::tz::TimeZone;
use serde_json::json;
use std::{collections::HashMap, sync::Arc};

/// Places each of `texts`, waiting to be filed, where it belongs on its phone's line, under the
/// name its sender, if someone at the DSP, is shown by as it's placed: each on its own, so one that
/// can't be yet, as reading what says where failed, is logged, kept as unplaced since now
/// (`unplaced`), and waits for the next upkeep, the rest placed.
pub(super) fn place(db: &Store, state: &State, dsp: &str, texts: &[Unfiled]) -> Result<()> {
    let mut filer = None;
    let mut senders = HashMap::new();
    for text in texts {
        let placed = place_of(db, state, dsp, &mut filer, text).and_then(|filed| {
            let sender = sender_name(db, &mut senders, text.sent_by.as_deref())?;
            let line = text.line.as_deref();
            db.place_communications_text(dsp, &text.id, line, &filed, sender.as_deref())
        });
        if let Err(error) = placed {
            unplaced(db, dsp, std::slice::from_ref(text), &error)?;
        }
    }
    Ok(())
}
/// Where `text` is placed: another line's under its number; Dispatch Phone's own where `filer`
/// says, made for the first of them.
fn place_of<'a>(
    db: &'a Store,
    state: &'a State,
    dsp: &'a str,
    filer: &mut Option<Filer<'a>>,
    text: &Unfiled,
) -> Result<Filed> {
    if text.line.is_some() {
        return Ok(Filed::Number(text.number.clone()));
    }
    let filer = match filer {
        Some(filer) => filer,
        None => filer.insert(Filer::new(db, state, dsp)?),
    };
    filer.placed(&text.number, text.at)
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
/// conversation while Routes says it ever was one and no one in Driver Match has it, else the
/// number's, as when Routes can't say.
pub(super) fn unread(db: &Store, state: &State, dsp: &str, number: &str) -> Filed {
    let ever = |ten: String| match routes::for_dsp(db, dsp)? {
        Some(routes) => routes.ever_work_phone(db, dsp, &ten),
        None => Ok(false),
    };
    let nobodys = || people::everyone(db, state, dsp).map(|everyone| !someones(&everyone, number));
    match ten_digits(number).map(ever) {
        Some(Ok(true)) if matches!(nobodys(), Ok(true)) => Filed::WorkPhone(number.to_owned()),
        _ => Filed::Number(number.to_owned()),
    }
}
/// Whether someone in Driver Match has `number`, from a source or added: then it's their own, never
/// a work phone nobody has, though route data once named it a work phone, as when a driver ran
/// the route on their own phone.
fn someones(everyone: &Everyone, number: &str) -> bool {
    !everyone.by_phone(number).is_empty()
}

/// Where a text with `number` written now belongs, as filing would place it: what someone at the
/// DSP opens to text the number first, so their text and its answers go there.
pub fn belongs(db: &Store, state: &State, dsp: &str, number: &str) -> Result<Filed> {
    Filer::new(db, state, dsp)?.placed(number, now())
}

/// The name the one at the DSP who sent a text, `by`, is shown by, by their account: their full
/// name as they sent it, as an answer from the page goes under. Each read once, into `senders`.
pub(super) fn sender_name(
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
pub(super) struct Filer<'a> {
    db: &'a Store,
    state: &'a State,
    dsp: &'a str,
    pub(super) zone: TimeZone,
    live: Option<&'static dyn LiveRouteData>,
    live_day: Option<String>,
    routes: Option<&'static dyn DriverRoutes>,
    /// Each day's work phones, or none for a day Routes can't read.
    days: HashMap<String, Option<Phones>>,
    /// Whether each number, by its ten digits, was ever a work phone.
    work_phones: HashMap<String, bool>,
    /// When the two hours after the day before each day stopped end, while they run into it.
    after_stop: HashMap<Date, Option<i64>>,
    /// Everyone Driver Match has, read once a number may be a work phone nobody has.
    everyone: Option<Arc<Everyone>>,
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
    pub(super) fn new(db: &'a Store, state: &'a State, dsp: &'a str) -> Result<Self> {
        let zone = zone(db, dsp)?;
        let live = routes::live_for_dsp(db, dsp)?;
        let live_day = match live {
            Some(live) => live.live_day(db, dsp)?,
            None => None,
        };
        Ok(Self {
            db,
            state,
            dsp,
            zone,
            live,
            live_day,
            routes: routes::for_dsp(db, dsp)?,
            days: HashMap::new(),
            work_phones: HashMap::new(),
            after_stop: HashMap::new(),
            everyone: None,
        })
    }
    /// Where a text with `number`, written at `at`, is filed: under the driver whose latest work
    /// phone it is that day; else under the work phone, or the number, as itself, the number's
    /// while someone in Driver Match has it (`someones`). None while what would tell can't be read:
    /// that day's work phones, or whether the number was ever one, as Routes reads them. Routes
    /// being off isn't the number not being a work phone.
    pub(super) fn filed(&mut self, number: &str, at: i64) -> Result<Option<Filed>> {
        let Some(ten) = ten_digits(number) else {
            return Ok(Some(Filed::Number(number.to_owned())));
        };
        let day = self.day_of(at)?.to_string();
        let Some(phones) = self.phones(&day)? else {
            return Ok(None);
        };
        match phones.drivers.get(&ten) {
            Some(Some(driver)) => Ok(Some(Filed::Driver {
                source: phones.source.clone(),
                id: driver.clone(),
            })),
            Some(None) => self.work_phone(number).map(Some),
            None => self.own(number, &ten),
        }
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
    /// Routes says it ever was one (`work_phone`), the number's while it says not; none while
    /// Routes is off.
    fn own(&mut self, number: &str, ten: &str) -> Result<Option<Filed>> {
        match self.ever_work_phone(ten)? {
            Some(true) => self.work_phone(number).map(Some),
            Some(false) => Ok(Some(Filed::Number(number.to_owned()))),
            None => Ok(None),
        }
    }
    /// The conversation of `number`, a work phone no one driver has: the work phone's own, but the
    /// number's while someone in Driver Match has it (`someones`).
    fn work_phone(&mut self, number: &str) -> Result<Filed> {
        Ok(match self.someones(number)? {
            true => Filed::Number(number.to_owned()),
            false => Filed::WorkPhone(number.to_owned()),
        })
    }
    /// Whether someone in Driver Match has `number` (`someones`), as everyone is read once.
    pub(super) fn someones(&mut self, number: &str) -> Result<bool> {
        let everyone = match &self.everyone {
            Some(everyone) => everyone.clone(),
            None => {
                let everyone = people::everyone(self.db, self.state, self.dsp)?;
                self.everyone.insert(everyone).clone()
            }
        };
        Ok(someones(&everyone, number))
    }
    /// The day a text written at `at` is filed by: its own, where the DSP is, but in the two hours
    /// after a stop that run past midnight, the day stopped, as a driver is answered then at the
    /// work phone they had at the stop (`live_day::Stopped`), so the answer is theirs too.
    fn day_of(&mut self, at: i64) -> Result<Date> {
        let day = clock::day(&self.zone, at);
        if !self.after_stop.contains_key(&day) {
            let until = live_day::after_stop_into(self.db, self.dsp, &self.zone, day)?;
            self.after_stop.insert(day, until);
        }
        Ok(match self.after_stop[&day] {
            Some(until) if at < until => day.yesterday().ok().unwrap_or(day),
            _ => day,
        })
    }
    /// Whether Routes can say anything of `day`: live route data holds it, or historical route data
    /// is on for the DSP.
    pub(super) fn reads(&self, day: &str) -> bool {
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
