//! Today's Routes, while live route data holds today: each driver its routes name, with their
//! route as last read live and the work phone they hold now, and their work conversation; each
//! driver holding a work phone today but gone from the day's list, once they texted today; and
//! each work phone nobody held that texted today, as itself. At the stop it's empty, and the
//! conversations stay. Offered while the DSP texts and has live route data.
use super::{
    live_day::LiveDay,
    numbers::Numbers,
    offered as texts, reading,
    storage::{TextConversation, TextStore},
};
use crate::{
    api::types::{RouteDriver, TodaysRoutes},
    backend::{people::People, today},
};
use dispatch_core::{Result, State, db::Store, foundation::names};
use dispatch_routes_contract as routes;
use std::collections::{BTreeMap, HashMap};

/// Whether the DSP has Today's Routes: it texts, and has live route data.
pub fn offered(db: &Store, dsp: &str) -> Result<bool> {
    Ok(texts(db, dsp)? && routes::live_for_dsp(db, dsp)?.is_some())
}

/// Today's Routes as it is now: its drivers by name, and the work phones nobody held, the latest
/// written in first.
pub fn read(db: &Store, state: &State, dsp: &str) -> Result<TodaysRoutes> {
    let live = match LiveDay::read(db, dsp)? {
        Some(live) if texts(db, dsp)? => live,
        _ => {
            return Ok(TodaysRoutes {
                day: None,
                drivers: Vec::new(),
                phones: Vec::new(),
            });
        }
    };
    let routes = today::busiest(live.reader.day(db, dsp)?.routes, today::driver);
    let holding = live.holding(db, dsp)?;
    let vans = today::vans(
        db,
        dsp,
        routes.values().filter_map(|route| route.vin.as_ref()),
    )?;
    // Their work conversations, and every one written in today: those of drivers gone from the
    // day's list, and of work phones nobody held.
    let ids: Vec<String> = routes.keys().cloned().collect();
    let mut kept: BTreeMap<String, TextConversation> = BTreeMap::new();
    for conversation in db
        .communications_drivers(dsp, &live.source, &ids)?
        .into_iter()
        .chain(db.communications_work_from(dsp, live.from)?)
    {
        let ours = conversation
            .driver
            .as_ref()
            .is_none_or(|(source, _)| *source == live.source);
        if ours {
            kept.insert(conversation.id.clone(), conversation);
        }
    }
    let people = People::read(db, state, dsp)?;
    let numbers = Numbers::read(db, dsp)?;
    let (mut theirs, mut phones) = (HashMap::new(), Vec::new());
    for conversation in
        reading::conversations(db, &people, &numbers, dsp, kept.into_values().collect())?
    {
        match conversation.driver_id.clone() {
            Some(driver) => {
                theirs.insert(driver, conversation);
            }
            None => phones.push(conversation),
        }
    }
    let gone: Vec<String> = theirs
        .keys()
        .filter(|driver| !routes.contains_key(*driver))
        .cloned()
        .collect();
    let mut drivers: Vec<RouteDriver> = routes
        .keys()
        .chain(&gone)
        .map(|driver| RouteDriver {
            driver_id: driver.clone(),
            person: people.routed(live.reader.source(), driver),
            today: today::shown(routes.get(driver), &vans, holding.get(driver)),
            conversation: theirs.remove(driver),
        })
        .collect();
    drivers.sort_by(|a, b| names::compare(&a.person.name, &b.person.name));
    phones.sort_by(|a, b| b.last_at.cmp(&a.last_at).then_with(|| b.id.cmp(&a.id)));
    Ok(TodaysRoutes {
        day: Some(live.day),
        drivers,
        phones,
    })
}

#[cfg(test)]
#[path = "../../tests/backend/texts/todays_routes.rs"]
mod tests;
