//! The facts every answer is made of: each source's rows for a DSP's days, in the DSP's
//! own time, with the days each source has. Which of them an answer may read is `access`'s.
use super::{
    Failure, Refusal,
    scope::{People, Period, Person},
};
// A4: each source's facts, until its feature answers for them.
use crate::{
    Code, Result,
    collectors::{cortex, paycom},
    contracts::{AgentArea, DailyTimecard, DriverSource, Dsp, MealStatus},
    db::{Store, n, s},
    workforce::assessment::paycom_day,
};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeSet;

/// A time of day where the DSP is, as "08:42", from epoch milliseconds.
pub fn clock(ms: Option<i64>, zone: chrono_tz::Tz) -> Option<String> {
    let at = chrono::DateTime::from_timestamp_millis(ms?)?;
    Some(at.with_timezone(&zone).format("%H:%M").to_string())
}
fn minutes(seconds: Option<i64>) -> Option<i64> {
    seconds.map(|s| (s + 30) / 60)
}
/// A missing meal or any unfinished meal leaves its total duration unknown.
pub fn total_minutes(values: impl Iterator<Item = Option<i64>>) -> Option<i64> {
    let mut values = values.peekable();
    values.peek()?;
    values.sum()
}
pub fn zone(dsp: &Dsp) -> chrono_tz::Tz {
    dsp.timezone.parse().unwrap_or(chrono_tz::UTC)
}

/// One driver's itinerary on one day, with Amazon's own package and stop counts.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteDay {
    pub date: String,
    pub route: Option<String>,
    pub itinerary: String,
    pub transporter_id: String,
    pub driver_name: String,
    pub status: Option<String>,
    pub packages_total: i64,
    pub packages_delivered: i64,
    pub packages_remaining: i64,
    pub packages_undeliverable: i64,
    pub stops_total: i64,
    pub stops_completed: i64,
    pub departed: Option<String>,
    pub first_stop: Option<String>,
    pub last_stop: Option<String>,
    pub ended: Option<String>,
    pub break_minutes: Option<i64>,
    pub overtime_minutes: Option<i64>,
}
/// Each day a source holds, and for routes whether the day is final or a snapshot.
#[derive(Clone, Debug, Default)]
pub struct Coverage {
    pub enabled: bool,
    pub days: Vec<String>,
    pub missing: Vec<String>,
    pub snapshots: Vec<String>,
}
/// Written compactly: how many of the days the source holds, and the missing and snapshot
/// days as ranges, never every day by name.
impl Serialize for Coverage {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        let mut out = serde_json::Map::new();
        if !self.enabled {
            out.insert("enabled".into(), Value::Bool(false));
        } else {
            out.insert("collected".into(), self.days.len().into());
            out.insert("of".into(), (self.days.len() + self.missing.len()).into());
            if !self.missing.is_empty() {
                out.insert("missing".into(), super::shape::ranges(&self.missing));
            }
            if !self.snapshots.is_empty() {
                out.insert("snapshots".into(), super::shape::ranges(&self.snapshots));
            }
        }
        Value::Object(out).serialize(serializer)
    }
}
impl Coverage {
    pub fn of(enabled: bool, held: BTreeSet<String>, period: &Period) -> Self {
        if !enabled {
            return Self::default();
        }
        let (days, missing) = period
            .days()
            .into_iter()
            .partition(|day| held.contains(day));
        Self {
            enabled,
            days,
            missing,
            snapshots: vec![],
        }
    }
}

