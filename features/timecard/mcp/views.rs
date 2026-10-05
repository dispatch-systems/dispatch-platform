//! Timecard's answers to agents: a day's timecards or one driver's, and a day's meal
//! breaks beside Paycom's lunches.
use super::facts::{self, MealDay, mark_routes, meal_issue, meal_span};
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
            scope::{DEFAULT_PERIOD, People, daily_limit, label, param, period, today, who},
            shape::{Table, hours, paged, understood},
        },
    },
};
use serde_json::{Value, json};

/// `GET /api/v1/timecards`: everyone's for a day, or one driver's for a period.
pub fn timecards(db: &Store, state: &State, caller: &Caller, query: &Value) -> Answer {
    catalog::check("timecards", query)?;
    let access = Access::of(db, caller, query)?;
    let dsp = access.dsp;
    let people = People::load(db, state, &access)?;
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
    daily_limit(&period)?;
    let (cards, coverage) =
        facts::timecards(db, dsp, &period, person.map(|p| p.paycom.as_slice()))?;
    let first = if person.is_some() { "date" } else { "driver" };
    let mut table = Table::new(&[first, "hours", "in", "out", "lunch_minutes", "needs_review"]);
    let multiple = person.is_none() && period.from != period.to;
    if multiple {
        table.columns.insert(0, "date");
    }
    for card in &cards {
        let mut row = vec![
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
        ];
        if multiple {
            row.insert(0, json!(card.date));
        }
        table.push(row);
    }
    let mut head = understood(dsp, Some(&period));
    if let Some(person) = person {
        head.insert("driver".into(), json!(label(&people, person)));
    }
    let mut answer = json!({
        "understood": head,
        "hours": coverage.known().then(|| hours(cards.iter().map(|c| c.hours).sum())),
        "coverage": coverage,
    });
    people.mark(&mut answer);
    paged(&mut answer, "timecards", table, query, 100)?;
    Ok(answer)
}

/// `GET /api/v1/meal-breaks`: one day's comparison for the drivers Cortex had a route for.
pub fn meal_breaks(db: &Store, state: &State, caller: &Caller, query: &Value) -> Answer {
    catalog::check("meal_breaks", query)?;
    let access = Access::of(db, caller, query)?;
    let dsp = access.dsp;
    let period = period(query, today(dsp), "yesterday")?;
    daily_limit(&period)?;
    let people = People::load(db, state, &access)?;
    let person = match param(query, "driver") {
        "" => None,
        named => Some(people.find(named)?),
    };
    let sources = person.map(|p| {
        p.paycom
            .iter()
            .map(|code| format!("paycom:{code}"))
            .chain(p.amazon.iter().map(|id| format!("cortex:{id}")))
            .collect::<Vec<_>>()
    });
    let (mut rows, coverage) = facts::meal_breaks_for(db, dsp, &period, sources.as_deref())?;
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
    let multiple = period.from != period.to;
    if multiple {
        table.columns.insert(0, "date");
    }
    for row in rows
        .iter()
        .filter(|r| r.cortex_route && (!issues || meal_issue(r)))
    {
        let (span, minutes) = meal_span(row);
        let mut values = vec![
            json!(whom(row)),
            json!(row.status),
            span,
            minutes,
            json!(row.late_clock_in),
            json!(row.long_gap),
        ];
        if multiple {
            values.insert(0, json!(row.date));
        }
        table.push(values);
    }
    // Only those Cortex had a route for: office staff with lunch punches are no meal
    // question, and listing them apart read to models as missed meals.
    let mut head = understood(dsp, Some(&period));
    if let Some(person) = person {
        head.insert("driver".into(), json!(label(&people, person)));
    }
    let mut answer = json!({
        "understood": head,
        "collected": !coverage.days.is_empty(),
        "coverage": coverage,
    });
    people.mark(&mut answer);
    paged(&mut answer, "drivers", table, query, 100)?;
    Ok(answer)
}
