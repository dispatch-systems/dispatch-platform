//! Questions about Amazon's weekly scorecard: customer feedback (CDF), Netradyne safety
//! events, returns to station with their contact-compliance notes, and each driver's tiers.
//! Each reads the week's active publication and answers a count or a short table, as the
//! route questions do.
use crate::backend::ScorecardStore;
use dispatch_core::{
    State,
    accounts::api::types::Dsp,
    db::{Store, s},
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
            shape::{Table, limit, page, paged, understood},
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
fn tier_rank(tier: &str) -> Option<u8> {
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
fn snake(text: &str) -> String {
    text.trim()
        .to_lowercase()
        .replace(['-', ' ', '/'], "_")
        .split('_')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

/// One row of a scorecard dataset, from the week's active publication.
struct Row {
    week: String,
    transporter_id: String,
    tracking_id: String,
    data: Value,
}
/// A dataset's rows whose own date falls in the period: `date` is the JSON field that
/// holds it, as "2026-09-14" or a time beginning with the date.
fn rows(
    db: &Store,
    dsp: &Dsp,
    table: &str,
    date: &str,
    period: &Period,
    drivers: Option<&[String]>,
) -> dispatch_core::Result<Vec<Row>> {
    let station = db.profile(&dsp.id)?.station_code;
    let data = db.scorecard_db(&dsp.id)?;
    let mut sql = format!(
        "SELECT x.week,COALESCE(x.transporter_id,'') transporter_id,COALESCE(x.tracking_id,'') tracking_id,x.row \
         FROM {table} x JOIN scorecard_publications p ON p.id=x.publication_id AND p.active=1 AND p.scope_verified=1 \
         WHERE p.station=? AND substr(json_extract(x.row,'$.{date}'),1,10) BETWEEN ? AND ?"
    );
    let mut params = vec![station, period.first(), period.last()];
    if let Some(ids) = drivers {
        sql.push_str(&format!(
            " AND x.transporter_id IN ({})",
            vec!["?"; ids.len().max(1)].join(",")
        ));
        params.extend(ids.iter().cloned());
        if ids.is_empty() {
            params.push(String::new());
        }
    }
    sql.push_str(" ORDER BY json_extract(x.row,'$.");
    sql.push_str(date);
    sql.push_str("') DESC, x.row_index");
    Ok(data
        .all(&sql, rusqlite::params_from_iter(&params))?
        .iter()
        .map(|r| Row {
            week: s(r, "week").into(),
            transporter_id: s(r, "transporter_id").into(),
            tracking_id: s(r, "tracking_id").into(),
            data: serde_json::from_str(s(r, "row")).unwrap_or_default(),
        })
        .collect())
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
    let mut out =
        json!({"collected": wanted.len() - missing.len() - pending.len(), "of": wanted.len()});
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
fn text(row: &Value, field: &str) -> String {
    match &row[field] {
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
/// A list beside groups shows only its first page, since the request's cursor pages the
/// groups: the rest are read by asking again without them.
fn first_page(answer: &mut Value, table: Table, query: &Value) -> Result<(), Refusal> {
    let total = table.rows.len();
    page(answer, "list", table, 0, total, limit(query, 100))?;
    let list = &mut answer["list"];
    if let Some(page) = list.get_mut("page").and_then(Value::as_object_mut) {
        page.remove("next_cursor");
        let shown = page["returned"].clone();
        list["note"] = json!(format!(
            "The first {shown} of {total}. Ask again without group_by to page through them all."
        ));
    }
    Ok(())
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
    let groups = groups_of(query, &["driver", "address", "type", "week", "day"])?;
    // Addresses come from the stored routes, so only as the key or app reads those.
    let places = facts::places();
    if groups.contains(&"address") {
        access.check(places.area)?;
    }
    let placed = access.reads(places.area);
    let min = param(query, "min_count").parse::<i64>().unwrap_or(1);
    let impacting = flag(query, "impacting");
    let coverage = weeks(db, dsp, &period)?;
    if let Some(refusal) = unknown(&period, &coverage, "") {
        return Err(refusal.into());
    }
    let found = rows(
        db,
        dsp,
        "customer_feedback",
        "delivery_time",
        &period,
        person.map(|p| p.amazon.as_slice()),
    )?;
    // Keep the rows the question is about, with the kinds each flags.
    let mut kept: Vec<(Row, Vec<&'static str>)> = vec![];
    for row in found {
        let negative = yes(&row.data, "negative_feedback_flag");
        let matches_kind = match kind {
            "negative" => negative,
            "positive" => !negative,
            _ => true,
        };
        let kinds: Vec<&'static str> = FEEDBACK
            .iter()
            .filter(|(_, field, _)| yes(&row.data, field))
            .map(|(name, _, _)| *name)
            .collect();
        if !matches_kind
            || (impacting && !yes(&row.data, "cdf_impact_flag"))
            || (!wanted_type.is_empty() && !kinds.contains(&wanted_type))
        {
            continue;
        }
        kept.push((row, kinds));
    }
    let tracking: Vec<String> = kept.iter().map(|(r, _)| r.tracking_id.clone()).collect();
    let looked_up = placed && (groups.contains(&"address") || flag(query, "list"));
    let addresses = if looked_up {
        (places.of)(db, dsp, &tracking)?
    } else {
        HashMap::new()
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
        "feedback": kept.len(),
        "coverage": coverage,
    });
    people.mark(&mut answer);
    if looked_up && access.read(places.area) == Read::Bypassed {
        access::bypassed(&mut answer, places.area.source());
    }
    if !groups.is_empty() {
        let mut counted: HashMap<Vec<String>, i64> = HashMap::new();
        let mut unplaced = 0;
        for (row, kinds) in &kept {
            let date = text(&row.data, "delivery_time")
                .chars()
                .take(10)
                .collect::<String>();
            let place = addresses.get(&row.tracking_id).cloned();
            if groups.contains(&"address") && place.is_none() {
                unplaced += 1;
                continue;
            }
            // A row flagging several kinds counts once under each when grouped by kind.
            let kind_keys: Vec<String> = if groups.contains(&"type") {
                if kinds.is_empty() {
                    vec!["unspecified".into()]
                } else {
                    kinds.iter().map(|k| k.to_string()).collect()
                }
            } else {
                vec![String::new()]
            };
            for kind_key in kind_keys {
                let keys: Vec<String> = groups
                    .iter()
                    .map(|g| match *g {
                        "driver" => who(&people, &row.transporter_id, &text(&row.data, "da_name")),
                        "address" => place.clone().unwrap_or_default(),
                        "type" => kind_key.clone(),
                        "week" => row.week.clone(),
                        _ => date.clone(),
                    })
                    .collect();
                *counted.entry(keys).or_default() += 1;
            }
        }
        counted.retain(|_, count| *count >= min);
        if groups.contains(&"address") {
            answer["unplaced"] = json!(unplaced);
            if unplaced > 0 {
                answer["note"] = json!(format!(
                    "{unplaced} feedback could not be placed at an address: their routes were not collected."
                ));
            }
        }
        paged(
            &mut answer,
            "groups",
            count_table(&groups, counted, "feedback"),
            query,
            100,
        )?;
    }
    if flag(query, "list") {
        let mut columns = vec!["date", "tracking", "driver", "types", "impacting"];
        if placed {
            columns.push("address");
        }
        let mut table = Table::new(&columns);
        for (row, kinds) in &kept {
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
        if groups.is_empty() {
            paged(&mut answer, "list", table, query, 100)?;
        } else {
            first_page(&mut answer, table, query)?;
        }
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
    let found = rows(
        db,
        dsp,
        "safety_events",
        "data_date",
        &period,
        person.map(|p| p.amazon.as_slice()),
    )?;
    let kind = |row: &Row| snake(&text(&row.data, "type"));
    let kept: Vec<&Row> = found
        .iter()
        .filter(|r| wanted_type.is_empty() || kind(r).contains(&wanted_type))
        .collect();
    // A dispute Amazon approved takes the event off the scorecard.
    let counts = |r: &Row| !snake(&text(&r.data, "final_resolution")).eq("dispute_approved");
    let mut by_type: BTreeMap<Vec<String>, (i64, i64)> = BTreeMap::new();
    for r in &kept {
        let entry = by_type.entry(vec![kind(r)]).or_default();
        entry.0 += 1;
        entry.1 += i64::from(counts(r));
    }
    let types = event_table(&["type"], by_type.into_iter().collect());
    let types = json!({"columns": types.columns, "rows": types.rows});
    let mut head = understood(dsp, Some(&period));
    if let Some(p) = person {
        head.insert("driver".into(), json!(p.name));
    }
    if !wanted_type.is_empty() {
        head.insert("type".into(), json!(wanted_type));
    }
    let mut answer = json!({
        "understood": head,
        "events": kept.len(),
        "counting": kept.iter().filter(|r| counts(r)).count(),
        "by_type": types,
        "coverage": coverage,
    });
    people.mark(&mut answer);
    if !groups.is_empty() {
        let mut counted: HashMap<Vec<String>, (i64, i64)> = HashMap::new();
        for r in &kept {
            let keys = groups
                .iter()
                .map(|g| match *g {
                    "driver" => who(&people, &r.transporter_id, &text(&r.data, "da_name")),
                    "type" => kind(r),
                    "week" => r.week.clone(),
                    _ => text(&r.data, "data_date"),
                })
                .collect();
            let entry = counted.entry(keys).or_default();
            entry.0 += 1;
            entry.1 += i64::from(counts(r));
        }
        paged(
            &mut answer,
            "groups",
            event_table(&groups, counted.into_iter().collect()),
            query,
            100,
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
        if groups.is_empty() {
            paged(&mut answer, "list", table, query, 100)?;
        } else {
            first_page(&mut answer, table, query)?;
        }
    }
    Ok(answer)
}

/// The notes Amazon leaves on a return when the driver did not contact the customer as
/// required: contact compliance, missed.
fn missed_contact(coaching: &str) -> bool {
    let lower = coaching.to_lowercase();
    !lower.is_empty()
        && (lower.contains("contact") || lower.contains("call") || lower.contains("text"))
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
    let found = rows(
        db,
        dsp,
        "returns_to_station",
        "delivery_planned_date",
        &period,
        person.map(|p| p.amazon.as_slice()),
    )?;
    let kept: Vec<&Row> = found
        .iter()
        .filter(|r| {
            let coaching = text(&r.data, "weekly_coaching");
            (reason.is_empty() || snake(&text(&r.data, "rts_reason_code")) == reason)
                && (!impacting || yes(&r.data, "impacting_dcr"))
                && match contact {
                    "missed" => missed_contact(&coaching),
                    "compliant" => text(&r.data, "weekly_exemption_reason") == "Contact Compliant",
                    _ => true,
                }
        })
        .collect();
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
        "returns": kept.len(),
        "hurting_dcr": kept.iter().filter(|r| yes(&r.data, "impacting_dcr")).count(),
        "contact_missed": kept.iter().filter(|r| missed_contact(&text(&r.data, "weekly_coaching"))).count(),
        "coverage": coverage,
    });
    people.mark(&mut answer);
    if !groups.is_empty() {
        let mut counted: HashMap<Vec<String>, i64> = HashMap::new();
        for r in &kept {
            let keys = groups
                .iter()
                .map(|g| match *g {
                    "driver" => who(&people, &r.transporter_id, &text(&r.data, "da_name")),
                    "reason" => snake(&text(&r.data, "rts_reason_code")),
                    "coaching" => match text(&r.data, "weekly_coaching") {
                        c if c.is_empty() => "none".to_owned(),
                        c => c,
                    },
                    "week" => r.week.clone(),
                    _ => text(&r.data, "delivery_planned_date"),
                })
                .collect();
            *counted.entry(keys).or_default() += 1;
        }
        paged(
            &mut answer,
            "groups",
            count_table(&groups, counted, "returns"),
            query,
            100,
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
        if groups.is_empty() {
            paged(&mut answer, "list", table, query, 100)?;
        } else {
            first_page(&mut answer, table, query)?;
        }
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
    let data = db.scorecard_db(&dsp.id)?;
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
    let drivers = data.all(
        "SELECT COALESCE(x.transporter_id,'') transporter_id,x.row FROM driver_scorecards x \
         JOIN scorecard_publications p ON p.id=x.publication_id AND p.active=1 AND p.scope_verified=1 \
         WHERE p.station=? AND p.week=? ORDER BY x.row_index",
        [&station, &week],
    )?;
    let mut tiers: BTreeMap<String, i64> = BTreeMap::new();
    let mut lines: Vec<(f64, Vec<Value>)> = vec![];
    for r in &drivers {
        let id = s(r, "transporter_id");
        if person.is_some_and(|p| !p.amazon.iter().any(|a| a == id)) {
            continue;
        }
        let row: Value = serde_json::from_str(s(r, "row")).unwrap_or_default();
        let overall = text(&row, "da_overall_tier");
        *tiers.entry(overall.clone()).or_default() += 1;
        if let Some(rank) = below_rank
            && tier_rank(&overall).is_none_or(|own| own >= rank)
        {
            continue;
        }
        let score = row["da_overall_score"].as_f64().unwrap_or(f64::MAX);
        let mut values = vec![json!(who(&people, id, &text(&row, "da_name")))];
        values.push(row["da_overall_score"].clone());
        for (_, field) in TIERS {
            values.push(match text(&row, field).as_str() {
                "" | "None" => Value::Null,
                tier => json!(tier),
            });
        }
        values.push(row["delivered"].clone());
        lines.push((score, values));
    }
    // Lowest scores first: the drivers to coach lead the table.
    lines.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut columns = vec!["driver", "score"];
    columns.extend(TIERS.iter().map(|(name, _)| *name));
    columns.push("delivered");
    let mut table = Table::new(&columns);
    for (_, values) in lines {
        table.push(values);
    }
    let mut answer = json!({
        "understood": head,
        "posted": true,
        "dsp": summary,
        "drivers_by_tier": tiers,
    });
    people.mark(&mut answer);
    paged(&mut answer, "drivers", table, query, 100)?;
    Ok(answer)
}