/// Routes driven in a period, every driver's or only `drivers`' transporter IDs.
pub fn routes(
    db: &Store,
    dsp: &Dsp,
    period: &Period,
    drivers: Option<&[String]>,
) -> Result<(Vec<RouteDay>, Coverage)> {
    let station = db.profile(&dsp.id)?.station_code;
    let data = db.routedata(&dsp.id)?;
    let published = data.all(
        "SELECT day,mode FROM route_publications WHERE station=? AND active=1 AND day BETWEEN ? AND ?",
        [&station, &period.first(), &period.last()],
    )?;
    let mut coverage = Coverage::of(
        true,
        published.iter().map(|r| s(r, "day").to_owned()).collect(),
        period,
    );
    coverage.snapshots = published
        .iter()
        .filter(|r| s(r, "mode") == "snapshot")
        .map(|r| s(r, "day").to_owned())
        .collect();
    let rows = data.all(
        "SELECT i.day,i.itinerary_id,i.transporter_id,i.driver_name,i.route_code,i.progress_status,\
         i.total_packages,i.delivered,i.remaining,i.undeliverable,i.total_locations,\
         i.completed_locations,i.breaks_secs,i.overtime_secs,d.departed_at,d.first_stop_at,\
         d.last_stop_at,d.session_end_at,d.stops_total,d.stops_completed,i.departed_at i_departed \
         FROM itineraries i JOIN route_publications p ON p.id=i.publication_id AND p.active=1 \
         LEFT JOIN driver_days d ON d.publication_id=i.publication_id AND d.itinerary_id=i.itinerary_id \
         WHERE p.station=? AND p.day BETWEEN ? AND ? ORDER BY i.day,i.route_code,i.driver_name",
        [&station, &period.first(), &period.last()],
    )?;
    let zone = zone(dsp);
    let number = |r: &Value, a: &str, b: &str| r[a].as_i64().or_else(|| r[b].as_i64()).unwrap_or(0);
    let found = rows
        .iter()
        .filter(|r| drivers.is_none_or(|ids| ids.iter().any(|id| id == s(r, "transporter_id"))))
        .map(|r| RouteDay {
            date: s(r, "day").into(),
            route: r["route_code"].as_str().map(str::to_owned),
            itinerary: s(r, "itinerary_id").into(),
            transporter_id: s(r, "transporter_id").into(),
            driver_name: s(r, "driver_name").into(),
            status: r["progress_status"].as_str().map(str::to_owned),
            packages_total: r["total_packages"].as_i64().unwrap_or(0),
            packages_delivered: r["delivered"].as_i64().unwrap_or(0),
            packages_remaining: r["remaining"].as_i64().unwrap_or(0),
            packages_undeliverable: r["undeliverable"].as_i64().unwrap_or(0),
            // Amazon's own stop counts; the station pickup is not a stop.
            stops_total: number(r, "total_locations", "stops_total"),
            stops_completed: number(r, "completed_locations", "stops_completed"),
            departed: clock(r["departed_at"].as_i64().or(r["i_departed"].as_i64()), zone),
            first_stop: clock(r["first_stop_at"].as_i64(), zone),
            last_stop: clock(r["last_stop_at"].as_i64(), zone),
            ended: clock(r["session_end_at"].as_i64(), zone),
            break_minutes: minutes(r["breaks_secs"].as_i64()),
            overtime_minutes: minutes(r["overtime_secs"].as_i64()),
        })
        .collect();
    Ok((found, coverage))
}

/// One employee's timecard for one day, as Paycom recorded it.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimecardDay {
    pub date: String,
    pub employee_code: String,
    pub name: String,
    pub hours: f64,
    pub clock_in: Option<String>,
    pub clock_out: Option<String>,
    pub lunch_minutes: Option<i64>,
    pub status: String,
    /// Punches Paycom recorded that do not read as a whole day.
    pub needs_review: bool,
}
fn clock_of(minute: i32, day: i32) -> String {
    let minute = minute.rem_euclid(1440);
    let next = match day {
        0 => String::new(),
        1 => " +1 day".into(),
        day => format!(" +{day} days"),
    };
    format!("{:02}:{:02}{next}", minute / 60, minute % 60)
}
fn timecard_day(row: DailyTimecard) -> TimecardDay {
    let card = row.card;
    let assessed = paycom_day(&card.status, &card.punches);
    let lunch = total_minutes(assessed.lunches.iter().map(|l| {
        let (out, back) = (l.out.as_ref()?, l.clock_in.as_ref()?);
        Some(i64::from(back.minute - out.minute))
    }));
    TimecardDay {
        date: card.date,
        employee_code: card.employee_code,
        name: row.name,
        hours: card.hours,
        clock_in: assessed.in_day.as_ref().map(|c| clock_of(c.minute, c.day)),
        clock_out: assessed.out_day.as_ref().map(|c| clock_of(c.minute, c.day)),
        lunch_minutes: lunch,
        status: card.status,
        needs_review: assessed.review,
    }
}

