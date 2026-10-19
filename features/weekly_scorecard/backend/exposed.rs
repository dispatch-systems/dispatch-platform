//! What Weekly Scorecard exposes to every other feature and agent tool: each driver's weekly
//! scorecards and the packages they list, from the weeks its own reads show
//! (`dispatch_weekly_scorecard_contract`).
use super::{WeeklyScorecardStore, flag, text, weekly_scorecard_timezone};
use chrono::{DateTime, Datelike, Duration};
use chrono_tz::Tz;
use dispatch_core::{
    Result,
    db::{Store, s},
    foundation::{validate, weeks::week_days},
    manifest::people::Source,
};
use dispatch_weekly_scorecard_contract::{
    Collected, DriverScorecards, Metric, Package, PackageKind, Scorecard,
};
use serde_json::Value;

/// The most days one read covers, and the most IDs it names.
const LONGEST: i64 = 366;
const MOST_IDS: usize = 32;
/// Each metric a scorecard grades, in its order: its name, and the fields holding what it
/// measured, its tier and its score.
const METRICS: &[(&str, &str, &str, &str)] = &[
    (
        "seatbelt",
        "seatbelt_rate",
        "seatbelt_tier",
        "seatbelt_score",
    ),
    (
        "speeding",
        "speeding_rate",
        "speeding_tier",
        "speeding_score",
    ),
    (
        "distractions",
        "distractions_rate",
        "distractions_tier",
        "distractions_score",
    ),
    (
        "following_distance",
        "following_distance_rate",
        "following_distance_tier",
        "following_distance_score",
    ),
    (
        "sign_signal_violations",
        "sign_signal_violations_rate",
        "sign_signal_violations_rate_tier",
        "sign_signal_violations_rate_score",
    ),
    (
        "cdf_dpmo",
        "cdf_metric_dpmo",
        "cdf_dpmo_tier",
        "cdf_dpmo_score",
    ),
    ("ced", "escalations_count", "ced_tier", "ced_score"),
    ("rts", "rts_metric_adjusted", "rts_tier", "rts_score"),
    ("dsb", "dsb_metric_dpmo", "dsb_tier", "dsb_score"),
    ("pod", "pod_metric", "pod_tier", "pod_score"),
];
/// The flags a concession raises, each its reason without the `_flag`.
const CONCESSION_FLAGS: &[&str] = &[
    "dac_dnr_flag",
    "dac_loss_flag",
    "simultaneous_del_bucket_flag",
    "del_greater_50m_dist_flag",
    "incorrect_scan_attended_delivery_flag",
    "incorrect_scan_unattended_delivery_flag",
    "no_pod_bucket_flag",
];
/// The answers a customer marks, the bad ones first.
const FEEDBACK_ANSWERS: &[&str] = &[
    "driver_was_unprofessional",
    "driver_mishandled_package",
    "delivered_to_wrong_address",
    "never_received_delivery",
    "not_delivered_to_preferred_location",
    "received_wrong_item",
    "delivered_with_care",
    "followed_instructions",
    "friendly",
    "respectful_of_property",
    "thank_my_driver",
    "above_and_beyond",
];

/// Weekly Scorecard's drivers' scorecards, as `DriverScorecards` reads them.
pub(crate) struct Exposed;
pub(crate) static EXPOSED: Exposed = Exposed;

impl DriverScorecards for Exposed {
    fn source(&self) -> Source {
        dispatch_cortex::SOURCE
    }
    fn collected(&self, db: &Store, dsp: &str) -> Result<Option<Collected>> {
        let weeks: Vec<String> = db
            .weekly_scorecard_weeks(dsp)?
            .weeks
            .into_iter()
            .rev()
            .filter(|week| week.posted && week.publication.is_some())
            .map(|week| week.week)
            .collect();
        let (Some(first), Some(last)) = (weeks.first(), weeks.last()) else {
            return Ok(None);
        };
        let (from, through) = (week_days(first)?.0, week_days(last)?.1);
        Ok(Some(Collected {
            from: from.to_string(),
            through: through.to_string(),
            weeks,
        }))
    }
    fn scorecards(
        &self,
        db: &Store,
        dsp: &str,
        ids: &[String],
        from: &str,
        to: &str,
    ) -> Result<Vec<Scorecard>> {
        let weeks = overlapping(ids, from, to)?;
        rows(db, dsp, "driver_weekly_scorecards", ids, &weeks)?
            .iter()
            .map(scorecard)
            .collect()
    }
    fn packages(
        &self,
        db: &Store,
        dsp: &str,
        ids: &[String],
        from: &str,
        to: &str,
    ) -> Result<Vec<Package>> {
        let weeks = overlapping(ids, from, to)?;
        let tz = weekly_scorecard_timezone(db, dsp)?;
        let mut packages = Vec::new();
        for (kind, table) in [
            (PackageKind::Concession, "delivery_concessions"),
            (PackageKind::Return, "returns_to_station"),
            (PackageKind::Feedback, "customer_feedback"),
        ] {
            for row in rows(db, dsp, table, ids, &weeks)? {
                packages.push(package(kind, &row, tz)?);
            }
        }
        packages.sort_by(|a, b| (&a.week, &a.date).cmp(&(&b.week, &b.date)));
        Ok(packages)
    }
}

