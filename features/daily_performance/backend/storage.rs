//! Atomic daily publication history, indexed independently from weekly data.
use super::settings::{COLLECTION, Policy};
use crate::api::types::{DailyPerformanceDataset, DailyPerformanceDay, DailyPerformanceSummary};
use dispatch_core::{
    Error, Result,
    collection::registry::AddedStorage,
    db::{Db, DspLease, Kind, Store, at, now, s},
    ensure,
    foundation::crypto,
    server::cache::DataDomain,
};
use dispatch_cortex::{
    daily_performance::{Capture, Collection, DATASETS, Days, MAX_DATES, Request},
    discovery::Scope,
};
use rusqlite::params;
use serde_json::{Value, json};
pub const DATABASE: Kind = Kind::new("daily_performance", 1);
pub const DOMAIN: DataDomain = DataDomain::new("daily_performance");
pub(crate) static STORAGE: AddedStorage = AddedStorage {
    id: "daily_performance",
    kind: DATABASE,
    marker: "storage.daily_performance",
    source: "daily_performance-v1",
    verify,
};
fn verify(db: &Db) -> Result<()> {
    for table in [
        "settings",
        "daily_publications",
        "daily_datasets",
        "daily_rows",
    ] {
        db.one(&format!("SELECT count(*) FROM {table} WHERE 0"), [])?;
    }
    Ok(())
}
pub(crate) fn today(store: &Store, dsp: &str) -> Result<chrono::NaiveDate> {
    let tz: chrono_tz::Tz = store
        .find_dsp(dsp)?
        .timezone
        .parse()
        .map_err(|_| Error::new("invalid_timezone", 400))?;
    Ok(dispatch_core::foundation::clock::today(tz))
}
pub(crate) fn bind(store: &Store, dsp: &str, dates: &[String]) -> Result<Value> {
    let tenant = store.find_dsp(dsp)?;
    let profile = store.profile(dsp)?;
    ensure(
        !profile.station_code.is_empty(),
        "daily_performance_station_required",
        409,
    )?;
    let request = Request {
        collection: Collection::DailyPerformance,
        dates: dates.to_vec(),
        station: profile.station_code,
        timezone: tenant.timezone,
        dsp_name: tenant.name,
        dsp_abbreviation: profile.abbreviation,
    };
    request.validate()?;
    Ok(serde_json::to_value(request)?)
}
/// A job's request for `dates`, oldest first, which `bind` completes when it runs.
pub(crate) fn request(store: &Store, dsp: &str, dates: &[String]) -> Result<Value> {
    let bound = bind(store, dsp, dates)?;
    Ok(json!({"collection":"daily_performance", "dates":dates, "station":bound["station"]}))
}
/// The days a job asks for: its `dates`, or the one `date` of a job queued before a job
/// read several.
pub(crate) fn requested_dates(request: &Value) -> Result<Vec<String>> {
    if let Some(date) = request.get("date") {
        let date = date
            .as_str()
            .ok_or_else(|| Error::new("invalid_input", 400))?;
        return Ok(vec![date.to_owned()]);
    }
    serde_json::from_value(request["dates"].clone()).map_err(|_| Error::new("invalid_input", 400))
}
pub(crate) fn text(row: &Value, key: &str) -> Option<String> {
    match row.get(key)? {
        Value::String(value) if !value.is_empty() => Some(value.chars().take(256).collect()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}
/// A yes or no, however Amazon writes it, as 1 or 0.
pub(crate) fn flag(value: &Value) -> Option<i64> {
    match value {
        Value::Bool(value) => Some(i64::from(*value)),
        Value::Number(value) => value.as_i64().filter(|v| [0, 1].contains(v)),
        Value::String(value) => match value.to_lowercase().as_str() {
            "y" | "yes" | "true" | "1" => Some(1),
            "n" | "no" | "false" | "0" => Some(0),
            _ => None,
        },
        _ => None,
    }
}
/// How long before a held read another must confirm what it saw.
const CONFIRM_AFTER_MS: i64 = 30 * 60_000;

/// Whether `capture`, a finished day, is held back rather than published: a dataset
/// answered with fewer rows than the day's active publication holds, which is what Cortex
/// answers while it republishes a day, and no read held at least half an hour before saw the
/// same counts for those datasets. A drop seen twice that far apart is what the day now is.
fn held(data: &DspLease<'_>, capture: &Capture, today: chrono::NaiveDate) -> Result<bool> {
    if capture.date >= today.to_string() {
        return Ok(false);
    }
    let Some(active) = data.one(
        "SELECT id,collected_at FROM daily_publications WHERE station=? AND date=? AND active=1",
        [&capture.station, &capture.date],
    )?
    else {
        return Ok(false);
    };
    let counts = |publication: &str| -> Result<std::collections::HashMap<String, i64>> {
        Ok(data
            .all(
                "SELECT dataset,row_count FROM daily_datasets WHERE publication_id=?",
                [publication],
            )?
            .iter()
            .map(|row| {
                (
                    s(row, "dataset").to_owned(),
                    row["row_count"].as_i64().unwrap_or(0),
                )
            })
            .collect())
    };
    let kept = counts(s(&active, "id"))?;
    let dropped: Vec<(&str, i64)> = DATASETS
        .iter()
        .zip(&capture.datasets)
        .map(|(dataset, captured)| (dataset.table, captured.rows.len() as i64))
        .filter(|(table, rows)| kept.get(*table).is_some_and(|kept| rows < kept))
        .collect();
    if dropped.is_empty() {
        return Ok(false);
    }
    let earlier = data.all(
        "SELECT id FROM daily_publications WHERE station=? AND date=? AND active=0          AND collected_at>? AND collected_at<=?",
        params![
            capture.station,
            capture.date,
            s(&active, "collected_at"),
            at(now() - CONFIRM_AFTER_MS)
        ],
    )?;
    for held in earlier {
        let seen = counts(s(&held, "id"))?;
        if dropped
            .iter()
            .all(|(table, rows)| seen.get(*table) == Some(rows))
        {
            return Ok(false);
        }
    }
    Ok(true)
}
/// One day of a job's collection, published as that day's active publication, or kept
/// beside it unpublished when held.
fn publish_day(data: &DspLease<'_>, job: &str, capture: &Capture, hold: bool) -> Result<()> {
    let publication = crypto::id("daily_performance")?;
    data.exec(
        "INSERT INTO daily_publications VALUES (?,?,?,?,?,?,?,?,0,?)",
        params![
            publication,
            job,
            capture.date,
            capture.station,
            capture.company_id,
            capture.dsp_code,
            at(capture.started_at),
            at(capture.finished_at),
            capture.row_count() as i64
        ],
    )?;
    for (dataset, captured) in DATASETS.iter().zip(&capture.datasets) {
        data.exec(
            "INSERT INTO daily_datasets VALUES (?,?,?,?,?)",
            params![
                publication,
                dataset.table,
                captured.source_url,
                captured.rows.len() as i64,
                if captured.rows.is_empty() {
                    "unconfirmed"
                } else {
                    "observed"
                }
            ],
        )?;
        for (index, row) in captured.rows.iter().enumerate() {
            let impact = dataset.impact.and_then(|field| flag(&row[field]));
            data.exec(
                "INSERT INTO daily_rows(publication_id,dataset,row_index,date,transporter_id,tracking_id,event_id,impact,row) \
                 VALUES (?,?,?,?,?,?,?,?,?)",
                params![
                    publication,
                    dataset.table,
                    index as i64,
                    capture.date,
                    text(row, "transporter_id"),
                    text(row, "tracking_id"),
                    text(row, "event_id"),
                    impact,
                    row.to_string()
                ],
            )?;
        }
    }
    if hold {
        return Ok(());
    }
    data.exec(
        "UPDATE daily_publications SET active=0 WHERE station=? AND date=? AND active=1",
        params![capture.station, capture.date],
    )?;
    data.exec(
        "UPDATE daily_publications SET active=1 WHERE id=?",
        [&publication],
    )?;
    Ok(())
}
pub trait DailyPerformanceStore {
    fn daily_performance_db(&self, dsp: &str) -> Result<DspLease<'_>>;
    fn daily_performance_days(&self, dsp: &str) -> Result<DailyPerformanceSummary>;
    fn daily_performance_jobs(&self, dsp: &str) -> Result<Vec<(String, Value)>>;
    fn enqueue_daily_performance(
        &self,
        dsp: &str,
        actor: Option<&str>,
        key: &str,
        from: &str,
        to: &str,
    ) -> Result<Value>;
    fn publish_daily_performance(
        &self,
        dsp: &str,
        job: &str,
        days: &Days,
        scope: &Scope,
    ) -> Result<()>;
}
impl DailyPerformanceStore for Store {
    fn daily_performance_db(&self, dsp: &str) -> Result<DspLease<'_>> {
        self.added_storage(dsp, dispatch_cortex::PROVIDER, &STORAGE)
    }
    fn daily_performance_jobs(&self, dsp: &str) -> Result<Vec<(String, Value)>> {
        let policy: Policy = dispatch_core::settings::read(self, dsp, &COLLECTION)?;
        let station = self.profile(dsp)?.station_code;
        let yesterday = today(self, dsp)? - chrono::Duration::days(1);
        let data = self.daily_performance_db(dsp)?;
        let cutoff = at(now() - i64::from(policy.refresh_hours) * 3_600_000);
        // Every date due, oldest first, read by one job.
        let mut dates = Vec::new();
        for offset in (0..policy.lookback_days).rev() {
            let date = (yesterday - chrono::Duration::days(i64::from(offset))).to_string();
            if data.one("SELECT id FROM daily_publications WHERE station=? AND date=? AND active=1 AND collected_at>=?",
                [&station, &date, &cutoff])?.is_none() {
                dates.push(date);
            }
        }
        let (Some(first), Some(last)) = (dates.first(), dates.last()) else {
            return Ok(Vec::new());
        };
        Ok(vec![(
            format!("daily_performance:{first}:{last}"),
            request(self, dsp, &dates)?,
        )])
    }
    fn enqueue_daily_performance(
        &self,
        dsp: &str,
        actor: Option<&str>,
        key: &str,
        from: &str,
        to: &str,
    ) -> Result<Value> {
        let parse = |value: &str| {
            chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .map_err(|_| Error::new("invalid_date", 400))
        };
        let (first, last) = (parse(from)?, parse(to)?);
        ensure(
            first <= last
                && (last - first).num_days() < MAX_DATES as i64
                && last <= today(self, dsp)?,
            "invalid_date_range",
            400,
        )?;
        // The whole range, one job.
        let dates: Vec<String> = first
            .iter_days()
            .take_while(|date| *date <= last)
            .map(|date| date.to_string())
            .collect();
        let jobs = [(
            format!("{key}:{from}:{to}"),
            dispatch_cortex::PROVIDER,
            request(self, dsp, &dates)?,
        )];
        Ok(serde_json::to_value(
            self.enqueue_batch(dsp, actor, &jobs)?,
        )?)
    }
    fn publish_daily_performance(
        &self,
        dsp: &str,
        job: &str,
        days: &Days,
        scope: &Scope,
    ) -> Result<()> {
        let job_request: Value = serde_json::from_str(&self.job_row(job, Some(dsp))?.request)?;
        let bound: Request =
            serde_json::from_value(bind(self, dsp, &requested_dates(&job_request)?)?)?;
        ensure(
            job_request["station"] == bound.station,
            "daily_performance_scope_mismatch",
            409,
        )?;
        days.validate_scope(&bound, scope)?;
        let today = today(self, dsp)?;
        let data = self.daily_performance_db(dsp)?;
        // Each day its own publication, all of them together.
        data.transaction(|| {
            for capture in &days.days {
                if let Some(existing) = data.one(
                    "SELECT station,company_id FROM daily_publications WHERE job_id=? AND date=?",
                    [job, &capture.date],
                )? {
                    ensure(
                        existing["station"] == capture.station
                            && existing["company_id"] == capture.company_id,
                        "daily_performance_publication_conflict",
                        503,
                    )?;
                    continue;
                }
                publish_day(&data, job, capture, held(&data, capture, today)?)?;
            }
            Ok(())
        })
    }
    fn daily_performance_days(&self, dsp: &str) -> Result<DailyPerformanceSummary> {
        let station = self.profile(dsp)?.station_code;
        let data = self.daily_performance_db(dsp)?;
        let rows = data.all("SELECT id,date,collected_at,row_count FROM daily_publications WHERE station=? AND active=1 ORDER BY date DESC",
            [&station])?;
        let mut days = Vec::new();
        for row in rows {
            let datasets = data.all("SELECT dataset,row_count,coverage FROM daily_datasets WHERE publication_id=? ORDER BY dataset",
                [s(&row,"id")])?.iter().map(|source| Ok(DailyPerformanceDataset {
                    id: dispatch_cortex::daily_performance::dataset(s(source,"dataset"))
                        .ok_or_else(|| Error::new("daily_performance_unknown_stored_dataset",503))?.id.into(),
                    dataset: s(source,"dataset").into(), rows: source["row_count"].as_u64().unwrap_or(0) as usize,
                    coverage: s(source,"coverage").into(),
                })).collect::<Result<Vec<_>>>()?;
            days.push(DailyPerformanceDay {
                date: s(&row, "date").into(),
                collected_at: s(&row, "collected_at").into(),
                row_count: row["row_count"].as_u64().unwrap_or(0) as usize,
                datasets,
            });
        }
        Ok(DailyPerformanceSummary {
            station,
            latest_date: (today(self, dsp)? - chrono::Duration::days(1)).to_string(),
            days,
        })
    }
}
