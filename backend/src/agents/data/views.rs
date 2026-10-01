//! Each endpoint's answer. Every answer opens with what it understood (the DSP with its
//! own date, the period, the driver) and says which days each source has.
use super::{
    Answer, Refusal,
    catalog::{self, METRICS, Metric, flag},
    facts::{self, Coverage, Inspection, MealDay, RouteDay, Sources, TimecardDay, clock},
    scope::{People, Period, Person, param, period, pick_dsp, today},
};
use crate::{
    State,
    agents::Caller,
    contracts::{DriverSource, Dsp, MealStatus},
    db::Store,
};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

fn about(dsp: &Dsp) -> Value {
    json!({"id": dsp.id, "name": dsp.name, "timezone": dsp.timezone, "today": today(dsp).to_string()})
}
fn spanned(period: &Period) -> Value {
    json!({
        "from": period.first(), "to": period.last(),
        "days": (period.to - period.from).num_days() + 1, "understood": period.label
    })
}
fn person_json(person: &Person) -> Value {
    json!({
        "code": person.code, "name": person.name, "match": person.status,
        "paycomIds": person.paycom, "amazonIds": person.amazon
    })
}
/// Who a source's row belongs to: their Driver Match person, or the ID as the source wrote
/// it when nobody holds it yet.
fn who(people: &People, source: DriverSource, id: &str, name: &str) -> Value {
    match people.holder(source, id) {
        Some(person) => json!({"code": person.code, "name": person.name}),
        None => json!({"code": null, "name": name, "id": id}),
    }
}
fn not_on(source: &str) -> Refusal {
    Refusal::new(
        403,
        "source_off",
        format!("This DSP has {source} switched off."),
    )
}
fn one_day(period: &Period) -> Result<(), Refusal> {
    if period.from == period.to {
        Ok(())
    } else {
        Err(Refusal::new(
            400,
            "one_day_only",
            "This answers one day at a time; give `date`.",
        ))
    }
}

/// `GET /api/v1/status`: which sources are on and how fresh each is.
pub fn status(db: &Store, caller: &Caller, query: &Value) -> Answer {
    catalog::check("status", query)?;
    let dsp = pick_dsp(caller, param(query, "dsp"))?;
    let on = Sources::of(db, &dsp.id)?;
    let fresh = facts::freshness(db, dsp)?;
    let source = |enabled: bool, key: &str| {
        let mut value = json!({"enabled": enabled});
        if enabled && let Some(found) = fresh[key].as_object() {
            for (k, v) in found {
                value[k] = v.clone();
            }
        }
        value
    };
    Ok(json!({
        "dsp": about(dsp),
        "sources": {
            "timecards": source(on.timecards, "timecards"),
            "mealBreaks": source(on.meal_breaks, "mealBreaks"),
            "routes": source(on.routes, "routes"),
            "dvic": source(on.dvic, "dvic"),
        }
    }))
}

/// `GET /api/v1/metrics`.
pub fn metrics(query: &Value) -> Answer {
    catalog::check("metrics", query)?;
    Ok(catalog::metrics())
}

/// `GET /api/v1/drivers`: everyone with a code, or those a search finds.
pub fn drivers(db: &Store, state: &State, caller: &Caller, query: &Value) -> Answer {
    catalog::check("drivers", query)?;
    let dsp = pick_dsp(caller, param(query, "dsp"))?;
    let people = People::load(db, state, &dsp.id)?;
    let wanted = param(query, "q").to_lowercase();
    let limit = param(query, "limit").parse().unwrap_or(200usize);
    let found: Vec<&Person> = people
        .list
        .iter()
        .filter(|p| {
            wanted.is_empty()
                || p.name.to_lowercase().contains(&wanted)
                || p.code.to_lowercase() == wanted
                || p.paycom
                    .iter()
                    .chain(&p.amazon)
                    .any(|id| id.to_lowercase() == wanted)
        })
        .collect();
    Ok(json!({
        "dsp": about(dsp),
        "found": found.len(),
        "drivers": found.iter().take(limit).map(|p| person_json(p)).collect::<Vec<_>>(),
    }))
}

