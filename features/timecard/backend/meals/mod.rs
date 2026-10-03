#[path = "assessment.rs"]
pub mod assessment;
#[path = "comparison.rs"]
pub(crate) mod comparison;
#[path = "keeper.rs"]
pub mod keeper;
#[path = "people.rs"]
pub mod people;
#[path = "sync.rs"]
pub(crate) mod sync;
// Cortex meal evidence and atomic publication. Browser data is untrusted input.
use crate::collectors::cortex::{
    self,
    discovery::Scope,
    meals::{Capture, Coverage, Itinerary, Meal},
};
use chrono::NaiveDate;
use dispatch_core::{
    Result,
    db::{Store, at, s},
    ensure,
    server::cache::DataDomain,
};
use rusqlite::params;
use serde_json::{Value, json};
use std::collections::HashSet;

/// Cortex's meal breaks, as Timecard keeps them.
pub const DOMAIN: DataDomain = DataDomain::new("meals");
/// A DSP's meal-break reads: Paycom (including live pages), Cortex meals, driver links and
/// preferences.
pub const CACHED: &str = "meals";

struct Boundaries {
    prior: Option<i64>,
    next: Option<i64>,
    prior_stop: Option<u32>,
    next_stop: Option<u32>,
    before: &'static str,
    after: &'static str,
}
fn boundaries(route: &Itinerary, meal: &Meal) -> Boundaries {
    if route.delivery_coverage != Coverage::Complete {
        return Boundaries {
            prior: None,
            next: None,
            prior_stop: None,
            next_stop: None,
            before: "unavailable",
            after: if meal.end.is_none() {
                "pending"
            } else {
                "unavailable"
            },
        };
    }
    let prior = meal.last_delivery;
    let next = meal.first_delivery;
    Boundaries {
        prior,
        next,
        prior_stop: meal.last_delivery_stop,
        next_stop: meal.first_delivery_stop,
        before: if prior.is_some() {
            "verified"
        } else {
            "absent"
        },
        after: if meal.end.is_none() {
            "pending"
        } else if next.is_some() {
            "verified"
        } else if route.route_complete {
            "absent"
        } else {
            "pending"
        },
    }
}
/// The route's page with one of its stops selected, as Cortex links a stop.
pub(crate) fn stop_url(route: &str, stop: u32) -> Option<String> {
    let mut url = url::Url::parse(route).ok()?;
    url.query_pairs_mut()
        .append_pair("selectedStopId", &stop.to_string());
    Some(url.into())
}
/// A meal as the comparison reads it. The stops become links beside the route's own.
pub fn comparison_meal(route: &Itinerary, meal: &Meal) -> Value {
    let b = boundaries(route, meal);
    json!({"mealId":meal.id,"lastDelivery":b.prior.map(at),"start":at(meal.start),"end":meal.end.map(at),
        "firstDelivery":b.next.map(at),"beforeStatus":b.before,"afterStatus":b.after,
        "lastDeliveryStop":b.prior_stop,"firstDeliveryStop":b.next_stop})
}
pub(crate) fn publish_meals(
    store: &Store,
    dsp: &str,
    job: &str,
    capture: &Capture,
    expected: &Scope,
) -> Result<Value> {
    capture.validate(expected)?;
    let db = store.collector(dsp, cortex::PROVIDER)?;
    db.transaction(|| {
        // Recovered workers may repeat publication after a crash between databases.
        if let Some(existing)=db.one("SELECT id FROM meal_publications WHERE job_id=?",[job])? {return Ok(existing);}
        let previous=db.one("SELECT id,collected_at FROM meal_publications WHERE \
            active=1 AND report_date=? AND station=? AND service_area_id=? AND \
            provider=?",params![expected.date,expected.station,expected.service_area_id,expected.provider])?;
        if let Some(previous)=&previous {
            ensure(s(previous,"collected_at")<=at(capture.finished_at).as_str(),"cortex_stale_capture",409)?;
            let ids:HashSet<_>=capture.itineraries.iter().map(|i|i.id.as_str()).collect();
            for route in db.all("SELECT itinerary_id FROM meal_itineraries WHERE publication_id=?",[s(previous,"id")])? {
                ensure(ids.contains(s(&route,"itinerary_id")),"cortex_membership_regressed",409)?;
            }
        }
        let id=dispatch_core::foundation::crypto::id("pub")?;
        let meals=capture.itineraries.iter().map(|r|r.meals.len()).sum::<usize>();
        let gaps=capture.itineraries.iter().flat_map(|r|r.meals.iter().map(move|m|boundaries(r,
            m))).filter(|b|b.prior.is_some()&&b.next.is_some()).count();
        db.exec("INSERT INTO \
            meal_publications(id,job_id,report_date,station,service_area_id,provider,timezone,started_at,\
            collected_at,itinerary_count,meal_count,verified_gap_count,adapter_version) VALUES (?,?,?,?,?,\
            ?,?,?,?,?,?,?,4)",params![id,job,expected.date,expected.station,
            expected.service_area_id,expected.provider,expected.timezone,
            at(capture.started_at),at(capture.finished_at),capture.itineraries.len() as i64,
            meals as i64,gaps as i64])?;
        for route in &capture.itineraries {
            let state=if route.meals.is_empty(){"none_recorded"}
            else if route.meals.iter().any(|m|m.end.is_none()){"in_progress"}else{"recorded"};

            db.exec("INSERT INTO meal_itineraries VALUES \
                (?,?,?,?,?,?,?,?,?)",params![id,route.id,route.transporter_id,
            route.driver,route.route,at(route.observed_at),route.route_complete,
            if route.delivery_coverage==Coverage::Complete{"complete"}else{"unavailable"},
            state])?;
            if let Some(url)=&route.source_url {
                db.exec("INSERT INTO meal_sources VALUES (?,?,?)",params![id,route.id,url])?;
            }
            for meal in &route.meals {
                let b=boundaries(route,meal);
                db.exec("INSERT INTO \
                    meal_records(publication_id,itinerary_id,meal_id,last_delivery_at,started_at,ended_at,\
            first_delivery_at,before_status,after_status) VALUES (?,?,?,?,?,?,?,?,?)",
            params![id,route.id,meal.id,b.prior.map(at),at(meal.start),
            meal.end.map(at),b.next.map(at),b.before,b.after])?;
                if b.prior_stop.is_some()||b.next_stop.is_some() {
                    db.exec("INSERT INTO meal_stops VALUES (?,?,?,?,?)",
                        params![id,route.id,meal.id,b.prior_stop,b.next_stop])?;
                }
            }
        }
        if let Some(previous)=previous {db.exec("UPDATE meal_publications SET active=0 WHERE id=?",[s(&previous,"id")])?;}
        db.exec("UPDATE meal_publications SET active=1 WHERE id=?",[&id])?;
        // Keep the active version and four prior accepted versions per scope.
        db.exec("DELETE FROM meal_publications WHERE active=0 AND report_date=? AND \
            station=? AND service_area_id=? AND provider=? AND id NOT IN (SELECT id FROM \
            meal_publications WHERE active=0 AND report_date=? AND station=? AND \
            service_area_id=? AND provider=? ORDER BY collected_at DESC,id DESC LIMIT \
            4)",params![expected.date,expected.station,expected.service_area_id,
            expected.provider,expected.date,expected.station,expected.service_area_id,
            expected.provider])?;
        Ok(json!({"id":id,"itineraries":capture.itineraries.len(),"meals":meals,"verifiedGapPairs":gaps}))
    })
}
pub(crate) fn meal_publications(store: &Store, dsp: &str, date: &str) -> Result<Value> {
    ensure(
        NaiveDate::parse_from_str(date, "%Y-%m-%d").is_ok(),
        "invalid_date",
        400,
    )?;
    Ok(json!(store.collector(dsp, cortex::PROVIDER)?.all(
        "SELECT id,job_id \
        jobId,report_date date,station,service_area_id \
        serviceAreaId,provider,timezone,started_at startedAt,collected_at \
        collectedAt,itinerary_count itineraryCount,meal_count \
        mealCount,verified_gap_count verifiedGapPairs FROM meal_publications WHERE \
        report_date=? AND active=1 ORDER BY station,service_area_id,provider",
        [date]
    )?))
}