/// Timecards in a period, every employee's or only `codes`'.
pub fn timecards(
    db: &Store,
    dsp: &Dsp,
    period: &Period,
    codes: Option<&[String]>,
) -> Result<(Vec<TimecardDay>, Coverage)> {
    let mut found = vec![];
    let mut held = BTreeSet::new();
    // Raw source rows include punch arrays and source metadata. Convert each
    // bounded batch before loading the next, even for a whole-team question.
    let days = period.days();
    for chunk in days.chunks(7) {
        for (day, daily) in db.daily_range(
            &dsp.id,
            &chunk[0],
            chunk.last().unwrap(),
            "employeeName",
            false,
            codes,
        )? {
            if daily.available {
                held.insert(day.clone());
            }
            found.extend(daily.rows.into_iter().map(timecard_day));
        }
    }
    Ok((found, Coverage::of(true, held, period)))
}

/// One driver's meal breaks on one day: Cortex's meals beside Paycom's lunch punches.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MealDay {
    pub date: String,
    /// `paycom:<code>` or `cortex:<transporter ID>`, as the comparison joins them.
    pub source: String,
    pub name: String,
    pub status: MealStatus,
    pub meals: Vec<MealTaken>,
    pub late_clock_in: bool,
    pub long_gap: bool,
    /// Whether Cortex had a route for the person that day. Without one, as for office
    /// staff, the status says nothing about a missed meal.
    pub cortex_route: bool,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MealTaken {
    pub start: Option<String>,
    pub end: Option<String>,
    pub minutes: Option<i64>,
}
/// Every day's meal-break comparison in a period.
pub fn meal_breaks(db: &Store, dsp: &Dsp, period: &Period) -> Result<(Vec<MealDay>, Coverage)> {
    meal_breaks_for(db, dsp, period, None)
}
pub fn meal_breaks_for(
    db: &Store,
    dsp: &Dsp,
    period: &Period,
    sources: Option<&[String]>,
) -> Result<(Vec<MealDay>, Coverage)> {
    let mut found = vec![];
    let mut held = BTreeSet::new();
    let zone = zone(dsp);
    let local = |at: &str| {
        chrono::DateTime::parse_from_rfc3339(at)
            .ok()
            .map(|t| t.with_timezone(&zone))
    };
    let days = period.days();
    for chunk in days.chunks(7) {
        for (day, comparison) in db.meal_comparisons(
            &dsp.id,
            &chunk[0],
            chunk.last().unwrap(),
            &dsp.timezone,
            sources,
        )? {
            if !comparison.cortex_publications.is_empty() {
                held.insert(day.clone());
            }
            for row in comparison.rows {
                let meals = row
                    .source
                    .cortex
                    .iter()
                    .map(|meal| {
                        let (start, end) =
                            (local(&meal.start), meal.end.as_deref().and_then(local));
                        MealTaken {
                            start: start.map(|t| t.format("%H:%M").to_string()),
                            end: end.map(|t| t.format("%H:%M").to_string()),
                            minutes: start.zip(end).map(|(a, b)| (b - a).num_minutes()),
                        }
                    })
                    .collect();
                found.push(MealDay {
                    date: day.clone(),
                    source: row.source.id,
                    name: row.source.name,
                    status: row.assessment.status,
                    meals,
                    late_clock_in: row.assessment.late_in,
                    long_gap: row.assessment.long_gap,
                    cortex_route: false,
                });
            }
        }
    }
    Ok((found, Coverage::of(true, held, period)))
}

/// Each day's drivers Cortex had a route for, by transporter ID: those who could have
/// taken a meal break in it.
pub fn meal_routes(db: &Store, dsp: &Dsp, period: &Period) -> Result<BTreeSet<(String, String)>> {
    let rows = db.collector(&dsp.id, cortex::PROVIDER)?.all(
        "SELECT p.report_date day,i.transporter_id id FROM meal_itineraries i \
         JOIN meal_publications p ON p.id=i.publication_id AND p.active=1 \
         WHERE p.report_date BETWEEN ? AND ?",
        [&period.first(), &period.last()],
    )?;
    Ok(rows
        .iter()
        .map(|r| (s(r, "day").to_owned(), s(r, "id").to_owned()))
        .collect())
}

/// What happened to a package on a route day, from Amazon's record of its drop-off.
pub const OUTCOMES: &[&str] = &[
    "delivered",
    "returned",
    "attempted",
    "not_picked_up",
    "cancelled",
    "open",
];
const OUTCOME: &str = "CASE t.task_state WHEN 'DELIVERED' THEN 'delivered' \
    WHEN 'BACK_TO_ORIGIN' THEN 'returned' WHEN 'UNDELIVERABLE' THEN 'returned' \
    WHEN 'DELIVERY_ATTEMPTED' THEN 'attempted' \
    WHEN 'PICKUP_FAILED' THEN 'not_picked_up' WHEN 'NOT_DELIVERED' THEN 'cancelled' ELSE 'open' END";