struct Gathered {
    routes: (Vec<RouteDay>, Coverage),
    timecards: (Vec<TimecardDay>, Coverage),
    meals: (Vec<MealDay>, Coverage),
    inspections: (Vec<Inspection>, Coverage),
}
/// What each switched-on source holds for the period, all of it or one person's.
fn gather(
    db: &Store,
    dsp: &Dsp,
    period: &Period,
    people: &People,
    person: Option<&Person>,
    wanted: &[&str],
) -> crate::Result<Gathered> {
    let on = Sources::of(db, &dsp.id)?;
    let wants = |source: &str| wanted.contains(&source);
    let amazon = person.map(|p| p.amazon.as_slice());
    let paycom = person.map(|p| p.paycom.as_slice());
    let routes = if on.routes && wants("routes") {
        facts::routes(db, dsp, period, amazon)?
    } else {
        (vec![], Coverage::default())
    };
    let timecards = if on.timecards && wants("timecards") {
        facts::timecards(db, dsp, period, paycom)?
    } else {
        (vec![], Coverage::default())
    };
    let meals = if on.meal_breaks && wants("meal_breaks") {
        let (mut rows, coverage) = facts::meal_breaks(db, dsp, period)?;
        mark_routes(db, dsp, period, people, &mut rows)?;
        let rows = match person {
            Some(p) => rows
                .into_iter()
                .filter(|row| {
                    let (kind, id) = row.source.split_once(':').unwrap_or(("", ""));
                    (kind == "paycom" && p.paycom.iter().any(|c| c == id))
                        || (kind == "cortex" && p.amazon.iter().any(|t| t == id))
                })
                .collect(),
            None => rows,
        };
        (rows, coverage)
    } else {
        (vec![], Coverage::default())
    };
    let inspections = if on.dvic && wants("dvic") {
        facts::inspections(db, dsp, period, amazon)?
    } else {
        (vec![], Coverage::default())
    };
    Ok(Gathered {
        routes,
        timecards,
        meals,
        inspections,
    })
}
fn coverage(gathered: &Gathered) -> Value {
    json!({
        "routes": gathered.routes.1,
        "timecards": gathered.timecards.1,
        "mealBreaks": gathered.meals.1,
        "dvic": gathered.inspections.1,
    })
}
const ALL: &[&str] = &["routes", "timecards", "meal_breaks", "dvic"];

/// Marks each meal row whose person Cortex had a route for that day.
fn mark_routes(
    db: &Store,
    dsp: &Dsp,
    period: &Period,
    people: &People,
    meals: &mut [MealDay],
) -> crate::Result<()> {
    let routes = facts::meal_routes(db, dsp, period)?;
    for meal in meals {
        let (kind, id) = meal.source.split_once(':').unwrap_or(("", ""));
        let ids: Vec<String> = if kind == "cortex" {
            vec![id.to_owned()]
        } else {
            people
                .holder(DriverSource::Paycom, id)
                .map(|p| p.amazon.clone())
                .unwrap_or_default()
        };
        meal.cortex_route = ids
            .into_iter()
            .any(|id| routes.contains(&(meal.date.clone(), id)));
    }
    Ok(())
}
/// A meal break to look at: the comparison found something on a day the driver had a route.
fn meal_issue(meal: &MealDay) -> bool {
    meal.cortex_route && meal.status != MealStatus::Same
}

fn day<'a>(
    days: &'a mut BTreeMap<String, Map<String, Value>>,
    date: &str,
) -> &'a mut Map<String, Value> {
    days.entry(date.to_owned()).or_insert_with(|| {
        let mut entry = Map::new();
        entry.insert("date".into(), json!(date));
        entry
    })
}
fn push(entry: &mut Map<String, Value>, list: &str, value: Value) {
    if let Some(items) = entry
        .entry(list)
        .or_insert_with(|| json!([]))
        .as_array_mut()
    {
        items.push(value);
    }
}

