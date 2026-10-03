//! The facts every answer is made of: each source's rows for a DSP's days, in the DSP's
//! own time, with the days each source has. Which of them an answer may read is `access`'s.
use super::{
    Failure, Refusal,
    scope::{People, Period, Person},
};
// A4: each source's facts, until its feature answers for them.
use crate::{
    Code, Result,
    contracts::{AgentArea, DriverSource, Dsp},
    db::{Store, n, s},
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