/// Why: Amazon's reason for a return or failure, or where a delivered package was left.
const REASON: &str = "CASE WHEN t.state_context IS NULL OR t.state_context='NONE' THEN '' \
    WHEN t.state_context LIKE 'DELIVERED_TO_%' THEN lower(substr(t.state_context,14)) \
    ELSE lower(t.state_context) END";

/// The outcome [`OUTCOME`] gives in SQL, for a task already read.
pub fn outcome_of(state: Option<&str>) -> &'static str {
    match state {
        Some("DELIVERED") => "delivered",
        Some("BACK_TO_ORIGIN" | "UNDELIVERABLE") => "returned",
        Some("DELIVERY_ATTEMPTED") => "attempted",
        Some("PICKUP_FAILED") => "not_picked_up",
        Some("NOT_DELIVERED") => "cancelled",
        _ => "open",
    }
}
/// The reason [`REASON`] gives in SQL, for a task already read.
pub fn reason_of(context: Option<&str>) -> String {
    match context {
        None | Some("NONE") => String::new(),
        Some(context) => context
            .strip_prefix("DELIVERED_TO_")
            .unwrap_or(context)
            .to_lowercase(),
    }
}
/// A day's itineraries a route code or itinerary ID names, with their route and driver.
pub fn find_itinerary(
    db: &Store,
    dsp: &Dsp,
    day: &str,
    wanted: &str,
) -> Result<Vec<(String, String, String)>> {
    let station = db.profile(&dsp.id)?.station_code;
    let rows = db.routedata(&dsp.id)?.all(
        "SELECT i.itinerary_id,COALESCE(i.route_code,'') route,i.driver_name FROM itineraries i \
         JOIN route_publications p ON p.id=i.publication_id AND p.active=1 \
         WHERE p.station=?1 AND p.day=?2 AND (i.itinerary_id=?3 OR upper(i.route_code)=upper(?3)) \
         ORDER BY route",
        [&station, day, wanted.trim()],
    )?;
    Ok(rows
        .iter()
        .map(|r| {
            (
                s(r, "itinerary_id").into(),
                s(r, "route").into(),
                s(r, "driver_name").into(),
            )
        })
        .collect())
}

/// Which packages a question is about.
#[derive(Default)]
pub struct Packages<'a> {
    pub drivers: Option<&'a [String]>,
    pub outcome: Option<&'a str>,
    pub reason: Option<&'a str>,
    pub route: Option<&'a str>,
}
/// What packages can be grouped by, and the column each groups on.
pub fn package_group(name: &str) -> Option<&'static str> {
    Some(match name {
        "driver" => "t.transporter_id",
        "day" => "t.day",
        "outcome" => OUTCOME,
        "reason" => REASON,
        "route" => "COALESCE(i.route_code,'')",
        "address" => "COALESCE(t.address_id,'')",
        _ => return None,
    })
}
fn package_scope(
    dsp: &Dsp,
    db: &Store,
    period: &Period,
    wanted: &Packages,
) -> Result<(String, Vec<String>)> {
    let station = db.profile(&dsp.id)?.station_code;
    let mut sql = String::from(
        " FROM tasks t JOIN route_publications p ON p.id=t.publication_id AND p.active=1 \
         LEFT JOIN itineraries i ON i.publication_id=t.publication_id AND i.itinerary_id=t.itinerary_id \
         WHERE p.station=? AND t.day BETWEEN ? AND ? AND t.task_type='DROP_OFF' AND t.active=1",
    );
    let mut params = vec![station, period.first(), period.last()];
    if let Some(ids) = wanted.drivers {
        sql.push_str(&format!(
            " AND t.transporter_id IN ({})",
            vec!["?"; ids.len().max(1)].join(",")
        ));
        params.extend(ids.iter().cloned());
        if ids.is_empty() {
            params.push(String::new());
        }
    }
    if let Some(outcome) = wanted.outcome {
        sql.push_str(&format!(" AND ({OUTCOME})=?"));
        params.push(outcome.to_owned());
    }
    if let Some(reason) = wanted.reason {
        sql.push_str(&format!(" AND ({REASON})=?"));
        params.push(reason.to_owned());
    }
    if let Some(route) = wanted.route {
        sql.push_str(" AND upper(COALESCE(i.route_code,''))=upper(?)");
        params.push(route.to_owned());
    }
    Ok((sql, params))
}
/// Counts per group: the group's values, in the order asked, and how many packages.
pub type Grouped = Vec<(Vec<String>, i64)>;
/// A normal month at a large station still fits, while one request cannot scan an
/// arbitrarily large retained history or materialize an arbitrary number of groups.
const PACKAGE_CANDIDATE_LIMIT: i64 = 2_000_000;
const PACKAGE_GROUP_LIMIT: usize = 10_000;
const PACKAGE_QUERY_OPS: u64 = 25_000_000;

