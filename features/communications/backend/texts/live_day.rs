//! The live day: the day live route data holds, while it holds today (`live_route_data`), which
//! Today's Routes lists and the DSP's work conversations wait on. A work conversation waits only on
//! texts written on the live day, from the first after its latest answer or Done: at the stop, as
//! live route data empties, none waits, and the next day starts afresh, with what drivers texted
//! before its first live read. A Slack or number's conversation waits as its messages say,
//! whichever day.
use super::{
    began,
    storage::{ConversationKind, TextConversation, TextStore},
    zone,
};
use crate::backend::storage::ConversationState;
use chrono::NaiveDate;
use dispatch_core::{Error, Result, db::Store};
use dispatch_routes_contract::{self as routes, LiveRouteData, WorkPhone};
use std::collections::HashMap;

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
    /// The work phone each driver holds now, by their ID: the latest they took.
    pub fn holding(&self, db: &Store, dsp: &str) -> Result<HashMap<String, WorkPhone>> {
        let mut holding: HashMap<String, WorkPhone> = HashMap::new();
        for held in self.reader.work_phones(db, dsp)? {
            if holding
                .get(&held.driver_id)
                .is_none_or(|kept| held.since > kept.since)
            {
                holding.insert(held.driver_id.clone(), held);
            }
        }
        Ok(holding)
    }
    /// The work phone the driver holding `ids` holds now, if any: the latest they took.
    pub fn held_by(&self, db: &Store, dsp: &str, ids: &[String]) -> Result<Option<WorkPhone>> {
        let mut holding = self.holding(db, dsp)?;
        Ok(ids
            .iter()
            .filter_map(|id| holding.remove(id))
            .max_by_key(|held| held.since))
    }
    /// Whether the driver `driver` is one of today's: on a route live route data names, or
    /// holding a work phone today.
    pub fn drives(&self, db: &Store, dsp: &str, driver: &str) -> Result<bool> {
        let routed = self
            .reader
            .day(db, dsp)?
            .routes
            .iter()
            .any(|route| route.driver_id == driver);
        Ok(routed
            || self
                .reader
                .work_phone_holds(db, dsp)?
                .iter()
                .any(|hold| hold.driver_id == driver))
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
