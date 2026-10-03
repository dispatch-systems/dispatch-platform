//! The answers core gives: who is asking, how fresh each source is, what the metrics mean,
//! and the questions every feature's facts are joined for. Each is shaped to be read
//! cheaply: it opens with what it understood (the DSP, its date today, the days and any
//! driver), gives the figure the question is about, then a table whose column names appear
//! once, one page at a time. Every row of detail waits for a request that asks for it.
use super::{
    Answer, Refusal,
    access::{self, Access, Read},
    catalog::{self, METRICS, Metric, flag},
    facts::{Daily, Facts},
    scope::{
        DEFAULT_PERIOD, DriverGroup, People, Period, Person, daily_limit, driver_group, param,
        period, today,
    },
    shape::{Table, hours, paged, understood},
};
use crate::{
    State,
    agents::Caller,
    contracts::{AgentArea, Dsp},
    db::Store,
    manifest::registry,
};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::LazyLock,
};

fn driver_json(person: &Person) -> Value {
    json!({"code": person.code, "name": person.name})
}
/// `GET /api/v1/status`: which sources are on, which the key or app reads, and how fresh
/// each it reads is, each feature's in the registry's order.
pub fn status(db: &Store, caller: &Caller, query: &Value) -> Answer {
    catalog::check("status", query)?;
    let access = Access::of(db, caller, query)?;
    let dsp = access.dsp;
    let station = db.profile(&dsp.id)?.station_code;
    let mut sources = Map::new();
    for source in registry().features.iter().flat_map(|f| f.mcp.sources) {
        let fresh = source.fresh(db, dsp, &station)?;
        let reads = access.reads_from(*source);
        let mut value = json!({"enabled": access.on(*source), "reads": reads});
        if reads && let Some(found) = fresh.as_ref().and_then(Value::as_object) {
            for (k, v) in found {
                value[k] = v.clone();
            }
        }
        sources.insert(source.key().into(), value);
    }
    Ok(json!({
        "understood": understood(dsp, None),
        "sources": sources
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
    let access = Access::of(db, caller, query)?;
    let dsp = access.dsp;
    let people = People::load(db, state, &access)?;
    let wanted = param(query, "q").to_lowercase();
    let include_ids = flag(query, "include_ids");
    let columns: &[&str] = if include_ids {
        &["code", "name", "match", "paycom", "amazon"]
    } else {
        &["code", "name", "match"]
    };
    let mut table = Table::new(columns);
    for p in people.list.iter().filter(|p| {
        wanted.is_empty()
            || p.name.to_lowercase().contains(&wanted)
            || p.code.to_lowercase() == wanted
            || p.paycom
                .iter()
                .chain(&p.amazon)
                .any(|id| id.to_lowercase() == wanted)
    }) {
        let mut row = vec![json!(p.code), json!(p.name), json!(p.status)];
        if include_ids {
            row.extend([json!(p.paycom.join(", ")), json!(p.amazon.join(", "))]);
        }
        table.push(row);
    }
    let mut answer = json!({"understood": understood(dsp, None), "found": table.rows.len()});
    people.mark(&mut answer);
    paged(&mut answer, "drivers", table, query, 100)?;
    Ok(answer)
}

/// Every kind of facts a driver's days and the team's table are made of, in the order of
/// the kinds of data they are read as.
fn daily() -> &'static [&'static dyn Daily] {
    static DAILY: LazyLock<Vec<&'static dyn Daily>> = LazyLock::new(|| {
        let mut all: Vec<&'static dyn Daily> = registry()
            .features
            .iter()
            .flat_map(|f| f.mcp.daily)
            .copied()
            .collect();
        all.sort_by_key(|kind| kind.area().order());
        all
    });
    &DAILY
}
/// What each kind holds for the period, all of it or one person's: those of `wanted`, the
/// kinds of data the answer reads, and nothing of the rest.
fn gather(
    db: &Store,
    dsp: &Dsp,
    period: &Period,
    people: &People,
    person: Option<&Person>,
    wanted: &[AgentArea],
) -> crate::Result<Vec<Box<dyn Facts>>> {
    daily()
        .iter()
        .map(|kind| {
            if wanted.contains(&kind.area()) {
                kind.gather(db, dsp, period, people, person)
            } else {
                Ok(kind.none())
            }
        })
        .collect()
}
fn coverage(gathered: &[Box<dyn Facts>]) -> Value {
    let mut out = Map::new();
    for (kind, facts) in daily().iter().zip(gathered) {
        out.insert(kind.coverage().into(), json!(facts.coverage()));
    }
    Value::Object(out)
}