fn package_too_large(message: &str) -> Failure {
    Refusal::new(422, "package_query_too_large", message).into()
}

fn package_query<T>(result: Result<T>) -> std::result::Result<T, Failure> {
    result.map_err(|error| {
        if error.is(Code::QueryLimitExceeded) {
            package_too_large(
                "That package question needs too much database work at once. Ask about fewer \
                 days, then continue with the next period.",
            )
        } else {
            error.into()
        }
    })
}

/// How many packages a question covers, and their counts grouped by up to two columns,
/// largest first.
pub fn package_counts(
    db: &Store,
    dsp: &Dsp,
    period: &Period,
    wanted: &Packages,
    groups: &[&str],
) -> std::result::Result<(i64, Grouped), Failure> {
    let data = db.routedata(&dsp.id)?;
    let (scope, params) = package_scope(dsp, db, period, wanted)?;
    let total = package_query(data.all_bounded(
        &format!("SELECT COUNT(*) n{scope}"),
        rusqlite::params_from_iter(&params),
        PACKAGE_QUERY_OPS,
    ))?
    .first()
    .map(|r| n(r, "n"))
    .unwrap_or(0);
    if total > PACKAGE_CANDIDATE_LIMIT {
        return Err(package_too_large(
            "That package question covers too much route data at once. Ask about fewer days \
             or add a driver, route, outcome or reason filter.",
        ));
    }
    if groups.is_empty() {
        return Ok((total, vec![]));
    }
    let columns: Vec<String> = groups
        .iter()
        .enumerate()
        .filter_map(|(k, g)| package_group(g).map(|c| format!("({c}) g{k}")))
        .collect();
    let keys: Vec<String> = (0..columns.len()).map(|k| format!("g{k}")).collect();
    let rows = package_query(data.all_bounded(
        &format!(
            "SELECT {}, COUNT(*) n{scope} GROUP BY {} ORDER BY n DESC, {} LIMIT {}",
            columns.join(","),
            keys.join(","),
            keys.join(","),
            PACKAGE_GROUP_LIMIT + 1,
        ),
        rusqlite::params_from_iter(&params),
        PACKAGE_QUERY_OPS,
    ))?;
    if rows.len() > PACKAGE_GROUP_LIMIT {
        return Err(package_too_large(
            "That package grouping has too many distinct results. Ask about fewer days or \
             group by fewer fields.",
        ));
    }
    Ok((
        total,
        rows.iter()
            .map(|r| (keys.iter().map(|k| s(r, k).to_owned()).collect(), n(r, "n")))
            .collect(),
    ))
}
/// One package on a route day, for a list.
pub struct PackageRow {
    pub day: String,
    pub tracking: String,
    pub transporter_id: String,
    pub driver_name: String,
    pub route: String,
    pub outcome: String,
    pub reason: String,
    pub at: Option<String>,
    pub address_id: String,
}
/// The packages themselves, newest day first, a page at a time.
pub fn package_rows(
    db: &Store,
    dsp: &Dsp,
    period: &Period,
    wanted: &Packages,
    offset: usize,
    limit: usize,
) -> std::result::Result<Vec<PackageRow>, Failure> {
    let data = db.routedata(&dsp.id)?;
    let (scope, mut params) = package_scope(dsp, db, period, wanted)?;
    params.push(limit.to_string());
    params.push(offset.to_string());
    let zone = zone(dsp);
    Ok(package_query(data.all_bounded(
            &format!(
                "SELECT t.day,COALESCE(t.tracking_id,'') tracking,t.transporter_id,\
                 COALESCE(i.driver_name,'') driver_name,COALESCE(i.route_code,'') route,\
                 ({OUTCOME}) outcome,({REASON}) reason,t.executed_at,COALESCE(t.address_id,'') address_id\
                 {scope} ORDER BY t.day DESC,t.executed_at,t.task_id LIMIT ? OFFSET ?"
            ),
            rusqlite::params_from_iter(&params),
            PACKAGE_QUERY_OPS,
        ))?
        .iter()
        .map(|r| PackageRow {
            day: s(r, "day").into(),
            tracking: s(r, "tracking").into(),
            transporter_id: s(r, "transporter_id").into(),
            driver_name: s(r, "driver_name").into(),
            route: s(r, "route").into(),
            outcome: s(r, "outcome").into(),
            reason: s(r, "reason").into(),
            at: clock(r["executed_at"].as_i64(), zone),
            address_id: s(r, "address_id").into(),
        })
        .collect())
}
/// `None` when Amazon has given this reason at the DSP. Otherwise a bounded set of choices
/// for correcting it. Both probes share the package query work budget and ignore stale or
/// foreign publications.
pub fn unknown_package_reason_choices(
    db: &Store,
    dsp: &Dsp,
    wanted: &str,
) -> std::result::Result<Option<Vec<String>>, Failure> {
    let data = db.routedata(&dsp.id)?;
    let station = db.profile(&dsp.id)?.station_code;
    let scope = " FROM tasks t JOIN route_publications p ON p.id=t.publication_id AND p.active=1 \
                 WHERE p.station=? AND t.task_type='DROP_OFF' AND t.active=1";
    let present = package_query(data.all_bounded(
        &format!("SELECT 1 present{scope} AND ({REASON})=? LIMIT 1"),
        [station.as_str(), wanted],
        PACKAGE_QUERY_OPS,
    ))?;
    if !present.is_empty() {
        return Ok(None);
    }
    let rows = package_query(data.all_bounded(
        &format!("SELECT DISTINCT ({REASON}) reason{scope} LIMIT 100"),
        [station],
        PACKAGE_QUERY_OPS,
    ))?;
    Ok(Some(
        rows.iter()
            .map(|r| s(r, "reason").to_owned())
            .filter(|r| !r.is_empty())
            .collect(),
    ))
}
/// One-line addresses, by the IDs Amazon gives them.
pub fn addresses(
    db: &Store,
    dsp: &Dsp,
    ids: &[String],
) -> Result<std::collections::HashMap<String, String>> {
    let data = db.routedata(&dsp.id)?;
    let mut out = std::collections::HashMap::new();
    for chunk in ids.chunks(400) {
        let rows = data.all(
            &format!(
                "SELECT address_id,address1,address2,city,state,postal_code FROM addresses \
                 WHERE address_id IN ({})",
                vec!["?"; chunk.len()].join(",")
            ),
            rusqlite::params_from_iter(chunk),
        )?;
        for r in rows {
            let parts: Vec<&str> = ["address1", "address2", "city", "state", "postal_code"]
                .iter()
                .map(|k| s(&r, k))
                .filter(|v| !v.is_empty())
                .collect();
            out.insert(s(&r, "address_id").to_owned(), parts.join(", "));
        }
    }
    Ok(out)
}
/// The days routes were collected for, and which of them are snapshots.
pub fn route_coverage(db: &Store, dsp: &Dsp, period: &Period) -> Result<Coverage> {
    let station = db.profile(&dsp.id)?.station_code;
    let data = db.routedata(&dsp.id)?;
    let published = data.all(
        "SELECT day,mode FROM route_publications WHERE station=? AND active=1 AND day BETWEEN ? AND ?",
        [&station, &period.first(), &period.last()],
    )?;
    let mut coverage = Coverage::of(
        true,
        published.iter().map(|r| s(r, "day").to_owned()).collect(),
        period,
    );
    coverage.snapshots = published
        .iter()
        .filter(|r| s(r, "mode") == "snapshot")
        .map(|r| s(r, "day").to_owned())
        .collect();
    Ok(coverage)
}

