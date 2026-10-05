//! Questions about Amazon's weekly scorecard: customer feedback (CDF), Netradyne safety
//! events, returns to station with their contact-compliance notes, and each driver's tiers.
//! Each reads the week's active publication and answers a count or a short table, as the
//! route questions do.
use super::query::{self, Dataset};
use crate::backend::ScorecardStore;
use dispatch_core::{
    State,
    accounts::api::types::Dsp,
    db::{Store, n, s},
    foundation::weeks,
    mcp::{
        Caller,
        api::types::DriverSource,
        data::{
            Answer, Refusal,
            access::{self, Access, Read},
            catalog::{self, flag},
            facts,
            scope::{DEFAULT_PERIOD, People, Period, Person, param, period, today},
            shape::{BUDGET, Table, limit, offset, page, paged_named, understood},
        },
    },
};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// The kinds of feedback a CDF row flags, as answers name them, beside Amazon's field.
pub const FEEDBACK: &[(&str, &str, bool)] = &[
    ("wrong_address", "delivered_to_wrong_address", true),
    ("never_received", "never_received_delivery", true),
    ("mishandled", "driver_mishandled_package", true),
    ("unprofessional", "driver_was_unprofessional", true),
    (
        "not_preferred_location",
        "not_delivered_to_preferred_location",
        true,
    ),
    ("wrong_item", "received_wrong_item", true),
    ("above_and_beyond", "above_and_beyond", false),
    ("delivered_with_care", "delivered_with_care", false),
    ("friendly", "friendly", false),
    ("followed_instructions", "followed_instructions", false),
    ("respectful_of_property", "respectful_of_property", false),
    ("thank_my_driver", "thank_my_driver", false),
];
pub const FEEDBACK_NAMES: &[&str] = &[
    "wrong_address",
    "never_received",
    "mishandled",
    "unprofessional",
    "not_preferred_location",
    "wrong_item",
    "above_and_beyond",
    "delivered_with_care",
    "friendly",
    "followed_instructions",
    "respectful_of_property",
    "thank_my_driver",
];
/// Amazon's tiers, best first, old names beside the new.
pub(super) fn tier_rank(tier: &str) -> Option<u8> {
    Some(match tier.to_lowercase().as_str() {
        "platinum" | "fantastic plus" | "fantastic" => 4,
        "gold" | "great" => 3,
        "silver" | "fair" => 2,
        "bronze" | "poor" => 1,
        _ => return None,
    })
}

