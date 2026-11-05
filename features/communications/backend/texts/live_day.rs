//! The live day: the day live route data holds, while it holds today (`live_route_data`), which
//! Today's Routes lists and the DSP's work conversations wait on. A work conversation waits only on
//! texts written on the live day, from the first after its latest answer or Done, and for two hours
//! after it ends, at the stop, as live route data empties (`AFTER_STOP_MS`): when it ended is kept
//! as first seen (`watched`), never past the day's own end. Then none waits, and the next day starts
//! afresh, with what drivers texted before its first live read. For those two hours a driver is
//! answered at the work phone they had at the stop (`Stopped`). A Slack or number's conversation
//! waits as its messages say, whichever day.
use super::{
    began, date,
    model::{Filed, LiveKept, TextConversation},
    storage::{DayStore, PickStore, TextStore},
    zone,
};
use crate::backend::{
    model::{ConversationKind, ConversationState},
    today,
};
use chrono::NaiveDate;
use chrono_tz::Tz;
use dispatch_core::{
    Error, Result,
    db::{Store, now},
};
use dispatch_routes_contract::{
    self as routes, DayPhones, DriverPhone, DriverRoutes, LiveRouteData, RouteDay,
};
use std::{
    cmp::Reverse,
    collections::{BTreeMap, HashMap, HashSet},
};

/// Two hours, in milliseconds: how long after the live day ends, at the stop, work conversations
/// go on waiting as on it.
pub const AFTER_STOP_MS: i64 = 2 * 60 * 60 * 1000;

/// The DSP's live day, and what reads it.
pub struct LiveDay {
    pub reader: &'static dyn LiveRouteData,
    /// The source naming its drivers, which their work conversations are kept by.
    pub source: String,
    /// YYYY-MM-DD in the DSP's own time.
    pub day: String,
    /// When it began where the DSP is (`began`), in milliseconds.
    pub from: i64,
}
impl LiveDay {
    /// The DSP's live day, while live route data holds today.
    pub fn read(db: &Store, dsp: &str) -> Result<Option<Self>> {
        let Some(reader) = routes::live_for_dsp(db, dsp)? else {
            return Ok(None);
        };
        let Some(day) = reader.live_day(db, dsp)? else {
            return Ok(None);
        };
        let from = began(zone(db, dsp)?, date(&day)?)?;
        Ok(Some(Self {
            reader,
            source: reader.source().as_str().to_owned(),
            day,
            from,
        }))
    }
    /// The work phone each driver has now, by their ID, as its ten digits: the one they were last
    /// picked for today that's no one driver's latest, as its texts are filed under them
    /// (`picking`); else their latest of today, as live route data reads it, but one that's another
    /// driver's latest too, which is no one driver's, so neither has it to be texted at.
    pub fn phones(&self, db: &Store, dsp: &str) -> Result<HashMap<String, String>> {
        held(
            db,
            dsp,
            &self.day,
            &self.source,
            self.reader.day_phones(db, dsp)?,
        )
    }
    /// The route today of each of `ids` with the most stops, by their ID (`today::busiest`), as live
    /// route data last read it: every one of them, as many at a time as one read of their routes
    /// names.
    pub fn routes_of(
        &self,
        db: &Store,
        dsp: &str,
        ids: &[String],
    ) -> Result<BTreeMap<String, RouteDay>> {
        let mut driven = Vec::new();
        for some in ids.chunks(routes::MOST_IDS) {
            driven.extend(self.reader.routes(db, dsp, some)?.routes);
        }
        Ok(today::busiest(driven, today::driver))
    }
    /// The work phone the driver holding `ids` has now (`phones`), with the ID that has it: of
    /// their IDs with one, that whose route in `driven`, theirs today (`routes_of`), has the most
    /// stops, the route their day shows (`today`), so its phone is the one shown beside it; of as
    /// many, the first linked.
    pub fn phone_of(
        &self,
        db: &Store,
        dsp: &str,
        ids: &[String],
        driven: &BTreeMap<String, RouteDay>,
    ) -> Result<Option<DriverPhone>> {
        let mut phones = self.phones(db, dsp)?;
        let theirs: Vec<DriverPhone> = ids
            .iter()
            .filter_map(|id| {
                Some(DriverPhone {
                    driver_id: id.clone(),
                    phone: phones.remove(id)?,
                    shared: false,
                })
            })
            .collect();
        let stops = |each: &DriverPhone| driven.get(&each.driver_id).map_or(0, |r| r.stops);
        Ok(theirs.into_iter().min_by_key(|each| Reverse(stops(each))))
    }
    /// Whether the driver `driver` is one of today's: on a route live route data names.
    pub fn drives(&self, db: &Store, dsp: &str, driver: &str) -> Result<bool> {
        let routes = self.reader.day(db, dsp)?.routes;
        Ok(routes.iter().any(|route| route.driver_id == driver))
    }
}