/// One vehicle inspection.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inspection {
    pub date: String,
    pub transporter_id: String,
    pub driver_name: String,
    pub vehicle_type: String,
    pub inspection_type: String,
    pub started: String,
    pub seconds: i64,
    pub minimum_seconds: i64,
    pub short: bool,
}
/// Inspections in a period, every driver's or only `drivers`' transporter IDs.
pub fn inspections(
    db: &Store,
    dsp: &Dsp,
    period: &Period,
    drivers: Option<&[String]>,
) -> Result<(Vec<Inspection>, Coverage)> {
    let station = db.profile(&dsp.id)?.station_code;
    let data = db.dvic(&dsp.id)?;
    // A report covers every day from its first to its last row.
    let mut held = BTreeSet::new();
    for report in data.all(
        "SELECT min_date,max_date FROM dvic_reports WHERE station=? AND scope_verified=1 AND min_date IS NOT NULL",
        [&station],
    )? {
        held.extend(period.days().into_iter().filter(|day| {
            day.as_str() >= s(&report, "min_date") && day.as_str() <= s(&report, "max_date")
        }));
    }
    let rows = data.all(
        "SELECT start_date,transporter_id,transporter_name,fleet_type,inspection_type,start_time,\
         duration_seconds,minimum_seconds,short FROM dvic_inspections \
         WHERE station=? AND scope_verified=1 AND start_date BETWEEN ? AND ? ORDER BY start_date,start_time",
        [&station, &period.first(), &period.last()],
    )?;
    let found = rows
        .iter()
        .filter(|r| drivers.is_none_or(|ids| ids.iter().any(|id| id == s(r, "transporter_id"))))
        .map(|r| Inspection {
            date: s(r, "start_date").into(),
            transporter_id: s(r, "transporter_id").into(),
            driver_name: s(r, "transporter_name").into(),
            vehicle_type: s(r, "fleet_type").into(),
            inspection_type: s(r, "inspection_type").into(),
            started: s(r, "start_time").into(),
            seconds: r["duration_seconds"].as_f64().unwrap_or(0.0).round() as i64,
            minimum_seconds: n(r, "minimum_seconds"),
            short: n(r, "short") == 1,
        })
        .collect();
    Ok((found, Coverage::of(true, held, period)))
}