/// A driver's scorecard, from its row.
fn scorecard(row: &Value) -> Result<Scorecard> {
    let (sunday, saturday) = week_days(s(row, "week"))?;
    let fields: Value = serde_json::from_str(s(row, "row"))?;
    Ok(Scorecard {
        week: s(row, "week").into(),
        from: sunday.to_string(),
        through: saturday.to_string(),
        driver_id: s(row, "transporter_id").into(),
        name: text(&fields, "da_name").unwrap_or_default(),
        overall_score: number(&fields, "da_overall_score"),
        overall_tier: text(&fields, "da_overall_tier"),
        delivered: number(&fields, "delivered").map(|n| n as i64),
        metrics: METRICS
            .iter()
            .map(|(name, value, tier, score)| Metric {
                name: (*name).into(),
                value: number(&fields, value),
                tier: text(&fields, tier),
                score: number(&fields, score),
            })
            .filter(|m| m.value.is_some() || m.tier.is_some() || m.score.is_some())
            .collect(),
    })
}
/// A package of `kind`, from its row, its day in `tz`.
fn package(kind: PackageKind, row: &Value, tz: Tz) -> Result<Package> {
    let fields: Value = serde_json::from_str(s(row, "row"))?;
    let (date, reasons) = match kind {
        // Its `data_date` is the day the concession was recorded, often days after the delivery,
        // which Amazon times in UTC.
        PackageKind::Concession => (
            DateTime::parse_from_rfc3339(s(&fields, "delivery_datetime_iso"))
                .ok()
                .map(|at| at.with_timezone(&tz).date_naive().to_string()),
            marked(&fields, CONCESSION_FLAGS)
                .map(|field| field.trim_end_matches("_flag").to_owned())
                .collect(),
        ),
        PackageKind::Return => (
            day(&fields["delivery_planned_date"]),
            text(&fields, "rts_reason_code").into_iter().collect(),
        ),
        // Amazon writes when it was delivered in the station's own time, with no offset.
        PackageKind::Feedback => (
            day(&fields["delivery_time"]),
            marked(&fields, FEEDBACK_ANSWERS)
                .map(str::to_owned)
                .collect(),
        ),
    };
    Ok(Package {
        kind,
        week: s(row, "week").into(),
        date,
        tracking_id: s(row, "tracking_id").into(),
        driver_id: s(row, "transporter_id").into(),
        impacting: row["impact"].as_i64().map(|impact| impact == 1),
        reasons,
    })
}
/// The fields among `keys` that `fields` marks as set.
fn marked(fields: &Value, keys: &'static [&'static str]) -> impl Iterator<Item = &'static str> {
    keys.iter()
        .copied()
        .filter(move |key| flag(&fields[*key]) == Some(true))
}
/// The rows of `table` naming any of `ids` in `weeks`: only the active, verified publications
/// at the DSP's station, as its weeks show them.
fn rows(
    db: &Store,
    dsp: &str,
    table: &str,
    ids: &[String],
    weeks: &[String],
) -> Result<Vec<Value>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let station = db.profile(dsp)?.station_code;
    db.weekly_scorecard_db(dsp)?.all(
        &format!(
            "SELECT p.week,d.transporter_id,d.tracking_id,d.impact,d.row FROM {table} d \
             JOIN weekly_scorecard_publications p ON p.id=d.publication_id \
             AND p.active=1 AND p.scope_verified=1 \
             WHERE p.station=?1 AND p.week IN (SELECT value FROM json_each(?2)) \
             AND d.transporter_id IN (SELECT value FROM json_each(?3)) ORDER BY p.week,d.row_index"
        ),
        [
            station,
            serde_json::to_string(weeks)?,
            serde_json::to_string(ids)?,
        ],
    )
}
/// The scorecard weeks overlapping `from` to `to`, refusing bad days and IDs.
fn overlapping(ids: &[String], from: &str, to: &str) -> Result<Vec<String>> {
    let (first, last) = validate::days(from, to, LONGEST)?;
    validate::ids(ids, MOST_IDS)?;
    let mut sunday = first - Duration::days(first.weekday().num_days_from_sunday().into());
    let mut weeks = Vec::new();
    while sunday <= last {
        let iso = (sunday + Duration::days(6)).iso_week();
        weeks.push(format!("{}-W{:02}", iso.year(), iso.week()));
        sunday += Duration::weeks(1);
    }
    Ok(weeks)
}
/// A number Amazon writes as a number or as text.
fn number(row: &Value, key: &str) -> Option<f64> {
    match row.get(key)? {
        Value::Number(value) => value.as_f64(),
        Value::String(value) => value.trim().parse().ok().filter(|n: &f64| n.is_finite()),
        _ => None,
    }
}
/// The day a field starts with, when it's YYYY-MM-DD.
fn day(value: &Value) -> Option<String> {
    let day = value.as_str()?.get(..10)?;
    validate::date(day).is_ok().then(|| day.to_owned())
}

#[cfg(test)]
#[path = "../tests/backend/exposed.rs"]
mod tests;