/// `GET /api/v1/drivers/{driver}`: one driver's days, one line each, with totals.
pub fn driver(db: &Store, state: &State, caller: &Caller, wanted: &str, query: &Value) -> Answer {
    catalog::check("driver", query)?;
    let access = Access::of(db, caller, query)?;
    let dsp = access.dsp;
    let people = People::load(db, state, &access)?;
    let person = people.find(wanted)?;
    let period = period(query, today(dsp), DEFAULT_PERIOD)?;
    daily_limit(&period)?;
    let kinds = daily();
    let read: Vec<AgentArea> = kinds
        .iter()
        .map(|kind| kind.area())
        .filter(|area| access.reads(*area))
        .collect();
    let gathered = gather(db, dsp, &period, &people, Some(person), &read)?;
    let mut head = understood(dsp, Some(&period));
    head.insert("driver".into(), driver_json(person));
    // A source not read is unknown, never zero: its figures are null, and it is named as not
    // allowed or switched off. One read by bypassing its feature is named too.
    let mut totals = Map::new();
    for (kind, facts) in kinds.iter().zip(&gathered) {
        for (name, value) in facts.totals() {
            let known = if read.contains(&kind.area()) {
                value
            } else {
                Value::Null
            };
            totals.insert(name.into(), known);
        }
    }
    let mut answer = json!({
        "understood": head,
        "totals": totals,
        "coverage": coverage(&gathered),
    });
    let named = |how: Read, name: fn(AgentArea) -> &'static str| -> Vec<&str> {
        kinds
            .iter()
            .map(|kind| kind.area())
            .filter(|area| access.read(*area) == how)
            .map(name)
            .collect()
    };
    let switch = |area: AgentArea| area.source().switch();
    for (key, names) in [
        ("not_allowed", named(Read::NotAllowed, AgentArea::label)),
        ("switched_off", named(Read::Off, switch)),
        ("bypassed", named(Read::Bypassed, switch)),
    ] {
        if !names.is_empty() {
            answer[key] = json!(names);
        }
    }
    people.mark(&mut answer);
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
        for (kind, facts) in kinds.iter().zip(&gathered) {
            for record in 0..facts.count() {
                let (date, ..) = facts.whose(record);
                push(date, kind.records(), facts.record(record));
            }
        }
        let mut table = Table::new(&["date", "records"]);
        for (date, records) in days {
            table.push(vec![json!(date), Value::Object(records)]);
        }
        paged(&mut answer, "days", table, query, 31)?;
        return Ok(answer);
    }
    // One line a day: each kind's records of the day that make a line.
    let mut lines: BTreeMap<&str, Vec<Vec<usize>>> = BTreeMap::new();
    for (index, facts) in gathered.iter().enumerate() {
        for record in (0..facts.count()).filter(|record| facts.lined(*record)) {
            let (date, ..) = facts.whose(record);
            lines
                .entry(date)
                .or_insert_with(|| vec![vec![]; kinds.len()])[index]
                .push(record);
        }
    }
    let mut columns = vec!["date"];
    columns.extend(kinds.iter().flat_map(|kind| kind.columns()));
    let mut table = Table::new(&columns);
    for (date, records) in lines {
        let mut row = vec![json!(date)];
        for (facts, records) in gathered.iter().zip(&records) {
            row.extend(facts.line(records));
        }
        table.push(row);
    }
    paged(&mut answer, "days", table, query, 100)?;
    Ok(answer)
}

/// One driver's (or one driver-day's) records as `team` adds them up: each kind's, by their
/// place in what the kind gathered.
struct Tally {
    label: String,
    records: Vec<Vec<usize>>,
}
/// A metric's value over a tally's records of its kind of data: null when it has none.
fn value(gathered: &[Box<dyn Facts>], records: &[Vec<usize>], metric: &Metric) -> Value {
    daily()
        .iter()
        .position(|kind| kind.area() == metric.area)
        .filter(|&index| !records[index].is_empty())
        .and_then(|index| gathered[index].metric(metric.name, &records[index]))
        .unwrap_or(Value::Null)
}

