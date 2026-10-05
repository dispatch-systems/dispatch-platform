//! A driver's report restricted to requested metrics and their source reads.
use super::{
    Answer, Refusal,
    access::{Access, Read},
    catalog::{self, METRICS, Metric},
    facts::Facts,
    scope::{People, Period, Person, daily_limit, param},
    shape::{Table, paged, understood},
    views::{daily, driver_json},
};
use crate::{db::Store, mcp::api::types::AgentArea};
use serde_json::{Map, Value, json};

pub fn driver(
    db: &Store,
    access: &Access<'_>,
    query: &Value,
    people: &People,
    person: &Person,
    period: &Period,
) -> Answer {
    let mut chosen: Vec<&Metric> = vec![];
    for name in param(query, "metrics")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let metric = catalog::metric(name).ok_or_else(|| {
            Refusal::new(
                400,
                "unknown_metric",
                format!("There is no metric `{name}`; list_metrics explains each one."),
            )
            .choices(METRICS.iter().map(|m| m.name.to_owned()).collect())
        })?;
        if !chosen.iter().any(|m| m.name == name) {
            chosen.push(metric);
        }
    }
    if chosen.is_empty() {
        return Err(Refusal::new(
            400,
            "unknown_metric",
            "Give at least one metric name; list_metrics explains each one.",
        )
        .into());
    }
    let kinds: Vec<_> = daily()
        .iter()
        .copied()
        .filter(|kind| chosen.iter().any(|m| m.area == kind.area()))
        .collect();
    if kinds.iter().any(|kind| kind.assessed()) {
        daily_limit(period)?;
    }
    let gathered: Vec<Box<dyn Facts>> = kinds
        .iter()
        .map(|kind| {
            if access.reads(kind.area()) {
                kind.gather(db, access.dsp, period, people, Some(person))
            } else {
                Ok(kind.none())
            }
        })
        .collect::<crate::Result<_>>()?;
    let mut coverage = Map::new();
    for (kind, facts) in kinds.iter().zip(&gathered) {
        coverage.insert(kind.coverage().into(), json!(facts.coverage()));
    }
    let metric_value = |metric: &Metric, date: Option<&str>| {
        let i = kinds
            .iter()
            .position(|kind| kind.area() == metric.area)
            .unwrap();
        let facts = &gathered[i];
        let known = access.reads(metric.area)
            && match date {
                Some(day) => facts.coverage().days.iter().any(|held| held == day),
                None => facts.coverage().known() && metric.total != "day",
            };
        if !known {
            return Value::Null;
        }
        let records: Vec<usize> = (0..facts.count())
            .filter(|r| facts.lined(*r) && date.is_none_or(|day| facts.whose(*r).0 == day))
            .collect();
        facts.metric(metric.name, &records).unwrap_or(Value::Null)
    };
    let totals: Map<String, Value> = chosen
        .iter()
        .map(|m| (m.name.into(), metric_value(m, None)))
        .collect();
    let mut head = understood(access.dsp, Some(period));
    head.insert("driver".into(), driver_json(person));
    let mut answer = json!({"understood": head, "totals": totals, "coverage": coverage});
    for (key, read) in [
        ("not_allowed", Read::NotAllowed),
        ("switched_off", Read::Off),
        ("bypassed", Read::Bypassed),
    ] {
        let names: Vec<&str> = kinds
            .iter()
            .map(|kind| kind.area())
            .filter(|area| access.read(*area) == read)
            .map(|area| {
                if read == Read::NotAllowed {
                    AgentArea::label(area)
                } else {
                    area.source().switch()
                }
            })
            .collect();
        if !names.is_empty() {
            answer[key] = json!(names);
        }
    }
    people.mark(&mut answer);
    if param(query, "detail") == "full" {
        let mut table = Table::new(&["date", "records"]);
        for date in period.days() {
            let mut records = Map::new();
            for (kind, facts) in kinds.iter().zip(&gathered) {
                let day: Vec<Value> = (0..facts.count())
                    .filter(|r| facts.whose(*r).0 == date)
                    .map(|r| facts.record(r))
                    .collect();
                if !day.is_empty() {
                    records.insert(kind.records().into(), json!(day));
                }
            }
            if !records.is_empty() {
                table.push(vec![json!(date), json!(records)]);
            }
        }
        paged(&mut answer, "days", table, query, 31)?;
    } else {
        let mut columns = vec!["date"];
        columns.extend(chosen.iter().map(|m| m.name));
        let mut table = Table::new(&columns);
        for date in period.days() {
            let mut row = vec![json!(date)];
            row.extend(chosen.iter().map(|m| metric_value(m, Some(&date))));
            table.push(row);
        }
        paged(&mut answer, "days", table, query, 100)?;
    }
    Ok(answer)
}
