//! What Daily Performance exposes to every other feature and agent tool: `daily_metrics`, its
//! drivers' days from the active publications of the DSP's station, as its days summary reads
//! them (`dispatch_daily_performance_contract`).
use super::storage::{flag, text};
use crate::DailyPerformanceStore;
use chrono::NaiveDate;
use dispatch_core::{
    Error, Result,
    db::{Store, s},
    ensure,
    foundation::validate,
    manifest::people::Source,
};
use dispatch_daily_performance_contract::{
    Behaviors, Collected, Count, DailyMetrics, Day, Days, Feedback, Quality, Returned, Returns,
    Safety, SafetyEvent,
};
use rusqlite::params;
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

/// The reasons customers give for negative feedback.
const NEGATIVES: &[&str] = &[
    "was_not_so_great_cnt",
    "delivered_to_wrong_address_cnt",
    "never_received_delivery_cnt",
    "not_delivered_to_preferred_location_cnt",
    "received_wrong_item_cnt",
    "item_damaged_cnt",
    "late_delivery_cnt",
    "driver_mishandled_package_cnt",
    "driver_was_unprofessional_cnt",
    "driver_was_driving_unsafely_cnt",
];
/// The delivery success behaviors Amazon counts.
const CONCERNS: &[&str] = &[
    "dnr_count",
    "dac_dnr_count",
    "dac_loss_count",
    "attended_count",
    "unattended_count",
    "simultaneous_del_bucket_count",
    "del_greater_50m_dist_count",
    "incorrect_scan_attended_delivery_count",
    "incorrect_scan_unattended_delivery_count",
    "no_pod_bucket_count",
];

/// The active publications of a station, `?1`, whose drivers' quality Amazon had posted: one
/// whose datasets all came back empty is published too, but proves nothing.
const OBSERVED: &str = "FROM daily_publications p JOIN daily_datasets d ON d.publication_id=p.id \
    AND d.dataset='driver_quality' AND d.coverage='observed' WHERE p.station=?1 AND p.active=1";

/// Daily Performance's daily metrics, as its contract says.
pub(crate) struct Exposed;
pub(crate) static EXPOSED: Exposed = Exposed;

impl DailyMetrics for Exposed {
    fn source(&self) -> Source {
        dispatch_cortex::SOURCE
    }
    fn collected(&self, db: &Store, dsp: &str) -> Result<Option<Collected>> {
        let station = db.profile(dsp)?.station_code;
        let row = db.daily_performance_db(dsp)?.one(
            &format!("SELECT min(p.date) first,max(p.date) last {OBSERVED}"),
            [&station],
        )?;
        Ok(row.and_then(|row| {
            Some(Collected {
                from: row["first"].as_str()?.into(),
                through: row["last"].as_str()?.into(),
            })
        }))
    }
    fn days(&self, db: &Store, dsp: &str, ids: &[String], from: &str, to: &str) -> Result<Days> {
        let day = |value: &str| {
            validate::date(value)?;
            NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .map_err(|_| Error::new("invalid_date", 400))
        };
        let span = (day(to)? - day(from)?).num_days();
        ensure(
            (0..31).contains(&span)
                && ids.len() <= 20
                && ids
                    .iter()
                    .all(|id| !id.is_empty() && id.chars().count() <= 128),
            "invalid_input",
            400,
        )?;
        let station = db.profile(dsp)?.station_code;
        let data = db.daily_performance_db(dsp)?;
        let collected = data
            .query_as::<(String,)>(
                &format!("SELECT p.date {OBSERVED} AND p.date>=?2 AND p.date<=?3 ORDER BY p.date"),
                params![station, from, to],
            )?
            .into_iter()
            .map(|(date,)| date)
            .collect();
        let mut rows = data.all(
            "SELECT x.date,x.transporter_id id,x.dataset,x.impact,x.row FROM daily_rows x \
             JOIN daily_publications p ON p.id=x.publication_id WHERE p.station=?1 AND p.active=1 \
             AND x.date>=?2 AND x.date<=?3 AND x.transporter_id IN (SELECT value FROM json_each(?4)) \
             AND x.dataset IN ('driver_quality','driver_safety','driver_feedback',\
             'driver_delivery_behaviors','driver_returns','returns_to_station','safety_events',\
             'live_safety_events') ORDER BY x.date,x.transporter_id,x.dataset,x.row_index",
            params![station, from, to, serde_json::to_string(ids)?],
        )?;
        // The assessed events first, so an event Amazon reports both ways is told once, assessed.
        rows.sort_by_key(|row| s(row, "dataset") != "safety_events");
        let mut events = HashSet::new();
        let mut days = BTreeMap::new();
        for row in &rows {
            let dataset = s(row, "dataset");
            let fields: Value = serde_json::from_str(s(row, "row"))?;
            if matches!(dataset, "safety_events" | "live_safety_events")
                && let Some(event) = text(&fields, "event_id")
                && !events.insert(event)
            {
                continue;
            }
            let (date, id) = (s(row, "date"), s(row, "id"));
            let day = days.entry((date, id)).or_insert_with(|| Day {
                date: date.into(),
                id: id.into(),
                quality: None,
                safety: None,
                feedback: None,
                behaviors: None,
                returns: None,
                returned: Vec::new(),
                events: Vec::new(),
            });
            let impact = row["impact"].as_i64().map(|impact| impact == 1);
            add(day, dataset, &fields, impact);
        }
        Ok(Days {
            collected,
            days: days.into_values().collect(),
        })
    }
}

