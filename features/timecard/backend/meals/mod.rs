pub(crate) mod assessment;
pub(crate) mod comparison;
pub(crate) mod keeper;
pub(crate) mod people;
pub(crate) mod sync;
// Cortex meal evidence and atomic publication. Browser data is untrusted input.
use chrono::NaiveDate;
use dispatch_core::{
    Result,
    db::{Store, at, s},
    ensure,
    server::cache::DataDomain,
};
use dispatch_cortex::{
    self as cortex,
    discovery::Scope,
    meals::{Capture, Coverage, Itinerary, Meal},
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
/// The routes of the scope's active publication, as Cortex collected them. Only a
/// publication that kept each delivery's stop stands in for a collection.
pub(crate) fn kept_routes(store: &Store, dsp: &str, scope: &Scope) -> Result<Vec<Itinerary>> {
    let db = store.collector(dsp, cortex::PROVIDER)?;
    let Some(publication) = db.one(
        "SELECT id FROM meal_publications WHERE active=1 AND report_date=? AND station=? AND \
         service_area_id=? AND provider=? AND timezone=? AND adapter_version>=4",
        params![
            scope.date,
            scope.station,
            scope.service_area_id,
            scope.provider,
            scope.timezone
        ],
    )?
    else {
        return Ok(Vec::new());
    };
    let id = s(&publication, "id");
    let time = |row: &Value, field: &str| -> Result<Option<i64>> {
        row[field]
            .as_str()
            .map(|text| {
                chrono::DateTime::parse_from_rfc3339(text)
                    .map(|t| t.timestamp_millis())
                    .map_err(|_| dispatch_core::Error::new("invalid_cortex_capture", 500))
            })
            .transpose()
    };
    let stop = |row: &Value, field: &str| row[field].as_u64().and_then(|v| u32::try_from(v).ok());
    let mut meals: std::collections::HashMap<String, Vec<Meal>> = Default::default();
    for row in db.all(
        "SELECT m.itinerary_id,m.meal_id,m.last_delivery_at,m.started_at,m.ended_at,\
         m.first_delivery_at,t.last_delivery_stop,t.first_delivery_stop FROM meal_records m \
         LEFT JOIN meal_stops t USING(publication_id,itinerary_id,meal_id) \
         WHERE m.publication_id=? ORDER BY m.itinerary_id,m.started_at,m.meal_id",
        [id],
    )? {
        let meal = Meal {
            id: s(&row, "meal_id").into(),
            start: time(&row, "started_at")?
                .ok_or_else(|| dispatch_core::Error::new("invalid_cortex_capture", 500))?,
            end: time(&row, "ended_at")?,
            last_delivery: time(&row, "last_delivery_at")?,
            first_delivery: time(&row, "first_delivery_at")?,
            last_delivery_stop: stop(&row, "last_delivery_stop"),
            first_delivery_stop: stop(&row, "first_delivery_stop"),
        };
        meals
            .entry(s(&row, "itinerary_id").into())
            .or_default()
            .push(meal);
    }
    db.all(
        "SELECT i.itinerary_id,i.transporter_id,i.driver_name,i.route_code,i.observed_at,\
         i.route_complete,i.delivery_coverage,u.url FROM meal_itineraries i \
         LEFT JOIN meal_sources u USING(publication_id,itinerary_id) \
         WHERE i.publication_id=? ORDER BY i.itinerary_id",
        [id],
    )?
    .iter()
    .map(|row| {
        Ok(Itinerary {
            id: s(row, "itinerary_id").into(),
            transporter_id: s(row, "transporter_id").into(),
            driver: s(row, "driver_name").into(),
            route: s(row, "route_code").into(),
            observed_at: time(row, "observed_at")?
                .ok_or_else(|| dispatch_core::Error::new("invalid_cortex_capture", 500))?,
            route_complete: row["route_complete"] == 1,
            delivery_coverage: if s(row, "delivery_coverage") == "complete" {
                Coverage::Complete
            } else {
                Coverage::Unavailable
            },
            meals: meals.remove(s(row, "itinerary_id")).unwrap_or_default(),
            source_url: row["url"].as_str().map(Into::into),
        })
    })
    .collect()
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
