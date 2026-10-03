//! Routes' answers to agents: a day's routes, one route's packages, one package, and
//! packages counted, grouped and listed.
use super::{
    LOCATIONS,
    facts::{self, Packages, RouteDay, outcome_of, reason_of},
};
use crate::{contracts::RouteAddress, routedata::RoutesStore};
use dispatch_core::{
    State,
    db::Store,
    mcp::{
        Caller,
        api::types::DriverSource,
        data::{
            Answer, Refusal,
            access::Access,
            catalog::{self, flag},
            facts::{clock, zone},
            scope::{DEFAULT_PERIOD, People, label, one_day, param, period, today, who},
            shape::{self, BUDGET, Table, offset_named, page, page_named, paged, understood},
        },
    },
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};

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
/// `GET /api/v1/routes`: one day's routes, one line each.
pub fn routes(db: &Store, state: &State, caller: &Caller, query: &Value) -> Answer {
    catalog::check("routes", query)?;
    let access = Access::of(db, caller, query)?;
    let dsp = access.dsp;
    let period = period(query, today(dsp), "yesterday")?;
    one_day(&period)?;
    let people = People::load(db, state, &access)?;
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
    people.mark(&mut answer);
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
    let access = Access::of(db, caller, query)?;
    let dsp = access.dsp;
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
    let people = People::load(db, state, &access)?;
    let zone = zone(dsp);
    let full = param(query, "detail") == "full";
    let places = access.reads(LOCATIONS);
    let mut columns = vec!["stop", "tracking", "outcome", "reason", "at"];
    if places {
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
                if places {
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
    people.mark(&mut answer);
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
    let access = Access::of(db, caller, query)?;
    let dsp = access.dsp;
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
    let people = People::load(db, state, &access)?;
    let zone = zone(dsp);
    let places = access.reads(LOCATIONS);
    let mut columns = vec!["date", "route", "driver", "outcome", "reason", "at"];
    if places {
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
        if places {
            row.push(json!(e.address.as_ref().map(address_line)));
        }
        table.push(row);
    }
    let mut answer = json!({"understood": understood(dsp, None), "tracking": found.tracking_id});
    people.mark(&mut answer);
    paged(&mut answer, "events", table, query, 50)?;
    Ok(answer)
}

/// `GET /api/v1/packages`: how many packages a question covers, grouped as asked, and the
/// packages themselves only when asked.
pub fn packages(db: &Store, state: &State, caller: &Caller, query: &Value) -> Answer {
    catalog::check("packages", query)?;
    let access = Access::of(db, caller, query)?;
    let dsp = access.dsp;
    let period = period(query, today(dsp), DEFAULT_PERIOD)?;
    let people = People::load(db, state, &access)?;
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
    if let Some(wanted) = &reason
        && let Some(known) = facts::unknown_package_reason_choices(db, dsp, wanted)?
    {
        return Err(Refusal::new(
            400,
            "unknown_reason",
            format!("Amazon has never given the reason `{wanted}` here."),
        )
        .choices(known)
        .into());
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
    if groups.contains(&"address") {
        access.check(LOCATIONS)?;
    }
    let places = access.reads(LOCATIONS);
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
    people.mark(&mut answer);
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
        let offset = shape::offset(query)?;
        let limit = shape::limit(query, 100);
        let rows = facts::package_rows(db, dsp, &period, &wanted, offset, limit)?;
        let mut columns = vec![
            "date", "tracking", "driver", "route", "outcome", "reason", "at",
        ];
        if places {
            columns.push("address");
        }
        let addresses = if places {
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
            if places {
                row.push(json!(addresses.get(&r.address_id)));
            }
            table.push(row);
        }
        page(&mut answer, "list", table, offset, total as usize, limit)?;
    }
    Ok(answer)
}