fn who(people: &People, id: &str, name: &str) -> String {
    match people.holder(DriverSource::Amazon, id) {
        Some(person) => {
            if people.list.iter().filter(|p| p.name == person.name).count() > 1 {
                format!("{} ({})", person.name, person.code)
            } else {
                person.name.clone()
            }
        }
        None if name.is_empty() => id.to_owned(),
        None => format!("{name} ({id})"),
    }
}
/// How a code Amazon writes in capitals reads in an answer: "BUSINESS CLOSED" is
/// business_closed.
pub(super) fn snake(text: &str) -> String {
    text.trim()
        .to_lowercase()
        .replace(['-', ' ', '/'], "_")
        .split('_')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

/// One row of a scorecard dataset, from the week's active publication.
pub(super) struct Row {
    pub transporter_id: String,
    pub tracking_id: String,
    pub data: Value,
}
/// The Amazon weeks a period touches, and which of them have been posted.
fn weeks(db: &Store, dsp: &Dsp, period: &Period) -> dispatch_core::Result<Value> {
    let station = db.profile(&dsp.id)?.station_code;
    let mut wanted: BTreeSet<String> = BTreeSet::new();
    let mut day = period.from;
    while day <= period.to {
        let saturday = day
            + chrono::Duration::days(i64::from(
                6 - chrono::Datelike::weekday(&day).num_days_from_sunday(),
            ));
        let iso = chrono::Datelike::iso_week(&saturday);
        wanted.insert(format!("{}-W{:02}", iso.year(), iso.week()));
        day = saturday + chrono::Duration::days(1);
    }
    let posted: BTreeSet<String> = db
        .scorecard_db(&dsp.id)?
        .all(
            "SELECT week FROM scorecard_publications WHERE station=? AND active=1 AND scope_verified=1",
            [&station],
        )?
        .iter()
        .map(|r| s(r, "week").to_owned())
        .collect();
    // A week still running, or the one just ended, may have no scorecard yet: it is pending,
    // not missing.
    let completed = weeks::last_completed_week(today(dsp));
    let (pending, missing): (Vec<&String>, Vec<&String>) = wanted
        .iter()
        .filter(|w| !posted.contains(*w))
        .partition(|w| **w >= completed);
    let collected = wanted.len() - missing.len() - pending.len();
    let mut out = json!({"status": facts::coverage_status(collected, wanted.len()),
        "collected": collected, "of": wanted.len()});
    if !missing.is_empty() {
        out["missing"] = json!(missing);
    }
    if !pending.is_empty() {
        out["not_posted_yet"] = json!(pending);
    }
    Ok(json!({"weeks": out}))
}
/// The latest scorecard week collected.
pub fn fresh(db: &Store, dsp: &Dsp, station: &str) -> dispatch_core::Result<Option<Value>> {
    let scorecard = db.scorecard_db(&dsp.id)?.one(
        "SELECT max(week) week,max(collected_at) collected_at FROM scorecard_publications \
         WHERE station=? AND active=1 AND scope_verified=1",
        [station],
    )?;
    Ok(scorecard.filter(|r| !r["week"].is_null()).map(|r| {
        serde_json::json!({
            "latestWeek": r["week"], "collectedAt": r["collected_at"]
        })
    }))
}

/// A period no collected week touches has nothing to count. It is refused rather than
/// answered, so an agent cannot read the missing figures as zero and is sent where else the
/// question may be answered.
fn unknown(period: &Period, coverage: &Value, elsewhere: &str) -> Option<Refusal> {
    let weeks = &coverage["weeks"];
    if weeks["collected"].as_u64() != Some(0) {
        return None;
    }
    let label = &period.label;
    Some(if weeks.get("missing").is_none() {
        Refusal::new(
            404,
            "not_posted_yet",
            format!(
                "Amazon has not posted the scorecard for {label} yet: unknown, not zero.{elsewhere}"
            ),
        )
    } else {
        Refusal::new(
            404,
            "not_collected",
            format!("The scorecard for {label} was not collected: unknown, not zero.{elsewhere}"),
        )
    })
}
pub(super) fn text(row: &Value, field: &str) -> String {
    scalar_text(&row[field])
}
pub(super) fn scalar_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.trim().to_owned(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}
fn yes(row: &Value, field: &str) -> bool {
    matches!(
        text(row, field).to_lowercase().as_str(),
        "1" | "y" | "yes" | "true"
    )
}
/// The driver a question names, as the Amazon IDs they hold.
fn person_ids<'a>(people: &'a People, query: &Value) -> Result<Option<&'a Person>, Refusal> {
    let named = param(query, "driver");
    if named.is_empty() {
        Ok(None)
    } else {
        people.find(named).map(Some)
    }
}
/// Groups, largest first: each key's count, or a refusal naming the choices.
fn groups_of(query: &Value, allowed: &[&'static str]) -> Result<Vec<&'static str>, Refusal> {
    let mut out = vec![];
    for asked in param(query, "group_by")
        .split(',')
        .map(str::trim)
        .filter(|g| !g.is_empty())
    {
        match allowed.iter().find(|a| **a == asked) {
            Some(g) if out.len() < 2 => out.push(*g),
            _ => {
                return Err(Refusal::new(
                    400,
                    "invalid_group_by",
                    format!("Group by one or two of {}.", allowed.join(", ")),
                )
                .choices(allowed.iter().map(|a| a.to_string()).collect()));
            }
        }
    }
    Ok(out)
}
/// Group pages and detail pages can advance independently. Summary-only requests keep
/// their original cursor; when details are present groups use groups_cursor.
fn group_page(
    answer: &mut Value,
    table: Table,
    query: &Value,
    listed: bool,
) -> Result<(), Refusal> {
    let table = if listed {
        table.with_budget(BUDGET.saturating_sub(answer.to_string().len()) / 2)
    } else {
        table
    };
    let cursor = if listed { "groups_cursor" } else { "cursor" };
    paged_named(answer, "groups", table, query, 100, cursor)
}
fn group_expression(group: &str, date: &str) -> String {
    match group {
        "driver" => format!(
            "json_array(COALESCE(x.transporter_id,''),{})",
            query::field("da_name")
        ),
        "week" => "x.week".into(),
        "type" => query::normalized("type"),
        "reason" => query::normalized("rts_reason_code"),
        "coaching" => {
            let field = query::field("weekly_coaching");
            format!("CASE WHEN {field}='' THEN 'none' ELSE {field} END")
        }
        "address" => "COALESCE(x.tracking_id,'')".into(),
        _ if date == "delivery_time" => format!("substr({},1,10)", query::field(date)),
        _ => query::field(date),
    }
}
fn group_keys(people: &People, groups: &[&str], row: &Value) -> Vec<String> {
    groups
        .iter()
        .enumerate()
        .map(|(i, group)| {
            let key = format!("k{i}");
            if *group == "driver" {
                let pair: Value = serde_json::from_str(s(row, &key)).unwrap_or_default();
                who(
                    people,
                    pair[0].as_str().unwrap_or(""),
                    pair[1].as_str().unwrap_or(""),
                )
            } else {
                text(row, &key)
            }
        })
        .collect()
}
fn feedback_kinds(row: &Row) -> Vec<&'static str> {
    FEEDBACK
        .iter()
        .filter(|(_, field, _)| yes(&row.data, field))
        .map(|(name, _, _)| *name)
        .collect()
}
/// Every value a field takes in a dataset's collected weeks, as answers name it.
fn ever(
    db: &Store,
    dsp: &Dsp,
    table: &str,
    field: &str,
) -> dispatch_core::Result<BTreeSet<String>> {
    let station = db.profile(&dsp.id)?.station_code;
    Ok(db
        .scorecard_db(&dsp.id)?
        .all(
            &format!(
                "SELECT DISTINCT json_extract(x.row,'$.{field}') v FROM {table} x \
                 JOIN scorecard_publications p ON p.id=x.publication_id AND p.active=1 AND p.scope_verified=1 \
                 WHERE p.station=?"
            ),
            [&station],
        )?
        .iter()
        .map(|r| snake(&text(r, "v")))
        .filter(|v| !v.is_empty())
        .collect())
}
/// Safety events and those still counting per group, most counting first.
fn event_table(groups: &[&'static str], mut rows: Vec<(Vec<String>, (i64, i64))>) -> Table {
    rows.sort_by(|a, b| {
        (b.1.1, b.1.0)
            .cmp(&(a.1.1, a.1.0))
            .then_with(|| a.0.cmp(&b.0))
    });
    let mut columns = groups.to_vec();
    columns.extend(["events", "counting"]);
    let mut table = Table::new(&columns);
    for (keys, (events, counting)) in rows {
        let mut row: Vec<Value> = keys.into_iter().map(Value::from).collect();
        row.extend([json!(events), json!(counting)]);
        table.push(row);
    }
    table
}
/// Counts per group, largest first, as a table.
fn count_table(groups: &[&'static str], counted: HashMap<Vec<String>, i64>, extra: &str) -> Table {
    let mut rows: Vec<(Vec<String>, i64)> = counted.into_iter().collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let mut columns = groups.to_vec();
    columns.push(match extra {
        "feedback" => "feedback",
        "returns" => "returns",
        _ => "events",
    });
    let mut table = Table::new(&columns);
    for (keys, count) in rows {
        let mut row: Vec<Value> = keys.into_iter().map(Value::from).collect();
        row.push(json!(count));
        table.push(row);
    }
    table
}

/// `GET /api/v1/feedback`: customer delivery feedback (CDF), counted and grouped.
pub fn feedback(db: &Store, state: &State, caller: &Caller, query: &Value) -> Answer {
    catalog::check("feedback", query)?;
    let access = Access::of(db, caller, query)?;
    let dsp = access.dsp;
    let period = period(query, today(dsp), DEFAULT_PERIOD)?;
    let people = People::load(db, state, &access)?;
    let person = person_ids(&people, query)?;
    let wanted_type = param(query, "type");
    // A kind of praise is counted among the praise unless the question says otherwise.
    let praise = FEEDBACK
        .iter()
        .any(|(name, _, negative)| *name == wanted_type && !negative);
    let kind = match param(query, "feedback") {
        "" if praise => "positive",
        "" | "negative" => "negative",
        other => other,
    };
    // Addresses come from the stored routes, so only as the key or app reads those, and only
    // in a build with the feature that holds them.
    let places = facts::places();
    let groups = match places {
        Some(_) => groups_of(query, &["driver", "address", "type", "week", "day"])?,
        None => groups_of(query, &["driver", "type", "week", "day"])?,
    };
    if let Some(places) = places
        && groups.contains(&"address")
    {
        access.check(places.area)?;
    }
    let placed = places.is_some_and(|places| access.reads(places.area));
    let min = param(query, "min_count").parse::<i64>().unwrap_or(1);
    let impacting = flag(query, "impacting");
    let coverage = weeks(db, dsp, &period)?;
    if let Some(refusal) = unknown(&period, &coverage, "") {
        return Err(refusal.into());
    }
    let mut selected = Dataset::new(
        db,
        dsp,
        "customer_feedback",
        "delivery_time",
        &period,
        person.map(|p| p.amazon.as_slice()),
    )?;
    match kind {
        "negative" => selected.and(&query::yes("negative_feedback_flag"), None),
        "positive" => selected.and(
            &format!("NOT {}", query::yes("negative_feedback_flag")),
            None,
        ),
        _ => (),
    }
    if impacting {
        selected.and(&query::yes("cdf_impact_flag"), None);
    }
    if let Some((_, field, _)) = FEEDBACK.iter().find(|(name, _, _)| *name == wanted_type) {
        selected.and(&query::yes(field), None);
    }
    let totals = selected.totals(&[])?;
    let total = n(&totals, "total") as usize;
    let listed = flag(query, "list");
    let start = offset(query)?;
    let take = limit(query, 100);
    let kept = if listed {
        selected.list(start, take)?
    } else {
        vec![]
    };
    let mut tracking: Vec<String> = kept.iter().map(|r| r.tracking_id.clone()).collect();
    let mut address_counts = vec![];
    if groups.contains(&"address") {
        address_counts =
            selected.groups(&[group_expression("address", "delivery_time")], &[], "")?;
        tracking.extend(address_counts.iter().map(|r| s(r, "k0").to_owned()));
    }
    let looked_up = placed && (groups.contains(&"address") || listed);
    let addresses = match places {
        Some(places) if looked_up => (places.of)(db, dsp, &tracking)?,
        _ => HashMap::new(),
    };
    let mut head = understood(dsp, Some(&period));
    head.insert("feedback".into(), json!(kind));
    if let Some(p) = person {
        head.insert("driver".into(), json!(p.name));
    }
    if !wanted_type.is_empty() {
        head.insert("type".into(), json!(wanted_type));
    }
    let mut answer = json!({
        "understood": head,
        "feedback": total,
        "coverage": coverage,
    });
    people.mark(&mut answer);
    if let Some(places) = places
        && looked_up
        && access.read(places.area) == Read::Bypassed
    {
        access::bypassed(&mut answer, places.area.source());
    }
    if !groups.is_empty() {
        let mut counted: HashMap<Vec<String>, i64> = HashMap::new();
        let mut keys: Vec<String> = groups
            .iter()
            .map(|g| group_expression(g, "delivery_time"))
            .collect();
        let mut expansion = String::new();
        if let Some(index) = groups.iter().position(|g| *g == "type") {
            let flags: Vec<String> = FEEDBACK
                .iter()
                .map(|(name, field, _)| {
                    format!("CASE WHEN {} THEN '{name}' END", query::yes(field))
                })
                .collect();
            let none = FEEDBACK
                .iter()
                .map(|(_, field, _)| query::yes(field))
                .collect::<Vec<_>>()
                .join("+");
            expansion = format!(
                "JOIN json_each(json_array({},CASE WHEN ({none})=0 THEN 'unspecified' END)) kinds ON kinds.value IS NOT NULL",
                flags.join(",")
            );
            keys[index] = "kinds.value".into();
        }
        for row in selected.groups(&keys, &[], &expansion)? {
            let mut keys = group_keys(&people, &groups, &row);
            if let Some(index) = groups.iter().position(|g| *g == "address") {
                let Some(address) = addresses.get(&keys[index]) else {
                    continue;
                };
                keys[index] = address.clone();
            }
            *counted.entry(keys).or_default() += n(&row, "total");
        }
        counted.retain(|_, count| *count >= min);
        if groups.contains(&"address") {
            let unplaced: i64 = address_counts
                .iter()
                .filter(|r| !addresses.contains_key(s(r, "k0")))
                .map(|r| n(r, "total"))
                .sum();
            answer["unplaced"] = json!(unplaced);
            if unplaced > 0 {
                answer["note"] = json!(format!(
                    "{unplaced} feedback could not be placed at an address: their routes were not collected."
                ));
            }
        }
        group_page(
            &mut answer,
            count_table(&groups, counted, "feedback"),
            query,
            listed,
        )?;
    }
    if flag(query, "list") {
        let mut columns = vec!["date", "tracking", "driver", "types", "impacting"];
        if placed {
            columns.push("address");
        }
        let mut table = Table::new(&columns);
        for row in &kept {
            let kinds = feedback_kinds(row);
            let mut values = vec![
                json!(
                    text(&row.data, "delivery_time")
                        .chars()
                        .take(10)
                        .collect::<String>()
                ),
                json!(row.tracking_id),
                json!(who(
                    &people,
                    &row.transporter_id,
                    &text(&row.data, "da_name")
                )),
                json!(kinds.join(", ")),
                json!(yes(&row.data, "cdf_impact_flag")),
            ];
            if placed {
                values.push(json!(addresses.get(&row.tracking_id)));
            }
            table.push(values);
        }
        page(&mut answer, "list", table, start, total, take)?;
    }
    Ok(answer)
}

/// `GET /api/v1/safety`: Netradyne safety events, counted, grouped or listed.
pub fn safety(db: &Store, state: &State, caller: &Caller, query: &Value) -> Answer {
    catalog::check("safety", query)?;
    let access = Access::of(db, caller, query)?;
    let dsp = access.dsp;
    let period = period(query, today(dsp), DEFAULT_PERIOD)?;
    let people = People::load(db, state, &access)?;
    let person = person_ids(&people, query)?;
    let groups = groups_of(query, &["driver", "type", "day", "week"])?;
    let wanted_type = snake(param(query, "type"));
    let coverage = weeks(db, dsp, &period)?;
    if let Some(refusal) = unknown(&period, &coverage, "") {
        return Err(refusal.into());
    }
    if !wanted_type.is_empty() {
        let known = ever(db, dsp, "safety_events", "type")?;
        if !known.is_empty() && !known.iter().any(|k| k.contains(&wanted_type)) {
            return Err(Refusal::new(
                400,
                "unknown_type",
                format!("Amazon has recorded no events of type `{wanted_type}`."),
            )
            .choices(known.into_iter().collect())
            .into());
        }
    }
    let mut selected = Dataset::new(
        db,
        dsp,
        "safety_events",
        "data_date",
        &period,
        person.map(|p| p.amazon.as_slice()),
    )?;
    if !wanted_type.is_empty() {
        selected.and(
            &format!("instr({},?)>0", query::normalized("type")),
            Some(&wanted_type),
        );
    }
    let counting = format!(
        "{} <> 'dispute_approved'",
        query::normalized("final_resolution")
    );
    if !param(query, "counting").is_empty() {
        selected.and(
            &format!("({counting})=CAST(? AS INTEGER)"),
            Some(if flag(query, "counting") { "1" } else { "0" }),
        );
    }
    let totals = selected.totals(&[("counting", counting.clone())])?;
    let total = n(&totals, "total") as usize;
    let by_type = selected.groups(
        &[query::normalized("type")],
        &[("counting", counting.clone())],
        "",
    )?;
    let types = event_table(
        &["type"],
        by_type
            .iter()
            .map(|r| {
                (
                    vec![s(r, "k0").to_owned()],
                    (n(r, "total"), n(r, "counting")),
                )
            })
            .collect(),
    );
    let types = json!({"columns": types.columns, "rows": types.rows});
    let listed = person.is_some() || flag(query, "list");
    let start = offset(query)?;
    let take = limit(query, 100);
    let kept = if listed {
        selected.list(start, take)?
    } else {
        vec![]
    };
    let kind = |row: &Row| snake(&text(&row.data, "type"));
    let mut head = understood(dsp, Some(&period));
    if let Some(p) = person {
        head.insert("driver".into(), json!(p.name));
    }
    if !wanted_type.is_empty() {
        head.insert("type".into(), json!(wanted_type));
    }
    let mut answer = json!({
        "understood": head,
        "events": total,
        "counting": totals["counting"],
        "by_type": types,
        "coverage": coverage,
    });
    people.mark(&mut answer);
    if !groups.is_empty() {
        let keys: Vec<String> = groups
            .iter()
            .map(|g| group_expression(g, "data_date"))
            .collect();
        let mut counted: HashMap<Vec<String>, (i64, i64)> = HashMap::new();
        for row in selected.groups(&keys, &[("counting", counting)], "")? {
            let entry = counted
                .entry(group_keys(&people, &groups, &row))
                .or_default();
            entry.0 += n(&row, "total");
            entry.1 += n(&row, "counting");
        }
        group_page(
            &mut answer,
            event_table(&groups, counted.into_iter().collect()),
            query,
            listed,
        )?;
    }
    // One driver's events are few; anyone's are listed when asked.
    if person.is_some() || flag(query, "list") {
        let mut table = Table::new(&[
            "date",
            "time",
            "driver",
            "type",
            "subtype",
            "severity",
            "resolution",
        ]);
        for r in &kept {
            let started = text(&r.data, "event_start_time_local");
            table.push(vec![
                json!(text(&r.data, "data_date")),
                json!(started.get(11..16).unwrap_or("")),
                json!(who(&people, &r.transporter_id, &text(&r.data, "da_name"))),
                json!(kind(r)),
                json!(text(&r.data, "subtype")),
                json!(text(&r.data, "severity").to_lowercase()),
                json!(match text(&r.data, "final_resolution").as_str() {
                    "" | "None" => "none".to_owned(),
                    other => snake(other),
                }),
            ]);
        }
        page(&mut answer, "list", table, start, total, take)?;
    }
    Ok(answer)
}

/// `GET /api/v1/returns`: Amazon's returns to station (RTS), with contact compliance.
pub fn returns(db: &Store, state: &State, caller: &Caller, query: &Value) -> Answer {
    catalog::check("returns", query)?;
    let access = Access::of(db, caller, query)?;
    let dsp = access.dsp;
    let period = period(query, today(dsp), DEFAULT_PERIOD)?;
    let people = People::load(db, state, &access)?;
    let person = person_ids(&people, query)?;
    let groups = groups_of(query, &["driver", "reason", "coaching", "week", "day"])?;
    let contact = param(query, "contact");
    let reason = snake(param(query, "reason"));
    let impacting = flag(query, "impacting");
    let coverage = weeks(db, dsp, &period)?;
    if let Some(refusal) = unknown(
        &period,
        &coverage,
        " For packages brought back on these days, ask packages with outcome returned.",
    ) {
        return Err(refusal.into());
    }
    if !reason.is_empty() {
        let known = ever(db, dsp, "returns_to_station", "rts_reason_code")?;
        if !known.is_empty() && !known.contains(&reason) {
            return Err(Refusal::new(
                400,
                "unknown_reason",
                format!("Amazon has recorded no returns with reason `{reason}`."),
            )
            .choices(known.into_iter().collect())
            .into());
        }
    }
    let mut selected = Dataset::new(
        db,
        dsp,
        "returns_to_station",
        "delivery_planned_date",
        &period,
        person.map(|p| p.amazon.as_slice()),
    )?;
    if !reason.is_empty() {
        selected.and(
            &format!("{}=?", query::normalized("rts_reason_code")),
            Some(&reason),
        );
    }
    if impacting {
        selected.and(&query::yes("impacting_dcr"), None);
    }
    match contact {
        "missed" => selected.and(&query::missed(), None),
        "compliant" => selected.and(
            &format!(
                "{}='Contact Compliant'",
                query::field("weekly_exemption_reason")
            ),
            None,
        ),
        _ => (),
    }
    let totals = selected.totals(&[
        ("hurting_dcr", query::yes("impacting_dcr")),
        ("contact_missed", query::missed()),
    ])?;
    let total = n(&totals, "total") as usize;
    let listed = flag(query, "list");
    let start = offset(query)?;
    let take = limit(query, 100);
    let kept = if listed {
        selected.list(start, take)?
    } else {
        vec![]
    };
    let mut head = understood(dsp, Some(&period));
    if let Some(p) = person {
        head.insert("driver".into(), json!(p.name));
    }
    for (key, value) in [("contact", contact), ("reason", reason.as_str())] {
        if !value.is_empty() {
            head.insert(key.into(), json!(value));
        }
    }
    let mut answer = json!({
        "understood": head,
        "returns": total,
        "hurting_dcr": totals["hurting_dcr"],
        "contact_missed": totals["contact_missed"],
        "coverage": coverage,
    });
    people.mark(&mut answer);
    if !groups.is_empty() {
        let keys: Vec<String> = groups
            .iter()
            .map(|g| group_expression(g, "delivery_planned_date"))
            .collect();
        let mut counted: HashMap<Vec<String>, i64> = HashMap::new();
        for row in selected.groups(&keys, &[], "")? {
            *counted
                .entry(group_keys(&people, &groups, &row))
                .or_default() += n(&row, "total");
        }
        group_page(
            &mut answer,
            count_table(&groups, counted, "returns"),
            query,
            listed,
        )?;
    }
    if flag(query, "list") {
        let mut table = Table::new(&[
            "date",
            "tracking",
            "driver",
            "reason",
            "coaching",
            "hurts_dcr",
            "exemption",
        ]);
        for r in &kept {
            table.push(vec![
                json!(text(&r.data, "delivery_planned_date")),
                json!(r.tracking_id),
                json!(who(&people, &r.transporter_id, &text(&r.data, "da_name"))),
                json!(snake(&text(&r.data, "rts_reason_code"))),
                json!(text(&r.data, "weekly_coaching")),
                json!(yes(&r.data, "impacting_dcr")),
                json!(text(&r.data, "weekly_exemption_reason")),
            ]);
        }
        page(&mut answer, "list", table, start, total, take)?;
    }
    Ok(answer)
}

/// The tiers a weekly scorecard row carries, by the names answers use.
const TIERS: &[(&str, &str)] = &[
    ("overall", "da_overall_tier"),
    ("cdf", "cdf_dpmo_tier"),
    ("dsb", "dsb_tier"),
    ("pod", "pod_tier"),
    ("rts", "rts_tier"),
    ("speeding", "speeding_tier"),
    ("seatbelt", "seatbelt_tier"),
    ("distractions", "distractions_tier"),
    ("following_distance", "following_distance_tier"),
    ("signals", "sign_signal_violations_rate_tier"),
];

/// `GET /api/v1/scorecard`: one week's scorecard, the DSP's and each driver's tiers.
pub fn weekly(db: &Store, state: &State, caller: &Caller, query: &Value) -> Answer {
    catalog::check("scorecard", query)?;
    let access = Access::of(db, caller, query)?;
    let dsp = access.dsp;
    let station = db.profile(&dsp.id)?.station_code;
    let data = query::database(db, dsp)?;
    let posted: Vec<String> = data
        .all(
            "SELECT week FROM scorecard_publications WHERE station=? AND active=1 AND scope_verified=1 ORDER BY week DESC",
            [&station],
        )?
        .iter()
        .map(|r| s(r, "week").to_owned())
        .collect();
    let asked = param(query, "week");
    let week = match asked.to_lowercase().as_str() {
        "" | "latest" => match posted.first() {
            Some(week) => week.clone(),
            None => {
                return Ok(json!({
                    "understood": understood(dsp, None),
                    "posted": false,
                    "note": "No scorecard week has been collected yet.",
                }));
            }
        },
        "last week" => weeks::last_completed_week(today(dsp)),
        other => {
            let week = other.to_uppercase();
            weeks::parse_week(&week).map_err(|_| {
                Refusal::new(
                    400,
                    "invalid_week",
                    "Write a week as 2026-W39, last week or latest.",
                )
            })?;
            week
        }
    };
    let mut head = understood(dsp, None);
    head.insert("week".into(), json!(week));
    if let Ok((first, last)) = weeks::week_days(&week) {
        head.insert("days".into(), json!(format!("{first}..{last}")));
    }
    if !posted.contains(&week) {
        return Ok(json!({
            "understood": head,
            "posted": false,
            "note": format!("The scorecard for {week} has not been collected; it may not be posted yet."),
            "weeks_collected": posted.iter().take(8).collect::<Vec<_>>(),
        }));
    }
    let dsp_row = data
        .one(
            "SELECT x.row FROM dsp_quality x JOIN scorecard_publications p ON p.id=x.publication_id \
             AND p.active=1 AND p.scope_verified=1 WHERE p.station=? AND p.week=? LIMIT 1",
            [&station, &week],
        )?
        .and_then(|r| serde_json::from_str::<Value>(s(&r, "row")).ok())
        .unwrap_or_default();
    let mut summary = Map::new();
    for (name, field) in [
        ("tier", "dsp_final_tier"),
        ("score", "dsp_final_score"),
        ("dcr", "dcr_tier"),
        ("contact_compliance", "cc_tier"),
        ("dsb", "dsb_tier"),
        ("pod", "pod_tier"),
        ("rts", "rts_tier"),
    ] {
        if !dsp_row[field].is_null() {
            summary.insert(name.into(), dsp_row[field].clone());
        }
    }
    let focus: Vec<String> = ["focus_area_1", "focus_area_2", "focus_area_3"]
        .iter()
        .map(|f| text(&dsp_row, f))
        .filter(|f| !f.is_empty())
        .collect();
    if !focus.is_empty() {
        summary.insert("focus_areas".into(), json!(focus));
    }
    // Driver cards carry no contact-compliance tier, so point at where each miss is.
    if summary.contains_key("contact_compliance") {
        summary.insert(
            "contact_compliance_by_driver".into(),
            json!("in returns, with contact missed"),
        );
    }
    let people = People::load(db, state, &access)?;
    let person = person_ids(&people, query)?;
    let below = param(query, "below");
    let below_rank = if below.is_empty() {
        None
    } else {
        tier_rank(below)
    };
    if !below.is_empty() && below_rank.is_none() {
        return Err(Refusal::new(
            400,
            "invalid_tier",
            "Name a tier: platinum, gold, silver or bronze.",
        )
        .into());
    }
    let mut scope = String::from(
        " FROM driver_scorecards x JOIN scorecard_publications p ON p.id=x.publication_id \
         AND p.active=1 AND p.scope_verified=1 WHERE p.station=? AND p.week=?",
    );
    let mut params = vec![station, week];
    if let Some(person) = person {
        if person.amazon.is_empty() {
            scope.push_str(" AND 0");
        } else {
            scope.push_str(&format!(
                " AND x.transporter_id IN ({})",
                vec!["?"; person.amazon.len()].join(",")
            ));
            params.extend_from_slice(&person.amazon);
        }
        head.insert("driver".into(), json!(person.name));
    }
    let overall = query::field("da_overall_tier");
    let mut tiers: BTreeMap<String, i64> = BTreeMap::new();
    for row in data.all(
        &format!("SELECT {overall} tier,COUNT(*) drivers{scope} GROUP BY tier"),
        rusqlite::params_from_iter(&params),
    )? {
        tiers.insert(s(&row, "tier").to_owned(), n(&row, "drivers"));
    }
    if let Some(rank) = below_rank {
        scope.push_str(&format!(
            " AND agent_scorecard_rank({overall}) < CAST(? AS INTEGER)"
        ));
        params.push(rank.to_string());
    }
    let total = data.count(
        &format!("SELECT COUNT(*){scope}"),
        rusqlite::params_from_iter(&params),
    )? as usize;
    let start = offset(query)?;
    let take = limit(query, 100);
    let sql = format!(
        "SELECT COALESCE(x.transporter_id,'') transporter_id,x.row{scope} ORDER BY CASE WHEN \
         json_type(x.row,'$.da_overall_score') IN ('integer','real') THEN json_extract(x.row,'$.da_overall_score') ELSE \
         1.7976931348623157e308 END,x.row_index,x.publication_id LIMIT ? OFFSET ?"
    );
    params.extend([take.to_string(), start.min(i64::MAX as usize).to_string()]);
    let drivers = data.all(&sql, rusqlite::params_from_iter(&params))?;
    let mut columns = vec!["driver", "score"];
    columns.extend(TIERS.iter().map(|(name, _)| *name));
    columns.push("delivered");
    let mut table = Table::new(&columns);
    for source in drivers {
        let row: Value = serde_json::from_str(s(&source, "row")).unwrap_or_default();
        let mut values = vec![
            json!(who(
                &people,
                s(&source, "transporter_id"),
                &text(&row, "da_name")
            )),
            row["da_overall_score"].clone(),
        ];
        for (_, field) in TIERS {
            values.push(match text(&row, field).as_str() {
                "" | "None" => Value::Null,
                tier => json!(tier),
            });
        }
        values.push(row["delivered"].clone());
        table.push(values);
    }
    let mut answer = json!({
        "understood": head,
        "posted": true,
        "dsp": summary,
        "drivers_by_tier": tiers,
    });
    people.mark(&mut answer);
    page(&mut answer, "drivers", table, start, total, take)?;
    Ok(answer)
}