/// The DSP's day that stopped last, while the two hours after it last and live route data holds no
/// day, as historical route data keeps it, which a stop saves it to.
pub struct Stopped {
    pub reader: &'static dyn DriverRoutes,
    /// The source naming its drivers, which their work conversations are kept by.
    pub source: String,
    /// YYYY-MM-DD in the DSP's own time.
    pub day: String,
}
impl Stopped {
    /// The DSP's day that stopped last, while at `at` the two hours after it last, and live route
    /// data holds no day, while Routes is on for the DSP.
    pub fn read(db: &Store, dsp: &str, at: i64) -> Result<Option<Self>> {
        if LiveDay::read(db, dsp)?.is_some() {
            return Ok(None);
        }
        match after_stop(db, dsp, at)? {
            Some((_, day)) => Self::of(db, dsp, &day.to_string()),
            None => Ok(None),
        }
    }
    /// `day`, as stopped, while Routes is on for the DSP.
    fn of(db: &Store, dsp: &str, day: &str) -> Result<Option<Self>> {
        Ok(routes::for_dsp(db, dsp)?.map(|reader| Self {
            reader,
            source: reader.source().as_str().to_owned(),
            day: day.to_owned(),
        }))
    }
    /// The work phone each driver had at the stop, by their ID, as `LiveDay::phones` has them that
    /// day: as historical route data reads it; none while it can't.
    pub fn phones(&self, db: &Store, dsp: &str) -> Result<HashMap<String, String>> {
        let read = self.reader.day_phones(db, dsp, &self.day)?;
        held(db, dsp, &self.day, &self.source, read)
    }
}

/// The work phone each driver of `source` has now, by their ID: on the live day, the one live route
/// data says (`LiveDay::phones`); for the two hours after the stop, the one they had then
/// (`Stopped::phones`); none else.
pub fn phones_now(db: &Store, dsp: &str, source: &str) -> Result<HashMap<String, String>> {
    if let Some(live) = LiveDay::read(db, dsp)? {
        return match live.source == source {
            true => live.phones(db, dsp),
            false => Ok(HashMap::new()),
        };
    }
    match Stopped::read(db, dsp, now())? {
        Some(stopped) if stopped.source == source => stopped.phones(db, dsp),
        _ => Ok(HashMap::new()),
    }
}

/// The work phone each driver of `source` has on `day`, by their ID, from `read`, its drivers'
/// latest: the one they were last picked for that day that's no one driver's latest, as its texts
/// are filed under them (`picking`); else their latest, but one that's another driver's latest too,
/// which is no one driver's, so neither has it to be texted at.
fn held(
    db: &Store,
    dsp: &str,
    day: &str,
    source: &str,
    read: DayPhones,
) -> Result<HashMap<String, String>> {
    let DayPhones::Read(read) = read else {
        return Ok(HashMap::new());
    };
    let latest: HashMap<String, String> = read
        .into_iter()
        .filter(|each| !each.shared)
        .map(|each| (each.driver_id, each.phone))
        .collect();
    let named: HashSet<&String> = latest.values().collect();
    let mut phones = HashMap::new();
    for (phone, picked) in db.communications_picked(dsp, day)? {
        if let Filed::Driver { source: of, id } = picked
            && of == source
            && !named.contains(&phone)
        {
            phones.insert(id, phone);
        }
    }
    for (driver, phone) in latest {
        phones.entry(driver).or_insert(phone);
    }
    Ok(phones)
}

/// `kept` as of the DSP's live day: a work conversation waiting by its texts waits only on those
/// written on it, since the first of them, and for two hours after it ends (`waits_from`); else
/// it's done with. Since when they wait is read only when one does.
pub fn slated(
    db: &Store,
    dsp: &str,
    mut kept: Vec<TextConversation>,
) -> Result<Vec<TextConversation>> {
    let waits = |each: &TextConversation| {
        each.kind == ConversationKind::Work && each.state == ConversationState::Waiting
    };
    let ids: Vec<String> = kept
        .iter()
        .filter(|each| waits(each))
        .map(|each| each.id.clone())
        .collect();
    if ids.is_empty() {
        return Ok(kept);
    }
    let since: HashMap<String, i64> = match waits_from(db, dsp, now())? {
        Some(from) => db
            .communications_waiting_from(dsp, &ids, from)?
            .into_iter()
            .collect(),
        None => HashMap::new(),
    };
    for each in kept.iter_mut().filter(|each| waits(each)) {
        match since.get(&each.id) {
            Some(&at) => each.waiting_since = Some(at),
            None => (each.state, each.waiting_since) = (ConversationState::Done, None),
        }
    }
    Ok(kept)
}