/// Adds a stored row of `dataset` to its day: the first of the datasets with a row a day, and
/// each package and event. `impact` is whether it counts against the driver, as stored.
fn add(day: &mut Day, dataset: &str, fields: &Value, impact: Option<bool>) {
    match dataset {
        "driver_quality" => {
            day.quality.get_or_insert(Quality {
                dispatched: number(fields, "dispatched"),
                delivered: number(fields, "delivered"),
                delivery_success_rate: number(fields, "delivery_success"),
                completed_routes: number(fields, "completed_routes"),
                dnr: number(fields, "dnr"),
                returned: number(fields, "return_to_station_all"),
                pod_opportunities: number(fields, "pod_opportunity"),
                pod_successes: number(fields, "pod_success"),
                pod_success_rate: number(fields, "pod_success_rate"),
                pod_bypassed: number(fields, "pod_bypass"),
                not_on_van: number(fields, "packages_not_on_van"),
                transferred_out: number(fields, "transferred_out"),
            });
        }
        "driver_safety" => {
            day.safety.get_or_insert(Safety {
                speeding: number(fields, "speeding_events"),
                seatbelt: number(fields, "seatbelt_events"),
                distraction: number(fields, "distraction_events"),
                sign_signal: number(fields, "sign_signal_events"),
                following_distance: number(fields, "following_distance_events"),
            });
        }
        "driver_feedback" => {
            day.feedback.get_or_insert(Feedback {
                positive: number(fields, "positive_response_cnt"),
                negative: number(fields, "negative_response_cnt"),
                negatives: counts(fields, NEGATIVES),
            });
        }
        "driver_delivery_behaviors" => {
            day.behaviors.get_or_insert(Behaviors {
                dsb: number(fields, "dsb_count"),
                concerns: counts(fields, CONCERNS),
            });
        }
        "driver_returns" => {
            day.returns.get_or_insert(Returns {
                all: number(fields, "rts_all"),
                controllable: number(fields, "rts_all_controllable"),
                exempted: number(fields, "rts_all_exempted"),
            });
        }
        "returns_to_station" => day.returned.push(Returned {
            tracking_id: text(fields, "tracking_id"),
            reason: text(fields, "rts_reason_code"),
            impacting: impact,
            exempted: flag(&fields["daily_exemption"]).map(|exempted| exempted == 1),
            exemption_reason: text(fields, "daily_exemption_reason"),
            coaching: text(fields, "daily_coaching"),
        }),
        _ => day.events.push(SafetyEvent {
            event_id: text(fields, "event_id"),
            kind: text(fields, "dashboard_metric_type"),
            subtype: text(fields, "dashboard_metric_subtype"),
            severity: text(fields, "severity"),
            started: text(fields, "event_start_time_local"),
            impacting: impact,
            assessed: dataset == "safety_events",
        }),
    }
}

/// A number, or one written as text; anything else is none.
fn number(fields: &Value, key: &str) -> Option<f64> {
    match &fields[key] {
        Value::Number(value) => value.as_f64(),
        Value::String(value) => value.trim().parse().ok(),
        _ => None,
    }
    .filter(|value: &f64| value.is_finite())
}

/// Each of `keys` that counts more than nothing.
fn counts(fields: &Value, keys: &[&str]) -> Vec<Count> {
    keys.iter()
        .filter_map(|key| {
            let count = number(fields, key).filter(|count| *count != 0.0)?;
            Some(Count {
                what: (*key).into(),
                count,
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "../tests/backend/exposed.rs"]
mod tests;