/// `GET /api/v1/drivers/{driver}`: one driver's days, with totals.
pub fn driver(db: &Store, state: &State, caller: &Caller, wanted: &str, query: &Value) -> Answer {
    catalog::check("driver", query)?;
    let dsp = pick_dsp(caller, param(query, "dsp"))?;
    let people = People::load(db, state, &dsp.id)?;
    let person = people.find(wanted)?;
    let period = period(query, today(dsp), "last 7 days")?;
    let gathered = gather(db, dsp, &period, &people, Some(person), ALL)?;
    let mut days: BTreeMap<String, Map<String, Value>> = BTreeMap::new();
    for route in &gathered.routes.0 {
        push(day(&mut days, &route.date), "routes", json!(route));
    }
    for card in &gathered.timecards.0 {
        day(&mut days, &card.date).insert("timecard".into(), json!(card));
    }
    for meal in &gathered.meals.0 {
        day(&mut days, &meal.date).insert("mealBreaks".into(), json!(meal));
    }
    for inspection in &gathered.inspections.0 {
        push(
            day(&mut days, &inspection.date),
            "inspections",
            json!(inspection),
        );
    }
    let routes = &gathered.routes.0;
    let sum = |f: fn(&RouteDay) -> i64| routes.iter().map(f).sum::<i64>();
    let hours: f64 = gathered.timecards.0.iter().map(|c| c.hours).sum();
    let totals = json!({
        "routes": routes.len(),
        "stopsCompleted": sum(|r| r.stops_completed),
        "stopsTotal": sum(|r| r.stops_total),
        "packagesDelivered": sum(|r| r.packages_delivered),
        "packagesTotal": sum(|r| r.packages_total),
        "hoursWorked": (hours * 100.0).round() / 100.0,
        "daysWorked": gathered.timecards.0.iter().filter(|c| c.hours > 0.0).count(),
        "mealIssues": gathered.meals.0.iter().filter(|m| meal_issue(m)).count(),
        "inspections": gathered.inspections.0.len(),
        "shortInspections": gathered.inspections.0.iter().filter(|i| i.short).count(),
    });
    Ok(json!({
        "dsp": about(dsp),
        "period": spanned(&period),
        "driver": person_json(person),
        "totals": totals,
        "days": days.into_values().collect::<Vec<_>>(),
        "coverage": coverage(&gathered),
    }))
}

/// One driver's (or one driver-day's) numbers as `team` adds them up.
#[derive(Default)]
struct Tally {
    routes: Vec<RouteDay>,
    timecards: Vec<TimecardDay>,
    meals: Vec<MealDay>,
    inspections: Vec<Inspection>,
    who: Value,
    date: Option<String>,
}
impl Tally {
    fn value(&self, metric: &Metric) -> Value {
        let routes =
            || -> Option<&Vec<RouteDay>> { (!self.routes.is_empty()).then_some(&self.routes) };
        let sum = |f: fn(&RouteDay) -> i64| routes().map(|r| json!(r.iter().map(f).sum::<i64>()));
        let opt_sum = |f: fn(&RouteDay) -> Option<i64>| {
            routes().map(|r| json!(r.iter().filter_map(f).sum::<i64>()))
        };
        let cards = (!self.timecards.is_empty()).then_some(&self.timecards);
        let meals = (!self.meals.is_empty()).then_some(&self.meals);
        let inspections = (!self.inspections.is_empty()).then_some(&self.inspections);
        match metric.name {
            "routes" => routes().map(|r| json!(r.len())),
            "stops_completed" => sum(|r| r.stops_completed),
            "stops_total" => sum(|r| r.stops_total),
            "packages_delivered" => sum(|r| r.packages_delivered),
            "packages_total" => sum(|r| r.packages_total),
            "packages_remaining" => sum(|r| r.packages_remaining),
            "packages_undeliverable" => sum(|r| r.packages_undeliverable),
            "break_minutes" => opt_sum(|r| r.break_minutes),
            "overtime_minutes" => opt_sum(|r| r.overtime_minutes),
            "hours_worked" => cards.map(|c| {
                let hours: f64 = c.iter().map(|c| c.hours).sum();
                json!((hours * 100.0).round() / 100.0)
            }),
            "days_worked" => cards.map(|c| json!(c.iter().filter(|c| c.hours > 0.0).count())),
            "lunch_minutes" => {
                cards.map(|c| json!(c.iter().filter_map(|c| c.lunch_minutes).sum::<i64>()))
            }
            "clock_in" => cards
                .and_then(|c| c.first()?.clock_in.clone())
                .map(Value::from),
            "clock_out" => cards
                .and_then(|c| c.last()?.clock_out.clone())
                .map(Value::from),
            "meal_issues" => meals.map(|m| json!(m.iter().filter(|m| meal_issue(m)).count())),
            "meal_status" => meals.and_then(|m| m.first()).map(|m| json!(m.status)),
            "inspections" => inspections.map(|i| json!(i.len())),
            "short_inspections" => inspections.map(|i| json!(i.iter().filter(|i| i.short).count())),
            _ => None,
        }
        .unwrap_or(Value::Null)
    }
}