/// When Paycom last brought timecards in, for the status answer.
pub fn fresh_timecards(db: &Store, dsp: &Dsp, _station: &str) -> Result<Option<Value>> {
    let paycom = db.collector(&dsp.id, paycom::PROVIDER)?.one(
        "SELECT collected_at,period_from,period_to FROM publications WHERE active=1",
        [],
    )?;
    Ok(paycom.map(|r| {
        serde_json::json!({
            "collectedAt": r["collected_at"], "periodFrom": r["period_from"], "periodTo": r["period_to"]
        })
    }))
}
/// The latest day Cortex's meal breaks were collected for.
pub fn fresh_meals(db: &Store, dsp: &Dsp, _station: &str) -> Result<Option<Value>> {
    let meals = db.collector(&dsp.id, cortex::PROVIDER)?.one(
        "SELECT max(report_date) day,max(collected_at) collected_at FROM meal_publications WHERE active=1",
        [],
    )?;
    Ok(meals.filter(|r| !r["day"].is_null()).map(|r| {
        serde_json::json!({
            "latestDay": r["day"], "collectedAt": r["collected_at"]
        })
    }))
}
/// The latest route day collected, and whether it is final.
pub fn fresh_routes(db: &Store, dsp: &Dsp, station: &str) -> Result<Option<Value>> {
    let routes = db.routedata(&dsp.id)?.one(
        "SELECT day,mode,collected_at FROM route_publications WHERE station=? AND active=1 \
         ORDER BY day DESC LIMIT 1",
        [station],
    )?;
    Ok(routes.map(|r| {
        serde_json::json!({
            "latestDay": r["day"], "final": r["mode"] == "final", "collectedAt": r["collected_at"]
        })
    }))
}
/// The latest day DVIC's reports cover.
pub fn fresh_dvic(db: &Store, dsp: &Dsp, station: &str) -> Result<Option<Value>> {
    let dvic = db.dvic(&dsp.id)?.one(
        "SELECT max(max_date) day,max(checked_at) checked_at FROM dvic_reports WHERE station=? AND scope_verified=1",
        [station],
    )?;
    Ok(dvic.filter(|r| !r["day"].is_null()).map(|r| {
        serde_json::json!({
            "latestDay": r["day"], "checkedAt": r["checked_at"]
        })
    }))
}
/// The latest scorecard week collected.
pub fn fresh_scorecard(db: &Store, dsp: &Dsp, station: &str) -> Result<Option<Value>> {
    let scorecard = db.scorecard(&dsp.id)?.one(
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

/// One kind of a DSP's facts by driver and day, which `drivers/{driver}` joins into a
/// driver's days and `team` into the team's table. A feature answers for each kind it
/// holds, in its manifest's `mcp`.
pub trait Daily: Sync {
    /// The kind of data it is read as.
    fn area(&self) -> AgentArea;
    /// Its name under an answer's `coverage`.
    fn coverage(&self) -> &'static str;
    /// Its name for its records on each day of a driver's full report.
    fn records(&self) -> &'static str;
    /// Its columns of a driver's days.
    fn columns(&self) -> &'static [&'static str];
    /// Whether its days are assessed one at a time, so that a question needing it is held
    /// to `DAILY_LONGEST` days.
    fn assessed(&self) -> bool {
        false
    }
    /// Its metrics whose team total is their value over every record, not the sum of the
    /// drivers', as a total that one unknown makes unknown.
    fn pooled(&self) -> &'static [&'static str] {
        &[]
    }
    /// What it holds in a period: everyone's, or one person's.
    fn gather(
        &self,
        db: &Store,
        dsp: &Dsp,
        period: &Period,
        people: &People,
        person: Option<&Person>,
    ) -> Result<Box<dyn Facts>>;
    /// Nothing, for an answer that doesn't read it.
    fn none(&self) -> Box<dyn Facts>;
}