/// The tally a source's record adds to: one per person, or per person per day.
fn tally<'a>(
    tallies: &'a mut BTreeMap<(String, String), Tally>,
    per_day: bool,
    date: &str,
    driver: DriverGroup,
) -> &'a mut Tally {
    let day = if per_day {
        date.to_owned()
    } else {
        String::new()
    };
    tallies.entry((day, driver.key)).or_insert_with(|| Tally {
        label: driver.label,
        records: vec![vec![]; daily().len()],
    })
}

/// Public names for internally distinct rows. A repeated unmatched name gets a stable ordinal
/// based on its private source identity; the identity itself never appears in the answer.
fn display_names(tallies: &BTreeMap<(String, String), Tally>) -> HashMap<String, String> {
    let mut identities: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for ((_, identity), tally) in tallies {
        identities
            .entry(tally.label.clone())
            .or_default()
            .insert(identity.clone());
    }
    let mut names = HashMap::new();
    for (label, group) in identities {
        let repeated = group.len() > 1;
        for (index, identity) in group.into_iter().enumerate() {
            let name = if !repeated {
                label.clone()
            } else if let Some(name) = label.strip_suffix(" (unmatched)") {
                format!("{name} (unmatched {})", index + 1)
            } else {
                format!("{label} ({})", index + 1)
            };
            names.insert(identity, name);
        }
    }
    names
}

/// `GET /api/v1/team`: chosen metrics for every driver, per driver or per driver per day.
pub fn team(db: &Store, state: &State, caller: &Caller, query: &Value) -> Answer {
    catalog::check("team", query)?;
    let access = Access::of(db, caller, query)?;
    let dsp = access.dsp;
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
    let kinds = daily();
    let areas: Vec<AgentArea> = kinds
        .iter()
        .map(|kind| kind.area())
        .filter(|area| chosen.iter().any(|m| m.area == *area))
        .collect();
    if kinds
        .iter()
        .any(|kind| kind.assessed() && areas.contains(&kind.area()))
    {
        daily_limit(&period)?;
    }
    // A metric whose data can't be read here is refused; one read by bypassing its feature
    // is answered, and the feature named.
    let mut bypassed = vec![];
    for area in &areas {
        if access.check(*area)? == Read::Bypassed {
            bypassed.push(area.source());
        }
    }
    let people = People::load(db, state, &access)?;
    let gathered = gather(db, dsp, &period, &people, None, &areas)?;
    // One tally per private identity, or per private identity per day. Display labels are not
    // identities: two unmatched source records may have the same or no name.
    let mut tallies: BTreeMap<(String, String), Tally> = BTreeMap::new();
    for (index, facts) in gathered.iter().enumerate() {
        for record in 0..facts.count() {
            let (date, source, id, name) = facts.whose(record);
            let driver = driver_group(&people, source, id, name);
            tally(&mut tallies, per_day, date, driver).records[index].push(record);
        }
    }
    let display_names = display_names(&tallies);
    let mut rows: Vec<(String, String, Vec<Value>)> = tallies
        .into_iter()
        .map(|((date, identity), t)| {
            let values = chosen
                .iter()
                .map(|m| value(&gathered, &t.records, m))
                .collect();
            let name = display_names
                .get(&identity)
                .cloned()
                .expect("every tally has a display name");
            (date, name, values)
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
        // A total one unknown makes unknown is its kind's, over every record.
        let kind = kinds.iter().position(|kind| kind.area() == metric.area);
        if let Some(index) = kind.filter(|&index| kinds[index].pooled().contains(&metric.name)) {
            let mut every = vec![vec![]; kinds.len()];
            every[index] = (0..gathered[index].count()).collect();
            totals.insert(metric.name.into(), value(&gathered, &every, metric));
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
    for source in bypassed {
        access::bypassed(&mut answer, source);
    }
    people.mark(&mut answer);
    paged(&mut answer, "rows", table, query, 100)?;
    Ok(answer)
}