/// The row a source's record adds to: one per person, or per person per day. A person is
/// keyed by their code, or by the source's ID while nobody holds it.
fn tally<'a>(
    tallies: &'a mut BTreeMap<(String, String), Tally>,
    per_day: bool,
    date: &str,
    whom: Value,
) -> &'a mut Tally {
    let id = whom["code"]
        .as_str()
        .or(whom["id"].as_str())
        .unwrap_or_default()
        .to_owned();
    let key = (
        if per_day {
            date.to_owned()
        } else {
            String::new()
        },
        id,
    );
    let entry = tallies.entry(key).or_default();
    if entry.who.is_null() {
        entry.who = whom;
        entry.date = per_day.then(|| date.to_owned());
    }
    entry
}

/// `GET /api/v1/team`: chosen metrics for every driver, per driver or per driver per day.
pub fn team(db: &Store, state: &State, caller: &Caller, query: &Value) -> Answer {
    catalog::check("team", query)?;
    let dsp = pick_dsp(caller, param(query, "dsp"))?;
    let period = period(query, today(dsp), "yesterday")?;
    let per_day = param(query, "per") == "day";
    let asked = param(query, "metrics");
    let names: Vec<&str> = if asked.is_empty() {
        vec!["stops_completed", "packages_delivered", "hours_worked"]
    } else {
        asked
            .split(',')
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .collect()
    };
    let mut chosen: Vec<&Metric> = vec![];
    for name in &names {
        let Some(metric) = catalog::metric(name) else {
            return Err(Refusal::new(
                400,
                "unknown_metric",
                format!("There is no metric `{name}`; see /api/v1/metrics."),
            )
            .choices(METRICS.iter().map(|m| m.name.to_owned()).collect())
            .into());
        };
        if metric.total == "day" && !per_day {
            return Err(Refusal::new(
                400,
                "day_metric",
                format!("`{name}` exists only per day; add per=day."),
            )
            .into());
        }
        if !chosen.iter().any(|m| m.name == metric.name) {
            chosen.push(metric);
        }
    }
    let sources: Vec<&str> = chosen.iter().map(|m| m.source).collect();
    let on = Sources::of(db, &dsp.id)?;
    for (source, enabled, label) in [
        ("routes", on.routes, "routes"),
        ("timecards", on.timecards, "timecards"),
        ("meal_breaks", on.meal_breaks, "meal breaks"),
        ("dvic", on.dvic, "DVIC"),
    ] {
        if sources.contains(&source) && !enabled {
            return Err(not_on(label).into());
        }
    }
    let people = People::load(db, state, &dsp.id)?;
    let gathered = gather(db, dsp, &period, &people, None, &sources)?;
    // Each row's key: the person's code, or the source's ID for someone without one yet.
    let mut tallies: BTreeMap<(String, String), Tally> = BTreeMap::new();
    for route in &gathered.routes.0 {
        let whom = who(
            &people,
            DriverSource::Amazon,
            &route.transporter_id,
            &route.driver_name,
        );
        tally(&mut tallies, per_day, &route.date, whom)
            .routes
            .push(route.clone());
    }
    for card in &gathered.timecards.0 {
        let whom = who(
            &people,
            DriverSource::Paycom,
            &card.employee_code,
            &card.name,
        );
        tally(&mut tallies, per_day, &card.date, whom)
            .timecards
            .push(card.clone());
    }
    for meal in &gathered.meals.0 {
        let (kind, id) = meal.source.split_once(':').unwrap_or(("", ""));
        let source = if kind == "paycom" {
            DriverSource::Paycom
        } else {
            DriverSource::Amazon
        };
        let whom = who(&people, source, id, &meal.name);
        tally(&mut tallies, per_day, &meal.date, whom)
            .meals
            .push(meal.clone());
    }
    for inspection in &gathered.inspections.0 {
        let whom = who(
            &people,
            DriverSource::Amazon,
            &inspection.transporter_id,
            &inspection.driver_name,
        );
        tally(&mut tallies, per_day, &inspection.date, whom)
            .inspections
            .push(inspection.clone());
    }
    let mut rows: Vec<Value> = tallies
        .into_values()
        .map(|t| {
            let mut row = Map::new();
            if let Some(date) = &t.date {
                row.insert("date".into(), json!(date));
            }
            row.insert("driver".into(), t.who.clone());
            for metric in &chosen {
                row.insert(metric.name.into(), t.value(metric));
            }
            Value::Object(row)
        })
        .collect();
    let sort = param(query, "sort");
    let by = if sort.is_empty() {
        chosen.first().map_or("name", |m| m.name)
    } else {
        sort
    };
    if by != "name" && !chosen.iter().any(|m| m.name == by) {
        return Err(Refusal::new(
            400,
            "invalid_sort",
            "Sort by `name` or one of the metrics asked for.",
        )
        .choices(
            std::iter::once("name")
                .chain(chosen.iter().map(|m| m.name))
                .map(str::to_owned)
                .collect(),
        )
        .into());
    }
    rows.sort_by(|a, b| {
        let name = |row: &Value| {
            row["driver"]["name"]
                .as_str()
                .unwrap_or_default()
                .to_lowercase()
        };
        if by == "name" {
            return name(a)
                .cmp(&name(b))
                .then_with(|| a["date"].as_str().cmp(&b["date"].as_str()));
        }
        // Highest first; a driver with nothing for the metric goes last.
        let value = |row: &Value| row[by].as_f64();
        match (value(a), value(b)) {
            (Some(x), Some(y)) => y.total_cmp(&x),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        }
        .then_with(|| name(a).cmp(&name(b)))
    });
    // The whole team's figure for each metric that adds up, from every row, so nobody
    // has to add the rows themselves; a metric no row has stays unknown.
    let mut totals = Map::new();
    for metric in chosen.iter().filter(|m| m.total != "day") {
        let values: Vec<f64> = rows
            .iter()
            .filter_map(|r| r[metric.name].as_f64())
            .collect();
        let sum: f64 = values.iter().sum();
        let value = if values.is_empty() {
            Value::Null
        } else if values.iter().all(|v| v.fract() == 0.0) {
            json!(sum as i64)
        } else {
            json!((sum * 100.0).round() / 100.0)
        };
        totals.insert(metric.name.into(), value);
    }
    let row_count = rows.len();
    rows.truncate(param(query, "limit").parse().unwrap_or(200));
    Ok(json!({
        "dsp": about(dsp),
        "period": spanned(&period),
        "per": if per_day { "day" } else { "driver" },
        "metrics": chosen.iter().map(|m| json!({"name": m.name, "unit": m.unit})).collect::<Vec<_>>(),
        "sortedBy": by,
        "totals": totals,
        "rowCount": row_count,
        "rows": rows,
        "coverage": coverage(&gathered),
    }))
}

