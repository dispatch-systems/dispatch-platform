//! The facts every answer is made of: each source's rows for a DSP's days, in the DSP's
//! own time, with the days each source has. A source a DSP has switched off is left out.
use super::scope::Period;
use crate::{
    Result,
    collectors::Provider,
    contracts::{Dsp, MealStatus},
    db::{Store, n, s},
    workforce::assessment::paycom_day,
};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeSet;

/// The sources a DSP has switched on, as agents may read them.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sources {
    pub timecards: bool,
    pub meal_breaks: bool,
    pub routes: bool,
    pub dvic: bool,
}
impl Sources {
    pub fn of(db: &Store, dsp: &str) -> Result<Self> {
        let on = db.features(dsp)?;
        let has = |id: &str| on.iter().any(|f| f == id);
        Ok(Self {
            timecards: has("timecard.daily") || has("timecard.employees"),
            meal_breaks: has("timecard.meal_breaks"),
            routes: has("routes"),
            dvic: has("dvic"),
        })
    }
}

/// A time of day where the DSP is, as "08:42", from epoch milliseconds.
pub fn clock(ms: Option<i64>, zone: chrono_tz::Tz) -> Option<String> {
    let at = chrono::DateTime::from_timestamp_millis(ms?)?;
    Some(at.with_timezone(&zone).format("%H:%M").to_string())
}
fn minutes(seconds: Option<i64>) -> Option<i64> {
    seconds.map(|s| (s + 30) / 60)
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
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Coverage {
    pub enabled: bool,
    pub days: Vec<String>,
    pub missing: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub snapshots: Vec<String>,
}
impl Coverage {
    fn of(enabled: bool, held: BTreeSet<String>, period: &Period) -> Self {
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
    let next = if day > 0 { " +1 day" } else { "" };
    format!("{:02}:{:02}{next}", minute / 60, minute % 60)
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
    for day in period.days() {
        let daily = db.daily(&dsp.id, &day, "employeeName", false)?;
        if daily.available {
            held.insert(day.clone());
        }
        for row in daily.rows {
            let card = row.card;
            if codes.is_some_and(|codes| !codes.contains(&card.employee_code)) {
                continue;
            }
            let assessed = paycom_day(&card.status, &card.punches);
            let lunch: i64 = assessed
                .lunches
                .iter()
                .filter_map(|l| {
                    let (out, back) = (l.out.as_ref()?, l.clock_in.as_ref()?);
                    Some(i64::from(
                        (back.day - out.day) * 1440 + back.minute - out.minute,
                    ))
                })
                .sum();
            found.push(TimecardDay {
                date: card.date,
                employee_code: card.employee_code,
                name: row.name,
                hours: card.hours,
                clock_in: assessed.in_day.as_ref().map(|c| clock_of(c.minute, c.day)),
                clock_out: assessed.out_day.as_ref().map(|c| clock_of(c.minute, c.day)),
                lunch_minutes: (!assessed.lunches.is_empty()).then_some(lunch),
                status: card.status,
                needs_review: assessed.review,
            });
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
    let mut found = vec![];
    let mut held = BTreeSet::new();
    let zone = zone(dsp);
    let local = |at: &str| {
        chrono::DateTime::parse_from_rfc3339(at)
            .ok()
            .map(|t| t.with_timezone(&zone))
    };
    for day in period.days() {
        let comparison = db.meal_comparison(&dsp.id, &day, &dsp.timezone)?;
        if !comparison.cortex_publications.is_empty() {
            held.insert(day.clone());
        }
        for row in comparison.rows {
            let meals = row
                .source
                .cortex
                .iter()
                .map(|meal| {
                    let (start, end) = (local(&meal.start), meal.end.as_deref().and_then(local));
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
            });
        }
    }
    Ok((found, Coverage::of(true, held, period)))
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
        "SELECT min_date,max_date FROM dvic_reports WHERE station=? AND min_date IS NOT NULL",
        [&station],
    )? {
        held.extend(period.days().into_iter().filter(|day| {
            day.as_str() >= s(&report, "min_date") && day.as_str() <= s(&report, "max_date")
        }));
    }
    let rows = data.all(
        "SELECT start_date,transporter_id,transporter_name,fleet_type,inspection_type,start_time,\
         duration_seconds,minimum_seconds,short FROM dvic_inspections \
         WHERE station=? AND start_date BETWEEN ? AND ? ORDER BY start_date,start_time",
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

/// When each source last brought something in, for the status answer.
pub fn freshness(db: &Store, dsp: &Dsp) -> Result<Value> {
    let station = db.profile(&dsp.id)?.station_code;
    let paycom = db.collector(&dsp.id, Provider::Paycom)?.one(
        "SELECT collected_at,period_from,period_to FROM publications WHERE active=1",
        [],
    )?;
    let meals = db.collector(&dsp.id, Provider::Cortex)?.one(
        "SELECT max(report_date) day,max(collected_at) collected_at FROM meal_publications WHERE active=1",
        [],
    )?;
    let routes = db.routedata(&dsp.id)?.one(
        "SELECT day,mode,collected_at FROM route_publications WHERE station=? AND active=1 \
         ORDER BY day DESC LIMIT 1",
        [&station],
    )?;
    let dvic = db.dvic(&dsp.id)?.one(
        "SELECT max(max_date) day,max(checked_at) checked_at FROM dvic_reports WHERE station=?",
        [&station],
    )?;
    Ok(serde_json::json!({
        "timecards": paycom.map(|r| serde_json::json!({
            "collectedAt": r["collected_at"], "periodFrom": r["period_from"], "periodTo": r["period_to"]
        })),
        "mealBreaks": meals.filter(|r| !r["day"].is_null()).map(|r| serde_json::json!({
            "latestDay": r["day"], "collectedAt": r["collected_at"]
        })),
        "routes": routes.map(|r| serde_json::json!({
            "latestDay": r["day"], "final": r["mode"] == "final", "collectedAt": r["collected_at"]
        })),
        "dvic": dvic.filter(|r| !r["day"].is_null()).map(|r| serde_json::json!({
            "latestDay": r["day"], "checkedAt": r["checked_at"]
        })),
    }))
}
