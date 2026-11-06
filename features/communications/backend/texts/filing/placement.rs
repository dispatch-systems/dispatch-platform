//! Phone/day attribution: live route phones while they hold the day, otherwise historical
//! phones, with picks and ever-work-phone fallback. A handoff moves the whole day's texts in
//! both directions. DSP-local days include the prior stop's window when it crosses midnight.
//!
//! An unread day says nothing: existing placements stay put; new texts use the work phone's own
//! conversation when known, otherwise the number's. Status and attachment updates never move a
//! text already filed. Actor names are read once per placement batch.
use super::super::{
    live_day,
    model::{Filed, Unfiled},
    numbers::ten_digits,
    storage::{PickStore, TextStore},
    zone,
};
use super::failed;
use crate::backend::sender;
use chrono::NaiveDate;
use chrono_tz::Tz;
use dispatch_core::{
    Error, Result,
    db::{Store, now},
    foundation::clock,
};
use dispatch_routes_contract::{
    self as routes, DayPhones, DriverPhone, DriverRoutes, LiveRouteData,
};
use serde_json::json;
use std::collections::HashMap;

/// Places each of `texts`, waiting to be filed, where it belongs, under the name its sender, if
/// someone at the DSP, is shown by as it's placed: each on its own, so one that can't be yet, as
/// reading what says where failed, is logged, kept as unplaced since now (`unplaced`), and waits
/// for the next upkeep, the rest placed.
pub(super) fn place(db: &Store, dsp: &str, texts: &[Unfiled]) -> Result<()> {
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
pub(super) fn unread(db: &Store, dsp: &str, number: &str) -> Filed {
    let ever = |ten: String| match routes::for_dsp(db, dsp)? {
        Some(routes) => routes.ever_work_phone(db, dsp, &ten),
        None => Ok(false),
    };
    match ten_digits(number).map(ever) {
        Some(Ok(true)) => Filed::WorkPhone(number.to_owned()),
        _ => Filed::Number(number.to_owned()),
    }
}

/// Where a text with `number` written now belongs, as filing would place it: what someone at the
/// DSP opens to text the number first, so their text and its answers go there.
pub fn belongs(db: &Store, dsp: &str, number: &str) -> Result<Filed> {
    Filer::new(db, dsp)?.placed(number, now())
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
    dsp: &'a str,
    pub(super) zone: Tz,
    live: Option<&'static dyn LiveRouteData>,
    live_day: Option<String>,
    routes: Option<&'static dyn DriverRoutes>,
    /// Each day's work phones, or none for a day Routes can't read.
    days: HashMap<String, Option<Phones>>,
    /// Each day's picks: the driver picked for each work phone, by its ten digits.
    picked: HashMap<String, HashMap<String, Filed>>,
    /// Whether each number, by its ten digits, was ever a work phone.
    work_phones: HashMap<String, bool>,
    /// When the two hours after the day before each day stopped end, while they run into it.
    after_stop: HashMap<NaiveDate, Option<i64>>,
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
    pub(super) fn new(db: &'a Store, dsp: &'a str) -> Result<Self> {
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
            after_stop: HashMap::new(),
        })
    }
    /// Where a text with `number`, written at `at`, is filed: under the driver whose latest work
    /// phone it is that day, else the driver picked for it that day; else under the work phone, or
    /// the number, as itself. None while what would tell can't be read: that day's work phones, or
    /// whether the number was ever one, as Routes reads them. Routes being off isn't the number not
    /// being a work phone.
    pub(super) fn filed(&mut self, number: &str, at: i64) -> Result<Option<Filed>> {
        let Some(ten) = ten_digits(number) else {
            return Ok(Some(Filed::Number(number.to_owned())));
        };
        let day = self.day_of(at)?.to_string();
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
    /// The day a text written at `at` is filed by: its own, where the DSP is, but in the two hours
    /// after a stop that run past midnight, the day stopped, as a driver is answered then at the
    /// work phone they had at the stop (`live_day::Stopped`), so the answer is theirs too.
    fn day_of(&mut self, at: i64) -> Result<NaiveDate> {
        let day = clock::day(self.zone, at);
        if !self.after_stop.contains_key(&day) {
            let until = live_day::after_stop_into(self.db, self.dsp, self.zone, day)?;
            self.after_stop.insert(day, until);
        }
        Ok(match self.after_stop[&day] {
            Some(until) if at < until => day.pred_opt().unwrap_or(day),
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