/// What one kind holds for a period: its records, in its own order, and the days it has.
pub trait Facts {
    fn coverage(&self) -> &Coverage;
    fn count(&self) -> usize;
    /// A record's day, and whom it names: the source, the ID it knows them by and the name
    /// it gives them.
    fn whose(&self, record: usize) -> (&str, DriverSource, &str, &str);
    /// A record in full, for a driver's full report.
    fn record(&self, record: usize) -> Value;
    /// Whether a record makes a line of a driver's days.
    fn lined(&self, _record: usize) -> bool {
        true
    }
    /// Its columns of one of a driver's days, from that day's records that make a line,
    /// which may be none.
    fn line(&self, records: &[usize]) -> Vec<Value>;
    /// Its figures in a driver's report, by name.
    fn totals(&self) -> Vec<(&'static str, Value)>;
    /// One of its kind's metrics over some of its records, never none.
    fn metric(&self, name: &str, records: &[usize]) -> Option<Value>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn card(punches: Value) -> TimecardDay {
        timecard_day(
            serde_json::from_value(json!({
                "employeeCode":"E1", "date":"2026-09-12", "name":"Driver",
                "hours":8, "status":"Complete", "punches":punches,
            }))
            .unwrap(),
        )
    }

    #[test]
    fn lunch_totals_distinguish_missing_open_mixed_complete_and_zero_durations() {
        let cases = [
            (json!([{"in":"08:00","out":"17:00"}]), None),
            (
                json!([{"in":"08:00","inKind":"IN DAY","out":"12:00","outKind":"OUT LUNCH"}]),
                None,
            ),
            (
                json!([
                    {"in":"08:00","inKind":"IN DAY","out":"12:00","outKind":"OUT LUNCH"},
                    {"in":"12:30","inKind":"IN LUNCH","out":"14:00","outKind":"OUT LUNCH"}
                ]),
                None,
            ),
            (
                json!([{"in":"08:00","out":"12:00"},{"in":"12:30","out":"17:00"}]),
                Some(30),
            ),
            (
                json!([{"in":"08:00","out":"12:00"},{"in":"12:00","out":"17:00"}]),
                Some(0),
            ),
        ];
        for (punches, expected) in cases {
            assert_eq!(card(punches.clone()).lunch_minutes, expected, "{punches}");
        }
        assert_eq!(total_minutes([Some(30), Some(15)].into_iter()), Some(45));
        assert_eq!(total_minutes([Some(30), None].into_iter()), None);
        assert_eq!(total_minutes([None].into_iter()), None);
        assert_eq!(total_minutes([].into_iter()), None);
    }

    #[test]
    fn overnight_lunch_uses_elapsed_minutes_and_local_clock_labels() {
        let card = card(json!([
            {"in":"22:00","inKind":"IN DAY","out":"23:50","outKind":"OUT LUNCH"},
            {"in":"00:20","inKind":"IN LUNCH","out":"06:00","outKind":"OUT DAY"}
        ]));
        assert_eq!(card.lunch_minutes, Some(30));
        assert_eq!(card.clock_in.as_deref(), Some("22:00"));
        assert_eq!(card.clock_out.as_deref(), Some("06:00 +1 day"));
        assert!(!card.needs_review);
    }
}