/// `GET /api/v1/routes`: one day's itineraries.
pub fn routes(db: &Store, state: &State, caller: &Caller, query: &Value) -> Answer {
    catalog::check("routes", query)?;
    let dsp = pick_dsp(caller, param(query, "dsp"))?;
    if !Sources::of(db, &dsp.id)?.routes {
        return Err(not_on("routes").into());
    }
    let period = period(query, today(dsp), "today")?;
    one_day(&period)?;
    let people = People::load(db, state, &dsp.id)?;
    let (found, coverage) = facts::routes(db, dsp, &period, None)?;
    let rows: Vec<Value> = found
        .iter()
        .map(|route| {
            let mut row = json!(route);
            row["driver"] = who(
                &people,
                DriverSource::Amazon,
                &route.transporter_id,
                &route.driver_name,
            );
            row
        })
        .collect();
    let sum = |f: fn(&RouteDay) -> i64| found.iter().map(f).sum::<i64>();
    let collected = !coverage.days.is_empty();
    // A day not collected has no totals: nothing is known, which is not zero.
    let totals = collected.then(|| {
        json!({
            "routes": found.len(),
            "stopsCompleted": sum(|r| r.stops_completed),
            "stopsTotal": sum(|r| r.stops_total),
            "packagesDelivered": sum(|r| r.packages_delivered),
            "packagesTotal": sum(|r| r.packages_total),
        })
    });
    let mut answer = json!({
        "dsp": about(dsp),
        "date": period.first(),
        "final": coverage.snapshots.is_empty() && collected,
        "collected": collected,
        "totals": totals,
        "itineraries": rows,
    });
    if !collected {
        answer["note"] = json!(format!(
            "No routes have been collected for {}; their numbers are unknown, not zero.",
            period.first()
        ));
    }
    Ok(answer)
}