/// Since when the DSP's work conversations wait at `at`, as it began where the DSP is: the live
/// day, while live route data holds today, and until `AFTER_STOP_MS` after it ended; none else.
pub fn waits_from(db: &Store, dsp: &str, at: i64) -> Result<Option<i64>> {
    if let Some(live) = LiveDay::read(db, dsp)? {
        return Ok(Some(live.from));
    }
    match after_stop(db, dsp, at)? {
        Some((zone, day)) => Ok(Some(began(zone, day)?)),
        None => Ok(None),
    }
}
/// The day live route data held last, in the DSP's `zone`, while at `at` the two hours after it
/// ended last.
fn after_stop(db: &Store, dsp: &str, at: i64) -> Result<Option<(Tz, NaiveDate)>> {
    let Some(kept) = db.communications_live_day(dsp)? else {
        return Ok(None);
    };
    let (zone, day) = (zone(db, dsp)?, date(&kept.day)?);
    let over = at >= ended(zone, day, kept.ended_at, at)? + AFTER_STOP_MS;
    Ok((!over).then_some((zone, day)))
}

/// What watching the live day at a moment finds to keep.
pub enum Watched {
    /// Live route data holds `day`, today, as it didn't.
    Live(String),
    /// Live route data held `day` until `at`, and doesn't now.
    Ended { day: String, at: i64 },
    /// The two hours after `day` ended passed: its work conversations, waiting from `from`, don't
    /// any more.
    Over { day: String, from: i64 },
}

/// What's changed of the DSP's live day at `at`, by what was kept of it: none while nothing has.
/// The day ends as first seen not held, though never after the day itself does, as one a stop
/// never saved leaves.
pub fn watched(db: &Store, dsp: &str, at: i64) -> Result<Option<Watched>> {
    let live = match routes::live_for_dsp(db, dsp)? {
        Some(reader) => reader.live_day(db, dsp)?,
        None => None,
    };
    let kept = db.communications_live_day(dsp)?;
    Ok(match (live, kept) {
        (Some(day), Some(kept)) if kept.day == day && kept.ended_at.is_none() => None,
        (Some(day), _) => Some(Watched::Live(day)),
        (None, Some(LiveKept { over: true, .. }) | None) => None,
        (None, Some(kept)) => {
            let (zone, day) = (zone(db, dsp)?, date(&kept.day)?);
            match kept.ended_at {
                None => Some(Watched::Ended {
                    day: kept.day,
                    at: ended(zone, day, None, at)?,
                }),
                Some(ended) if at >= ended + AFTER_STOP_MS => Some(Watched::Over {
                    day: kept.day,
                    from: began(zone, day)?,
                }),
                Some(_) => None,
            }
        }
    })
}
/// Keeps what `watched` found; once the two hours after a stop pass, notes changed the work
/// conversations waiting, and the drivers' who had a work phone at the stop, so the pages waiting
/// on changes hear they don't wait, nor have it, any more.
pub fn keep(db: &Store, dsp: &str, watched: &Watched) -> Result<()> {
    match watched {
        Watched::Live(day) => db.live_communications_day(dsp, day),
        Watched::Ended { day, at } => db.ended_communications_day(dsp, day, *at),
        Watched::Over { day, from } => db.dsp(dsp)?.transaction(|| {
            db.changed_communications_waiting(dsp, *from)?;
            let held: Vec<(String, String)> = match Stopped::of(db, dsp, day)? {
                Some(stopped) => (stopped.phones(db, dsp)?.into_keys())
                    .map(|driver| (stopped.source.clone(), driver))
                    .collect(),
                None => Vec::new(),
            };
            if !held.is_empty() {
                db.changed_communications_drivers(dsp, Some(&held))?;
            }
            db.over_communications_day(dsp, day)
        }),
    }
}

/// When the live day `day` ended, kept as `ended_at`, or else `at`: never after the day did.
fn ended(zone: Tz, day: NaiveDate, ended_at: Option<i64>, at: i64) -> Result<i64> {
    let next = day
        .succ_opt()
        .ok_or_else(|| Error::new("invalid_date", 400))?;
    Ok(ended_at.unwrap_or(at).min(began(zone, next)?))
}

/// The work conversations waiting on the live day, or in the two hours after it, the longest
/// waiting first: none else.
pub fn waiting(db: &Store, dsp: &str) -> Result<Vec<TextConversation>> {
    let mut waiting: Vec<TextConversation> = slated(db, dsp, db.waiting_communications_work(dsp)?)?
        .into_iter()
        .filter(|each| each.state == ConversationState::Waiting)
        .collect();
    waiting.sort_by(|a, b| {
        a.waiting_since
            .cmp(&b.waiting_since)
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(waiting)
}

#[cfg(test)]
#[path = "../../tests/backend/texts/live_day.rs"]
mod tests;
