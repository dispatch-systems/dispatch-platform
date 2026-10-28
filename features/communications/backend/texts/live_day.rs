//! The live day: the day live route data holds, while it holds today (`live_route_data`), which
//! Today's Routes lists and the DSP's work conversations wait on. A work conversation waits only on
//! texts written on the live day, from the first after its latest answer or Done: at the stop, as
//! live route data empties, none waits, and the next day starts afresh, with what drivers texted
//! before its first live read. A Slack or number's conversation waits as its messages say,
//! whichever day.
use super::{began, model::TextConversation, storage::TextStore, zone};
use crate::backend::{
    model::{ConversationKind, ConversationState},
    today,
};
use chrono::NaiveDate;
use dispatch_core::{Error, Result, db::Store};
use dispatch_routes_contract::{self as routes, DayPhones, DriverPhone, LiveRouteData, RouteDay};
use std::{
    cmp::Reverse,
    collections::{BTreeMap, HashMap},
};

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
        let date = NaiveDate::parse_from_str(&day, "%Y-%m-%d")
            .map_err(|_| Error::new("invalid_date", 400))?;
        let from = began(zone(db, dsp)?, date)?;
        Ok(Some(Self {
            reader,
            source: reader.source().as_str().to_owned(),
            day,
            from,
        }))
    }
    /// The work phone each driver has now, by their ID, as its ten digits: their latest of today, as
    /// live route data reads it, but one that's another driver's latest too, which is no one
    /// driver's, so neither has it to be texted at.
    pub fn phones(&self, db: &Store, dsp: &str) -> Result<HashMap<String, String>> {
        Ok(match self.reader.day_phones(db, dsp)? {
            DayPhones::Read(phones) => phones
                .into_iter()
                .filter(|each| !each.shared)
                .map(|each| (each.driver_id, each.phone))
                .collect(),
            DayPhones::Uncollected | DayPhones::Unextracted => HashMap::new(),
        })
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

/// `kept` as of the DSP's live day: a work conversation waiting by its texts waits only on those
/// written on it, since the first of them; else it's done with, as at the stop. The live day is
/// read only when one does.
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
    let since: HashMap<String, i64> = match LiveDay::read(db, dsp)? {
        Some(live) => db
            .communications_waiting_from(dsp, &ids, live.from)?
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

/// The work conversations waiting on the live day, the longest waiting first: none without one.
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