fn place(
    caller: &Caller,
    address: Option<&crate::contracts::RouteAddress>,
    lat: Option<f64>,
    lon: Option<f64>,
) -> Value {
    if !caller.locations {
        return Value::Null;
    }
    json!({"address": address, "scanLatitude": lat, "scanLongitude": lon})
}

/// `GET /api/v1/routes/{itinerary}`: one itinerary's stops and packages.
pub fn route(db: &Store, state: &State, caller: &Caller, itinerary: &str, query: &Value) -> Answer {
    catalog::check("route", query)?;
    let dsp = pick_dsp(caller, param(query, "dsp"))?;
    if !Sources::of(db, &dsp.id)?.routes {
        return Err(not_on("routes").into());
    }
    let period = period(query, today(dsp), "today")?;
    one_day(&period)?;
    let Some(detail) = db.route_itinerary(&dsp.id, &period.first(), itinerary)? else {
        return Err(Refusal::new(
            404,
            "itinerary_not_found",
            format!(
                "No itinerary {itinerary} on {}; list them with /api/v1/routes.",
                period.first()
            ),
        )
        .into());
    };
    let people = People::load(db, state, &dsp.id)?;
    let zone = facts::zone(dsp);
    let stops: Vec<Value> = detail
        .stops
        .iter()
        .map(|stop| {
            json!({
                "sequence": stop.sequence,
                "type": stop.stop_type,
                "plannedStart": clock(stop.planned_start_at, zone),
                "plannedEnd": clock(stop.planned_end_at, zone),
                "place": place(caller, stop.address.as_ref(), None, None),
                "packages": stop.tasks.iter().map(|task| json!({
                    "tracking": task.tracking_id,
                    "type": task.task_type,
                    "outcome": task.task_state,
                    "reason": task.state_context,
                    "scanned": clock(task.executed_at, zone),
                    "scan": if caller.locations { json!({"latitude": task.latitude, "longitude": task.longitude}) } else { Value::Null },
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    let it = &detail.itinerary;
    Ok(json!({
        "dsp": about(dsp),
        "date": period.first(),
        "final": detail.publication.mode == "final",
        "itinerary": it.itinerary_id,
        "route": it.route_code,
        "driver": who(&people, DriverSource::Amazon, &it.transporter_id, &it.driver_name),
        "addresses": caller.locations,
        "stops": stops,
    }))
}

/// `GET /api/v1/packages/{tracking}`: who carried a package and what happened to it.
pub fn package(
    db: &Store,
    state: &State,
    caller: &Caller,
    tracking: &str,
    query: &Value,
) -> Answer {
    catalog::check("package", query)?;
    let dsp = pick_dsp(caller, param(query, "dsp"))?;
    if !Sources::of(db, &dsp.id)?.routes {
        return Err(not_on("routes").into());
    }
    // Amazon writes tracking IDs in capitals; an agent may not.
    let tracking = tracking.trim().to_uppercase();
    let found = db.route_package(&dsp.id, &tracking)?;
    if found.events.is_empty() {
        return Err(Refusal::new(
            404,
            "package_not_found",
            format!("No collected route carried {tracking}."),
        )
        .into());
    }
    let people = People::load(db, state, &dsp.id)?;
    let zone = facts::zone(dsp);
    Ok(json!({
        "dsp": about(dsp),
        "tracking": found.tracking_id,
        "events": found.events.iter().map(|e| json!({
            "date": e.day,
            "route": e.route_code,
            "itinerary": e.itinerary_id,
            "driver": who(&people, DriverSource::Amazon, &e.transporter_id, e.driver_name.as_deref().unwrap_or("")),
            "type": e.task.task_type,
            "outcome": e.task.task_state,
            "reason": e.task.state_context,
            "scanned": clock(e.task.executed_at, zone),
            "place": place(caller, e.address.as_ref(), e.task.latitude, e.task.longitude),
        })).collect::<Vec<_>>(),
    }))
}

/// `GET /api/v1/timecards`: everyone's for a day, or one driver's for a period.
pub fn timecards(db: &Store, state: &State, caller: &Caller, query: &Value) -> Answer {
    catalog::check("timecards", query)?;
    let dsp = pick_dsp(caller, param(query, "dsp"))?;
    if !Sources::of(db, &dsp.id)?.timecards {
        return Err(not_on("timecards").into());
    }
    let people = People::load(db, state, &dsp.id)?;
    let named = param(query, "driver");
    let person = if named.is_empty() {
        None
    } else {
        Some(people.find(named)?)
    };
    let period = period(
        query,
        today(dsp),
        if person.is_some() {
            "last 7 days"
        } else {
            "today"
        },
    )?;
    if person.is_none() {
        one_day(&period)?;
    }
    let (cards, coverage) =
        facts::timecards(db, dsp, &period, person.map(|p| p.paycom.as_slice()))?;
    Ok(json!({
        "dsp": about(dsp),
        "period": spanned(&period),
        "driver": person.map(person_json),
        "timecards": cards.iter().map(|card| {
            let mut row = json!(card);
            row["driver"] = who(&people, DriverSource::Paycom, &card.employee_code, &card.name);
            row
        }).collect::<Vec<_>>(),
        "coverage": coverage,
    }))
}

/// `GET /api/v1/meal-breaks`: one day's comparison.
pub fn meal_breaks(db: &Store, state: &State, caller: &Caller, query: &Value) -> Answer {
    catalog::check("meal_breaks", query)?;
    let dsp = pick_dsp(caller, param(query, "dsp"))?;
    if !Sources::of(db, &dsp.id)?.meal_breaks {
        return Err(not_on("meal breaks").into());
    }
    let period = period(query, today(dsp), "today")?;
    one_day(&period)?;
    let people = People::load(db, state, &dsp.id)?;
    let (mut rows, coverage) = facts::meal_breaks(db, dsp, &period)?;
    mark_routes(db, dsp, &period, &people, &mut rows)?;
    let issues = flag(query, "issues");
    let whom = |row: &MealDay| {
        let (kind, id) = row.source.split_once(':').unwrap_or(("", ""));
        let source = if kind == "paycom" {
            DriverSource::Paycom
        } else {
            DriverSource::Amazon
        };
        who(&people, source, id, &row.name)
    };
    let (driving, staying): (Vec<&MealDay>, Vec<&MealDay>) =
        rows.iter().partition(|row| row.cortex_route);
    Ok(json!({
        "dsp": about(dsp),
        "date": period.first(),
        "collected": !coverage.days.is_empty(),
        "drivers": driving.iter().filter(|r| !issues || meal_issue(r)).map(|row| {
            let mut value = json!(row);
            value["driver"] = whom(row);
            value
        }).collect::<Vec<_>>(),
        // Paycom has these people that day but Cortex had no route for them, as for office
        // staff: no meal break was expected in Cortex.
        "withoutRoute": staying.iter().map(|row| whom(row)).collect::<Vec<_>>(),
    }))
}

/// `GET /api/v1/dvic`: inspections in a period.
pub fn dvic(db: &Store, state: &State, caller: &Caller, query: &Value) -> Answer {
    catalog::check("dvic", query)?;
    let dsp = pick_dsp(caller, param(query, "dsp"))?;
    if !Sources::of(db, &dsp.id)?.dvic {
        return Err(not_on("DVIC").into());
    }
    let people = People::load(db, state, &dsp.id)?;
    let named = param(query, "driver");
    let person = if named.is_empty() {
        None
    } else {
        Some(people.find(named)?)
    };
    let period = period(query, today(dsp), "last 7 days")?;
    let (found, coverage) =
        facts::inspections(db, dsp, &period, person.map(|p| p.amazon.as_slice()))?;
    let short = flag(query, "short");
    Ok(json!({
        "dsp": about(dsp),
        "period": spanned(&period),
        "driver": person.map(person_json),
        "inspections": found.iter().filter(|i| !short || i.short).map(|i| {
            let mut row = json!(i);
            row["driver"] = who(&people, DriverSource::Amazon, &i.transporter_id, &i.driver_name);
            row
        }).collect::<Vec<_>>(),
        "coverage": coverage,
    }))
}
