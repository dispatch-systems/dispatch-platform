//! DVIC's answer to agents: each driver's inspections in a period, the short ones counted.
use super::facts::{Inspection, inspections};
use dispatch_core::{
    State,
    db::Store,
    mcp::{
        Caller,
        api::types::DriverSource,
        data::{
            Answer,
            access::Access,
            catalog::{self, flag},
            scope::{DEFAULT_PERIOD, People, label, param, period, today, who},
            shape::{Table, paged, understood},
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
    let (found, coverage) = inspections(db, dsp, &period, person.map(|p| p.amazon.as_slice()))?;
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
