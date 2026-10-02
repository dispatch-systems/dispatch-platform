//! Each endpoint's answer, shaped to be read cheaply. An answer opens with what it
//! understood (the DSP, its date today, the days and any driver), gives the figure the
//! question is about, then a table whose column names appear once, one page at a time.
//! Every row of detail waits for a request that asks for it.
use super::{
    Answer, Refusal,
    catalog::{self, METRICS, Metric, Source, flag},
    facts::{
        self, Coverage, Inspection, MealDay, Packages, RouteDay, Sources, TimecardDay, clock,
        outcome_of, reason_of,
    },
    scope::{DEFAULT_PERIOD, People, Period, Person, daily_limit, param, period, pick_dsp, today},
    shape::{BUDGET, Table, hours, offset_named, page, page_named, paged, understood},
};
use crate::{
    State,
    agents::Caller,
    contracts::{DriverSource, Dsp, MealStatus, RouteAddress},
    db::Store,
};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashMap};

/// A person as answers name them: their name, with their code when another has it too.
fn label(people: &People, person: &Person) -> String {
    if people.list.iter().filter(|p| p.name == person.name).count() > 1 {
        format!("{} ({})", person.name, person.code)
    } else {
        person.name.clone()
    }
}
/// Who a source's row belongs to: their Driver Match person, or the name and ID the source
/// gave while nobody holds that ID yet.
fn who(people: &People, source: DriverSource, id: &str, name: &str) -> String {
    match people.holder(source, id) {
        Some(person) => label(people, person),
        None => format!("{name} ({id})"),
    }
}
fn driver_json(person: &Person) -> Value {
    json!({"code": person.code, "name": person.name, "paycom": person.paycom, "amazon": person.amazon})
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
fn address_line(address: &RouteAddress) -> String {
    [
        &address.address1,
        &address.city,
        &address.state,
        &address.postal_code,
    ]
    .into_iter()
    .flatten()
    .filter(|v| !v.is_empty())
    .cloned()
    .collect::<Vec<_>>()
    .join(", ")
}
fn locations_off() -> Refusal {
    Refusal::new(
        403,
        "locations_off",
        "This key is not allowed addresses; ask without them.",
    )
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
        "understood": understood(dsp, None),
        "sources": {
            "timecards": source(on.timecards, "timecards"),
            "mealBreaks": source(on.meal_breaks, "mealBreaks"),
            "routes": source(on.routes, "routes"),
            "dvic": source(on.dvic, "dvic"),
            "scorecard": source(on.scorecard, "scorecard"),
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
    let mut table = Table::new(&["code", "name", "match", "paycom", "amazon"]);
    for p in people.list.iter().filter(|p| {
        wanted.is_empty()
            || p.name.to_lowercase().contains(&wanted)
            || p.code.to_lowercase() == wanted
            || p.paycom
                .iter()
                .chain(&p.amazon)
                .any(|id| id.to_lowercase() == wanted)
    }) {
        table.push(vec![
            json!(p.code),
            json!(p.name),
            json!(p.status),
            json!(p.paycom.join(", ")),
            json!(p.amazon.join(", ")),
        ]);
    }
    let mut answer = json!({"understood": understood(dsp, None), "found": table.rows.len()});
    paged(&mut answer, "drivers", table, query, 100)?;
    Ok(answer)
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
        let sources = person.map(|p| {
            p.paycom
                .iter()
                .map(|c| format!("paycom:{c}"))
                .chain(p.amazon.iter().map(|id| format!("cortex:{id}")))
                .collect::<Vec<_>>()
        });
        let (mut rows, coverage) = facts::meal_breaks_for(db, dsp, period, sources.as_deref())?;
        mark_routes(db, dsp, period, people, &mut rows)?;
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
fn meal_span(meal: &MealDay) -> (Value, Value) {
    let spans: Vec<String> = meal
        .meals
        .iter()
        .map(|m| {
            format!(
                "{}–{}",
                m.start.as_deref().unwrap_or("?"),
                m.end.as_deref().unwrap_or("?")
            )
        })
        .collect();
    let minutes = facts::total_minutes(meal.meals.iter().map(|m| m.minutes));
    (
        if spans.is_empty() {
            Value::Null
        } else {
            json!(spans.join(", "))
        },
        json!(minutes),
    )
}

/// One day of a driver's report, as one line.
#[derive(Default)]
struct Line {
    routes: i64,
    route: Vec<String>,
    stops: i64,
    delivered: i64,
    undeliverable: i64,
    hours: Option<f64>,
    clock_in: Option<String>,
    clock_out: Option<String>,
    meal: Option<MealStatus>,
    inspections: i64,
    short: i64,
}

/// `GET /api/v1/drivers/{driver}`: one driver's days, one line each, with totals.
pub fn driver(db: &Store, state: &State, caller: &Caller, wanted: &str, query: &Value) -> Answer {
    catalog::check("driver", query)?;
    let dsp = pick_dsp(caller, param(query, "dsp"))?;
    let people = People::load(db, state, &dsp.id)?;
    let person = people.find(wanted)?;
    let period = period(query, today(dsp), DEFAULT_PERIOD)?;
    daily_limit(&period)?;
    let gathered = gather(db, dsp, &period, &people, Some(person), ALL)?;
    let routes = &gathered.routes.0;
    let sum = |f: fn(&RouteDay) -> i64| routes.iter().map(f).sum::<i64>();
    let worked: f64 = gathered.timecards.0.iter().map(|c| c.hours).sum();
    let mut head = understood(dsp, Some(&period));
    head.insert("driver".into(), driver_json(person));
    // A source switched off is unknown, never zero: its figures are null and it is named.
    let on = Sources::of(db, &dsp.id)?;
    let known = |source: Source, value: Value| if on.has(source) { value } else { Value::Null };
    let mut answer = json!({
        "understood": head,
        "totals": {
            "routes": known(Source::Routes, json!(routes.len())),
            "stops_completed": known(Source::Routes, json!(sum(|r| r.stops_completed))),
            "packages_delivered": known(Source::Routes, json!(sum(|r| r.packages_delivered))),
            "packages_undeliverable":
                known(Source::Routes, json!(sum(|r| r.packages_undeliverable))),
            "hours_worked": known(Source::Timecards, hours(worked)),
            "days_worked": known(
                Source::Timecards,
                json!(gathered.timecards.0.iter().filter(|c| c.hours > 0.0).count()),
            ),
            "meal_issues": known(
                Source::MealBreaks,
                json!(gathered.meals.0.iter().filter(|m| meal_issue(m)).count()),
            ),
            "inspections": known(Source::Dvic, json!(gathered.inspections.0.len())),
            "short_inspections": known(
                Source::Dvic,
                json!(gathered.inspections.0.iter().filter(|i| i.short).count()),
            ),
        },
        "coverage": coverage(&gathered),
    });
    let off: Vec<&str> = [
        Source::Routes,
        Source::Timecards,
        Source::MealBreaks,
        Source::Dvic,
    ]
    .into_iter()
    .filter(|s| !on.has(*s))
    .map(Source::switch)
    .collect();
    if !off.is_empty() {
        answer["switched_off"] = json!(off);
    }
    if param(query, "detail") == "full" {
        // Every record the sources hold, day by day.
        let mut days: BTreeMap<String, Map<String, Value>> = BTreeMap::new();
        let mut push = |date: &str, key: &str, value: Value| {
            let day = days.entry(date.to_owned()).or_default();
            match day.get_mut(key).and_then(Value::as_array_mut) {
                Some(list) => list.push(value),
                None => {
                    day.insert(key.into(), json!([value]));
                }
            }
        };
        for route in routes {
            push(&route.date, "routes", json!(route));
        }
        for card in &gathered.timecards.0 {
            push(&card.date, "timecards", json!(card));
        }
        for meal in &gathered.meals.0 {
            push(&meal.date, "mealBreaks", json!(meal));
        }
        for inspection in &gathered.inspections.0 {
            push(&inspection.date, "inspections", json!(inspection));
        }
        let mut table = Table::new(&["date", "records"]);
        for (date, records) in days {
            table.push(vec![json!(date), Value::Object(records)]);
        }
        paged(&mut answer, "days", table, query, 31)?;
        return Ok(answer);
    }
    let mut lines: BTreeMap<&str, Line> = BTreeMap::new();
    for route in routes {
        let line = lines.entry(&route.date).or_default();
        line.routes += 1;
        line.route.extend(route.route.clone());
        line.stops += route.stops_completed;
        line.delivered += route.packages_delivered;
        line.undeliverable += route.packages_undeliverable;
    }
    for card in &gathered.timecards.0 {
        let line = lines.entry(&card.date).or_default();
        line.hours = Some(line.hours.unwrap_or(0.0) + card.hours);
        line.clock_in = line.clock_in.take().or(card.clock_in.clone());
        line.clock_out = card.clock_out.clone().or(line.clock_out.take());
    }
    for meal in gathered.meals.0.iter().filter(|m| m.cortex_route) {
        lines.entry(&meal.date).or_default().meal = Some(meal.status);
    }
    for inspection in &gathered.inspections.0 {
        let line = lines.entry(&inspection.date).or_default();
        line.inspections += 1;
        line.short += i64::from(inspection.short);
    }
    let mut table = Table::new(&[
        "date",
        "route",
        "stops",
        "delivered",
        "undeliverable",
        "hours",
        "in",
        "out",
        "meal",
        "inspections",
        "short",
    ]);
    for (date, line) in lines {
        let routed = line.routes > 0;
        let some = |value: i64| if routed { json!(value) } else { Value::Null };
        table.push(vec![
            json!(date),
            if routed {
                json!(line.route.join(", "))
            } else {
                Value::Null
            },
            some(line.stops),
            some(line.delivered),
            some(line.undeliverable),
            line.hours.map(hours).unwrap_or(Value::Null),
            json!(line.clock_in),
            json!(line.clock_out),
            json!(line.meal),
            json!(line.inspections),
            json!(line.short),
        ]);
    }
    paged(&mut answer, "days", table, query, 100)?;
    Ok(answer)
}

/// One driver's (or one driver-day's) numbers as `team` adds them up.
#[derive(Default)]
struct Tally {
    routes: Vec<RouteDay>,
    timecards: Vec<TimecardDay>,
    meals: Vec<MealDay>,
    inspections: Vec<Inspection>,
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
            "hours_worked" => cards.map(|c| hours(c.iter().map(|c| c.hours).sum())),
            "days_worked" => cards.map(|c| json!(c.iter().filter(|c| c.hours > 0.0).count())),
            "lunch_minutes" => {
                cards.map(|c| json!(facts::total_minutes(c.iter().map(|c| c.lunch_minutes))))
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

/// The tally a source's record adds to: one per person, or per person per day.
fn tally<'a>(
    tallies: &'a mut BTreeMap<(String, String), Tally>,
    per_day: bool,
    date: &str,
    whom: String,
) -> &'a mut Tally {
    let day = if per_day {
        date.to_owned()
    } else {
        String::new()
    };
    tallies.entry((day, whom)).or_default()
}

/// `GET /api/v1/team`: chosen metrics for every driver, per driver or per driver per day.
pub fn team(db: &Store, state: &State, caller: &Caller, query: &Value) -> Answer {
    catalog::check("team", query)?;
    let dsp = pick_dsp(caller, param(query, "dsp"))?;
    let period = period(query, today(dsp), DEFAULT_PERIOD)?;
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
    let mut chosen: Vec<&'static Metric> = vec![];
    for name in &names {
        let Some(metric) = catalog::metric(name) else {
            return Err(Refusal::new(
                400,
                "unknown_metric",
                format!("There is no metric `{name}`; list_metrics explains each one."),
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
    let sort = param(query, "sort");
    let by = if sort.is_empty() {
        chosen.first().map_or("name", |m| m.name)
    } else {
        sort
    };
    let by_column = chosen.iter().position(|m| m.name == by);
    let lowest = param(query, "order") == "lowest";
    if by != "name" && by_column.is_none() {
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
    let sources: Vec<&str> = chosen.iter().map(|m| m.source).collect();
    if sources.contains(&"timecards") || sources.contains(&"meal_breaks") {
        daily_limit(&period)?;
    }
    let on = Sources::of(db, &dsp.id)?;
    for (name, source) in [
        ("routes", Source::Routes),
        ("timecards", Source::Timecards),
        ("meal_breaks", Source::MealBreaks),
        ("dvic", Source::Dvic),
    ] {
        if sources.contains(&name) && !on.has(source) {
            return Err(facts::switched_off(dsp, source).into());
        }
    }
    let people = People::load(db, state, &dsp.id)?;
    let gathered = gather(db, dsp, &period, &people, None, &sources)?;
    // One tally per person, or per person per day, keyed by how answers name them.
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
    let mut rows: Vec<(String, String, Vec<Value>)> = tallies
        .into_iter()
        .map(|((date, whom), t)| {
            let values = chosen.iter().map(|m| t.value(m)).collect();
            (date, whom, values)
        })
        .collect();
    rows.sort_by(|a, b| {
        let name = |row: &(String, String, Vec<Value>)| row.1.to_lowercase();
        let Some(column) = by_column else {
            return name(a).cmp(&name(b)).then_with(|| a.0.cmp(&b.0));
        };
        // Highest first unless asked otherwise; a driver with nothing for it goes last.
        let value = |row: &(String, String, Vec<Value>)| row.2[column].as_f64();
        match (value(a), value(b)) {
            (Some(x), Some(y)) if lowest => x.total_cmp(&y),
            (Some(x), Some(y)) => y.total_cmp(&x),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        }
        .then_with(|| name(a).cmp(&name(b)))
    });
    // The whole team's figure for each metric that adds up, so nobody adds the rows.
    let mut totals = Map::new();
    for (k, metric) in chosen.iter().enumerate().filter(|(_, m)| m.total != "day") {
        if metric.name == "lunch_minutes" {
            totals.insert(
                metric.name.into(),
                json!(facts::total_minutes(
                    gathered.timecards.0.iter().map(|card| card.lunch_minutes)
                )),
            );
            continue;
        }
        let values: Vec<f64> = rows.iter().filter_map(|r| r.2[k].as_f64()).collect();
        let sum: f64 = values.iter().sum();
        let value = if values.is_empty() {
            Value::Null
        } else if values.iter().all(|v| v.fract() == 0.0) {
            json!(sum as i64)
        } else {
            hours(sum)
        };
        totals.insert(metric.name.into(), value);
    }
    let mut columns: Vec<&'static str> = vec![];
    if per_day {
        columns.push("date");
    }
    columns.push("driver");
    columns.extend(chosen.iter().map(|m| m.name));
    let mut table = Table::new(&columns);
    for (date, whom, values) in rows {
        let mut row = vec![];
        if per_day {
            row.push(json!(date));
        }
        row.push(json!(whom));
        row.extend(values);
        table.push(row);
    }
    let mut answer = json!({
        "understood": understood(dsp, Some(&period)),
        "sorted": if by == "name" {
            "by name".to_owned()
        } else {
            format!("by {by}, {} first", if lowest { "lowest" } else { "highest" })
        },
        "totals": totals,
        "coverage": coverage(&gathered),
    });
    paged(&mut answer, "rows", table, query, 100)?;
    Ok(answer)
}

/// `GET /api/v1/routes`: one day's routes, one line each.
pub fn routes(db: &Store, state: &State, caller: &Caller, query: &Value) -> Answer {
    catalog::check("routes", query)?;
    let dsp = pick_dsp(caller, param(query, "dsp"))?;
    let period = period(query, today(dsp), "yesterday")?;
    one_day(&period)?;
    let people = People::load(db, state, &dsp.id)?;
    let (found, coverage) = facts::routes(db, dsp, &period, None)?;
    let collected = !coverage.days.is_empty();
    let sum = |f: fn(&RouteDay) -> i64| found.iter().map(f).sum::<i64>();
    let mut table = Table::new(&[
        "route",
        "driver",
        "packages",
        "delivered",
        "undeliverable",
        "stops",
        "completed",
        "departed",
        "ended",
    ]);
    for route in &found {
        table.push(vec![
            json!(route.route),
            json!(who(
                &people,
                DriverSource::Amazon,
                &route.transporter_id,
                &route.driver_name
            )),
            json!(route.packages_total),
            json!(route.packages_delivered),
            json!(route.packages_undeliverable),
            json!(route.stops_total),
            json!(route.stops_completed),
            json!(route.departed),
            json!(route.ended),
        ]);
    }
    let mut answer = json!({
        "understood": understood(dsp, Some(&period)),
        "final": coverage.snapshots.is_empty() && collected,
        // A day not collected has no totals: nothing is known, which is not zero.
        "totals": collected.then(|| json!({
            "routes": found.len(),
            "packages": sum(|r| r.packages_total),
            "delivered": sum(|r| r.packages_delivered),
            "undeliverable": sum(|r| r.packages_undeliverable),
            "stops_completed": sum(|r| r.stops_completed),
        })),
    });
    if !collected {
        answer["note"] = json!(format!(
            "No routes have been collected for {}; their numbers are unknown, not zero.",
            period.first()
        ));
        return Ok(answer);
    }
    paged(&mut answer, "routes", table, query, 100)?;
    Ok(answer)
}

/// `GET /api/v1/routes/{route}`: what happened on one route, its problems first.
pub fn route(db: &Store, state: &State, caller: &Caller, wanted: &str, query: &Value) -> Answer {
    catalog::check("route", query)?;
    let dsp = pick_dsp(caller, param(query, "dsp"))?;
    let period = period(query, today(dsp), "yesterday")?;
    one_day(&period)?;
    let day = period.first();
    let found = facts::find_itinerary(db, dsp, &day, wanted)?;
    let itinerary = match found.as_slice() {
        [(id, ..)] => id.clone(),
        [] => {
            let (routes, _) = facts::routes(db, dsp, &period, None)?;
            return Err(Refusal::new(
                404,
                "route_not_found",
                format!("No route {wanted} on {day}; route_day lists the day's routes."),
            )
            .choices(routes.iter().filter_map(|r| r.route.clone()).collect())
            .into());
        }
        many => {
            return Err(Refusal::new(
                400,
                "route_ambiguous",
                format!("{wanted} names several itineraries on {day}; give one."),
            )
            .choices(
                many.iter()
                    .map(|(id, route, driver)| format!("{id} ({route}, {driver})"))
                    .collect(),
            )
            .into());
        }
    };
    let Some(detail) = db.route_itinerary(&dsp.id, &day, &itinerary)? else {
        return Err(Refusal::new(
            404,
            "route_not_found",
            format!("No route {wanted} on {day}."),
        )
        .into());
    };
    let people = People::load(db, state, &dsp.id)?;
    let zone = facts::zone(dsp);
    let full = param(query, "detail") == "full";
    let mut columns = vec!["stop", "tracking", "outcome", "reason", "at"];
    if caller.locations {
        columns.push("address");
    }
    let mut table = Table::new(&columns);
    let mut outcomes: BTreeMap<&str, i64> = BTreeMap::new();
    let mut reasons: BTreeMap<String, i64> = BTreeMap::new();
    let mut stops = 0;
    for stop in &detail.stops {
        let mut dropped = false;
        for task in stop
            .tasks
            .iter()
            .filter(|t| t.task_type.as_deref() == Some("DROP_OFF"))
        {
            dropped = true;
            let outcome = outcome_of(task.task_state.as_deref());
            let reason = reason_of(task.state_context.as_deref());
            *outcomes.entry(outcome).or_default() += 1;
            if outcome != "delivered" && !reason.is_empty() {
                *reasons.entry(reason.clone()).or_default() += 1;
            }
            if full || outcome != "delivered" {
                let mut row = vec![
                    json!(stop.sequence),
                    json!(task.tracking_id),
                    json!(outcome),
                    json!(reason),
                    json!(clock(task.executed_at, zone)),
                ];
                if caller.locations {
                    row.push(json!(stop.address.as_ref().map(address_line)));
                }
                table.push(row);
            }
        }
        stops += i64::from(dropped);
    }
    let it = &detail.itinerary;
    let mut answer = json!({
        "understood": understood(dsp, Some(&period)),
        "route": it.route_code,
        "driver": who(&people, DriverSource::Amazon, &it.transporter_id, &it.driver_name),
        "final": detail.publication.mode == "final",
        "stops": stops,
        "packages": outcomes.values().sum::<i64>(),
        "outcomes": outcomes,
        "reasons": reasons,
    });
    let key = if full {
        "packages_list"
    } else {
        "not_delivered"
    };
    paged(&mut answer, key, table, query, 200)?;
    Ok(answer)
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
    let mut columns = vec!["date", "route", "driver", "outcome", "reason", "at"];
    if caller.locations {
        columns.push("address");
    }
    let mut table = Table::new(&columns);
    for e in &found.events {
        let mut row = vec![
            json!(e.day),
            json!(e.route_code),
            json!(who(
                &people,
                DriverSource::Amazon,
                &e.transporter_id,
                e.driver_name.as_deref().unwrap_or("")
            )),
            json!(outcome_of(e.task.task_state.as_deref())),
            json!(reason_of(e.task.state_context.as_deref())),
            json!(clock(e.task.executed_at, zone)),
        ];
        if caller.locations {
            row.push(json!(e.address.as_ref().map(address_line)));
        }
        table.push(row);
    }
    let mut answer = json!({"understood": understood(dsp, None), "tracking": found.tracking_id});
    paged(&mut answer, "events", table, query, 50)?;
    Ok(answer)
}

/// `GET /api/v1/packages`: how many packages a question covers, grouped as asked, and the
/// packages themselves only when asked.
pub fn packages(db: &Store, state: &State, caller: &Caller, query: &Value) -> Answer {
    catalog::check("packages", query)?;
    let dsp = pick_dsp(caller, param(query, "dsp"))?;
    let period = period(query, today(dsp), DEFAULT_PERIOD)?;
    let people = People::load(db, state, &dsp.id)?;
    let named = param(query, "driver");
    let person = if named.is_empty() {
        None
    } else {
        Some(people.find(named)?)
    };
    let outcome = Some(param(query, "outcome")).filter(|o| !o.is_empty());
    // Reasons as people write them: "Business Closed" is business_closed.
    let reason = param(query, "reason")
        .trim()
        .to_lowercase()
        .replace([' ', '-'], "_");
    let reason = Some(reason).filter(|r| !r.is_empty());
    if let Some(wanted) = &reason {
        let known = facts::package_reasons(db, dsp)?;
        if !known.contains(wanted) {
            return Err(Refusal::new(
                400,
                "unknown_reason",
                format!("Amazon has never given the reason `{wanted}` here."),
            )
            .choices(known)
            .into());
        }
    }
    let groups: Vec<&str> = param(query, "group_by")
        .split(',')
        .map(str::trim)
        .filter(|g| !g.is_empty())
        .collect();
    if groups.len() > 2 || groups.iter().any(|g| facts::package_group(g).is_none()) {
        return Err(Refusal::new(
            400,
            "invalid_group_by",
            "Group by one or two of driver, day, outcome, reason, route and address.",
        )
        .choices(
            ["driver", "day", "outcome", "reason", "route", "address"]
                .map(str::to_owned)
                .to_vec(),
        )
        .into());
    }
    let list = flag(query, "list");
    if !param(query, "groups_cursor").is_empty() && (!list || groups.is_empty()) {
        return Err(Refusal::new(
            400,
            "invalid_parameter",
            "Use `groups_cursor` when requesting both `group_by` and `list=true`.",
        )
        .into());
    }
    if groups.contains(&"address") && !caller.locations {
        return Err(locations_off().into());
    }
    let wanted = Packages {
        drivers: person.map(|p| p.amazon.as_slice()),
        outcome,
        reason: reason.as_deref(),
        route: Some(param(query, "route")).filter(|r| !r.is_empty()),
    };
    let (total, grouped) = facts::package_counts(db, dsp, &period, &wanted, &groups)?;
    let mut head = understood(dsp, Some(&period));
    if let Some(person) = person {
        head.insert("driver".into(), json!(label(&people, person)));
    }
    for (key, value) in [
        ("outcome", outcome),
        ("reason", reason.as_deref()),
        ("route", wanted.route),
    ] {
        if let Some(value) = value {
            head.insert(key.into(), json!(value));
        }
    }
    let mut answer = json!({
        "understood": head,
        "packages": total,
        "coverage": facts::route_coverage(db, dsp, &period)?,
    });
    // Nothing with both: say what that reason did come with, so a guessed outcome is fixed.
    if total == 0
        && let (Some(asked), Some(_)) = (outcome, &reason)
    {
        let (_, seen) = facts::package_counts(
            db,
            dsp,
            &period,
            &Packages {
                drivers: wanted.drivers,
                outcome: None,
                reason: wanted.reason,
                route: wanted.route,
            },
            &["outcome"],
        )?;
        if !seen.is_empty() {
            let found: Vec<String> = seen
                .iter()
                .map(|(keys, n)| format!("{n} {}", keys[0]))
                .collect();
            answer["note"] = json!(format!(
                "None were {asked}. With that reason there were: {}.",
                found.join(", ")
            ));
        }
    }
    if !groups.is_empty() {
        // A driver may hold several transporter IDs: their counts add up under one name.
        let addresses = if groups.contains(&"address") {
            let ids: Vec<String> = grouped
                .iter()
                .flat_map(|(keys, _)| {
                    groups
                        .iter()
                        .zip(keys)
                        .filter(|(g, _)| **g == "address")
                        .map(|(_, k)| k.clone())
                })
                .collect();
            facts::addresses(db, dsp, &ids)?
        } else {
            HashMap::new()
        };
        let mut merged: Vec<(Vec<String>, i64)> = vec![];
        let mut index: HashMap<Vec<String>, usize> = HashMap::new();
        for (keys, count) in grouped {
            let keys: Vec<String> = groups
                .iter()
                .zip(keys)
                .map(|(g, k)| match *g {
                    "driver" => people
                        .holder(DriverSource::Amazon, &k)
                        .map(|person| label(&people, person))
                        .unwrap_or(k),
                    "address" => addresses.get(&k).cloned().unwrap_or(k),
                    _ => k,
                })
                .collect();
            match index.get(&keys) {
                Some(&at) => merged[at].1 += count,
                None => {
                    index.insert(keys.clone(), merged.len());
                    merged.push((keys, count));
                }
            }
        }
        merged.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let mut columns: Vec<&'static str> = groups
            .iter()
            .map(|g| match *g {
                "driver" => "driver",
                "day" => "day",
                "outcome" => "outcome",
                "reason" => "reason",
                "route" => "route",
                _ => "address",
            })
            .collect();
        columns.push("packages");
        let mut table = Table::new(&columns);
        for (keys, count) in merged {
            let mut row: Vec<Value> = keys.into_iter().map(Value::from).collect();
            row.push(json!(count));
            table.push(row);
        }
        // Each table can advance independently when both are requested.
        if list {
            let total = table.rows.len();
            let offset = offset_named(query, "groups_cursor")?;
            table.rows.drain(..offset.min(total));
            // Leave room for package rows when both independently pageable tables are requested.
            table = table.with_budget(BUDGET.saturating_sub(answer.to_string().len()) / 2);
            page_named(
                &mut answer,
                "groups",
                table,
                offset,
                total,
                100,
                "groups_cursor",
            )?;
        } else {
            paged(&mut answer, "groups", table, query, 100)?;
        }
    }
    if list {
        let offset = super::shape::offset(query)?;
        let limit = super::shape::limit(query, 100);
        let rows = facts::package_rows(db, dsp, &period, &wanted, offset, limit)?;
        let mut columns = vec![
            "date", "tracking", "driver", "route", "outcome", "reason", "at",
        ];
        if caller.locations {
            columns.push("address");
        }
        let addresses = if caller.locations {
            let ids: Vec<String> = rows.iter().map(|r| r.address_id.clone()).collect();
            facts::addresses(db, dsp, &ids)?
        } else {
            HashMap::new()
        };
        let mut table = Table::new(&columns);
        for r in rows {
            let mut row = vec![
                json!(r.day),
                json!(r.tracking),
                json!(who(
                    &people,
                    DriverSource::Amazon,
                    &r.transporter_id,
                    &r.driver_name
                )),
                json!(r.route),
                json!(r.outcome),
                json!(r.reason),
                json!(r.at),
            ];
            if caller.locations {
                row.push(json!(addresses.get(&r.address_id)));
            }
            table.push(row);
        }
        page(&mut answer, "list", table, offset, total as usize, limit)?;
    }
    Ok(answer)
}

/// `GET /api/v1/timecards`: everyone's for a day, or one driver's for a period.
pub fn timecards(db: &Store, state: &State, caller: &Caller, query: &Value) -> Answer {
    catalog::check("timecards", query)?;
    let dsp = pick_dsp(caller, param(query, "dsp"))?;
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
            DEFAULT_PERIOD
        } else {
            "yesterday"
        },
    )?;
    if person.is_none() {
        one_day(&period)?;
    }
    daily_limit(&period)?;
    let (cards, coverage) =
        facts::timecards(db, dsp, &period, person.map(|p| p.paycom.as_slice()))?;
    let first = if person.is_some() { "date" } else { "driver" };
    let mut table = Table::new(&[first, "hours", "in", "out", "lunch_minutes", "needs_review"]);
    for card in &cards {
        table.push(vec![
            if person.is_some() {
                json!(card.date)
            } else {
                json!(who(
                    &people,
                    DriverSource::Paycom,
                    &card.employee_code,
                    &card.name
                ))
            },
            hours(card.hours),
            json!(card.clock_in),
            json!(card.clock_out),
            json!(card.lunch_minutes),
            json!(card.needs_review),
        ]);
    }
    let mut head = understood(dsp, Some(&period));
    if let Some(person) = person {
        head.insert("driver".into(), json!(label(&people, person)));
    }
    let mut answer = json!({
        "understood": head,
        "hours": hours(cards.iter().map(|c| c.hours).sum()),
        "coverage": coverage,
    });
    paged(&mut answer, "timecards", table, query, 100)?;
    Ok(answer)
}

/// `GET /api/v1/meal-breaks`: one day's comparison for the drivers Cortex had a route for.
pub fn meal_breaks(db: &Store, state: &State, caller: &Caller, query: &Value) -> Answer {
    catalog::check("meal_breaks", query)?;
    let dsp = pick_dsp(caller, param(query, "dsp"))?;
    let period = period(query, today(dsp), "yesterday")?;
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
    let mut table = Table::new(&[
        "driver",
        "status",
        "meal",
        "minutes",
        "late_clock_in",
        "long_gap",
    ]);
    for row in rows
        .iter()
        .filter(|r| r.cortex_route && (!issues || meal_issue(r)))
    {
        let (span, minutes) = meal_span(row);
        table.push(vec![
            json!(whom(row)),
            json!(row.status),
            span,
            minutes,
            json!(row.late_clock_in),
            json!(row.long_gap),
        ]);
    }
    // Only those Cortex had a route for: office staff with lunch punches are no meal
    // question, and listing them apart read to models as missed meals.
    let mut answer = json!({
        "understood": understood(dsp, Some(&period)),
        "collected": !coverage.days.is_empty(),
    });
    paged(&mut answer, "drivers", table, query, 100)?;
    Ok(answer)
}

/// `GET /api/v1/dvic`: inspections in a period, per driver, the short ones counted.
pub fn dvic(db: &Store, state: &State, caller: &Caller, query: &Value) -> Answer {
    catalog::check("dvic", query)?;
    let dsp = pick_dsp(caller, param(query, "dsp"))?;
    let people = People::load(db, state, &dsp.id)?;
    let named = param(query, "driver");
    let person = if named.is_empty() {
        None
    } else {
        Some(people.find(named)?)
    };
    let period = period(query, today(dsp), DEFAULT_PERIOD)?;
    let (found, coverage) =
        facts::inspections(db, dsp, &period, person.map(|p| p.amazon.as_slice()))?;
    let short = flag(query, "short");
    let mut head = understood(dsp, Some(&period));
    if let Some(person) = person {
        head.insert("driver".into(), json!(label(&people, person)));
    }
    let mut answer = json!({
        "understood": head,
        "inspections": found.len(),
        "short": found.iter().filter(|i| i.short).count(),
        "coverage": coverage,
    });
    let whom = |i: &Inspection| {
        who(
            &people,
            DriverSource::Amazon,
            &i.transporter_id,
            &i.driver_name,
        )
    };
    if param(query, "detail") == "full" {
        let mut table = Table::new(&[
            "date", "driver", "type", "started", "seconds", "minimum", "short",
        ]);
        for i in found.iter().filter(|i| !short || i.short) {
            table.push(vec![
                json!(i.date),
                json!(whom(i)),
                json!(i.inspection_type),
                json!(i.started),
                json!(i.seconds),
                json!(i.minimum_seconds),
                json!(i.short),
            ]);
        }
        paged(&mut answer, "list", table, query, 200)?;
        return Ok(answer);
    }
    // driver → (inspections, short, shortest seconds)
    let mut per: BTreeMap<String, (i64, i64, i64)> = BTreeMap::new();
    for i in &found {
        let entry = per.entry(whom(i)).or_insert((0, 0, i64::MAX));
        entry.0 += 1;
        entry.1 += i64::from(i.short);
        entry.2 = entry.2.min(i.seconds);
    }
    let mut rows: Vec<(String, (i64, i64, i64))> =
        per.into_iter().filter(|(_, t)| !short || t.1 > 0).collect();
    rows.sort_by(|a, b| b.1.1.cmp(&a.1.1).then_with(|| a.0.cmp(&b.0)));
    let mut table = Table::new(&["driver", "inspections", "short", "shortest_seconds"]);
    for (driver, (count, shorts, shortest)) in rows {
        table.push(vec![
            json!(driver),
            json!(count),
            json!(shorts),
            json!(shortest),
        ]);
    }
    paged(&mut answer, "drivers", table, query, 100)?;
    Ok(answer)
}
