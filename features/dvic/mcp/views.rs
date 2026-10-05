//! DVIC's answer to agents: each driver's inspections in a period, the short ones counted.
use super::facts::{Inspection, InspectionQuery};
use dispatch_core::{
    State,
    db::{Store, n, s},
    mcp::{
        Caller,
        api::types::DriverSource,
        data::{
            Answer,
            access::Access,
            catalog::{self},
            scope::{DEFAULT_PERIOD, People, label, param, period, today, who},
            shape::{Table, limit, offset, page, paged, understood},
        },
    },
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// `GET /api/v1/dvic`: inspections in a period, per driver, the short ones counted.
pub fn dvic(db: &Store, state: &State, caller: &Caller, query: &Value) -> Answer {
    catalog::check("dvic", query)?;
    let access = Access::of(db, caller, query)?;
    let dsp = access.dsp;
    let people = People::load(db, state, &access)?;
    let named = param(query, "driver");
    let person = if named.is_empty() {
        None
    } else {
        Some(people.find(named)?)
    };
    let period = period(query, today(dsp), DEFAULT_PERIOD)?;
    let scope = InspectionQuery::new(db, dsp, &period, person.map(|p| p.amazon.as_slice()))?;
    let count = scope.count()?;
    let coverage = &scope.coverage;
    let mut head = understood(dsp, Some(&period));
    if let Some(person) = person {
        head.insert("driver".into(), json!(label(&people, person)));
    }
    let mut answer = json!({
        "understood": head,
        "inspections": coverage.known().then_some(count),
        "short": coverage.known().then_some(count),
        "coverage": coverage,
    });
    people.mark(&mut answer);
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
        let start = offset(query)?;
        let take = limit(query, 200);
        for i in &scope.list(start, Some(take))? {
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
        page(&mut answer, "list", table, start, count, take)?;
        return Ok(answer);
    }
    // driver → (inspections, short, shortest seconds)
    let mut per: BTreeMap<String, (i64, i64, i64)> = BTreeMap::new();
    for row in scope.groups()? {
        let driver = who(
            &people,
            DriverSource::Amazon,
            s(&row, "transporter_id"),
            s(&row, "transporter_name"),
        );
        let entry = per.entry(driver).or_insert((0, 0, i64::MAX));
        entry.0 += n(&row, "inspections");
        entry.1 += n(&row, "inspections");
        entry.2 = entry.2.min(n(&row, "shortest"));
    }
    let mut rows: Vec<(String, (i64, i64, i64))> = per.into_iter().collect();
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
