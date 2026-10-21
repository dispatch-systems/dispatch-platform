//! The profile beside a conversation: what's known of the member it's with, besides the name,
//! role and photo the conversation has. Their route today, the DSP's own day, as Routes has read
//! it (`driver_routes`), with what the DSP calls its van as Vans has it (`amazon_fleet`); and how
//! to reach them: their phone number and email address as Driver Match has them (`directory`),
//! and the workspace the conversation is in. Each feature is asked only while it's on for the
//! DSP, and what none knows is left out.
use super::{conversations, storage::CommunicationsStore};
use crate::api::types::{Profile, ProfileToday};
use dispatch_core::{
    Error, Result, State,
    db::Store,
    foundation::clock,
    manifest::people::{self, Person},
};
use dispatch_routes_contract as routes;
use dispatch_slack::kit;
use dispatch_vans_contract as fleet;

/// The most of someone's IDs one read of their routes names, as Routes reads at most.
const MOST_IDS: usize = 16;

/// The profile of the member the conversation `id` is with, today being the day it is in
/// `zone`, the DSP's timezone.
pub fn of(db: &Store, state: &State, dsp: &str, zone: &str, id: &str) -> Result<Profile> {
    let conversation = db
        .communications_conversation(dsp, id)?
        .ok_or_else(|| Error::new("communications_conversation_not_found", 404))?;
    let everyone = people::everyone(db, state, dsp)?;
    let person = everyone.holding(
        kit::SOURCE,
        &kit::member_id(&conversation.team, &conversation.user),
    );
    let today = match person {
        Some(person) => today(db, dsp, zone, person)?,
        None => None,
    };
    let slack = conversations::workspaces(db, dsp)?
        .into_iter()
        .find(|workspace| workspace.team_id == conversation.team)
        .map(|workspace| workspace.name);
    Ok(Profile {
        today,
        phone: person.and_then(|person| person.phones.first().cloned()),
        email: person.and_then(|person| person.emails.first().cloned()),
        slack,
    })
}

/// The route `person` has today, if Routes has read one.
fn today(db: &Store, dsp: &str, zone: &str, person: &Person) -> Result<Option<ProfileToday>> {
    let Some(routes) = routes::for_dsp(db, dsp)? else {
        return Ok(None);
    };
    let ids: Vec<String> = person
        .ids_of(routes.source())
        .iter()
        .take(MOST_IDS)
        .cloned()
        .collect();
    if ids.is_empty() {
        return Ok(None);
    }
    let zone: chrono_tz::Tz = zone
        .parse()
        .map_err(|_| Error::new("invalid_timezone", 400))?;
    let day = clock::today(zone).to_string();
    // Their own route, beside any they rescued, which has fewer stops.
    let driven = routes.routes(db, dsp, &ids, &day, &day)?.routes;
    let Some(route) = driven.into_iter().max_by_key(|route| route.stops) else {
        return Ok(None);
    };
    let van = match (route.vin, fleet::for_dsp(db, dsp)?) {
        (Some(vin), Some(fleet)) => fleet
            .vans(db, dsp, &[vin])?
            .into_iter()
            .next()
            .and_then(|van| van.name),
        _ => None,
    };
    let today = ProfileToday {
        route: route.route_code,
        van,
        wave_at: route.wave_start_at,
        stops: (route.stops > 0).then_some(route.stops),
    };
    let known = today.route.is_some()
        || today.van.is_some()
        || today.wave_at.is_some()
        || today.stops.is_some();
    Ok(known.then_some(today))
}

#[cfg(test)]
#[path = "../tests/backend/profile.rs"]
mod tests;
