//! Atomic daily publication history, indexed independently from weekly data.
use crate::api::types::{
    DailyPerformanceDataset, DailyPerformanceDay, DailyPerformancePolicy, DailyPerformanceSummary,
};
use dispatch_core::{
    Error, Result,
    collection::registry::AddedStorage,
    db::{Db, DspLease, Kind, Store, at, now, s},
    ensure,
    foundation::crypto,
    server::cache::DataDomain,
};
use dispatch_cortex::{
    daily_performance::{Capture, Collection, DATASETS, Request},
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
    Ok(chrono::Utc::now().with_timezone(&tz).date_naive())
}
pub(crate) fn bind(store: &Store, dsp: &str, date: &str) -> Result<Value> {
    let tenant = store.find_dsp(dsp)?;
    let profile = store.profile(dsp)?;
    ensure(
        !profile.station_code.is_empty(),
        "daily_performance_station_required",
        409,
    )?;
    let request = Request {
        collection: Collection::DailyPerformance,
        date: date.into(),
        station: profile.station_code,
        timezone: tenant.timezone,
        dsp_name: tenant.name,
        dsp_abbreviation: profile.abbreviation,
    };
    request.validate()?;
    Ok(serde_json::to_value(request)?)
}
pub(crate) fn request(store: &Store, dsp: &str, date: &str) -> Result<Value> {
    let bound = bind(store, dsp, date)?;
    Ok(json!({"collection":"daily_performance", "date":date, "station":bound["station"]}))
}
fn text(row: &Value, key: &str) -> Option<String> {
    match row.get(key)? {
        Value::String(value) if !value.is_empty() => Some(value.chars().take(256).collect()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}
pub trait DailyPerformanceStore {
    fn daily_performance_db(&self, dsp: &str) -> Result<DspLease<'_>>;
    fn daily_performance_days(&self, dsp: &str) -> Result<DailyPerformanceSummary>;
    fn daily_performance_policy(&self, dsp: &str) -> Result<DailyPerformancePolicy>;
    fn set_daily_performance_policy(
        &self,
        dsp: &str,
        policy: &DailyPerformancePolicy,
    ) -> Result<()>;
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
        capture: &Capture,
        scope: &Scope,
    ) -> Result<()>;
}
impl DailyPerformanceStore for Store {
    fn daily_performance_db(&self, dsp: &str) -> Result<DspLease<'_>> {
        self.added_storage(dsp, dispatch_cortex::PROVIDER, &STORAGE)
    }
    fn daily_performance_policy(&self, dsp: &str) -> Result<DailyPerformancePolicy> {
        Ok(serde_json::from_value(
            self.daily_performance_db(dsp)?.setting(
                "collection_policy",
                serde_json::to_value(DailyPerformancePolicy::default())?,
            )?,
        )?)
    }
    fn set_daily_performance_policy(
        &self,
        dsp: &str,
        policy: &DailyPerformancePolicy,
    ) -> Result<()> {
        ensure(
            (1..=30).contains(&policy.lookback_days) && (1..=168).contains(&policy.refresh_hours),
            "invalid_input",
            400,
        )?;
        self.daily_performance_db(dsp)?
            .set("collection_policy", &serde_json::to_value(policy)?)
    }
    fn daily_performance_jobs(&self, dsp: &str) -> Result<Vec<(String, Value)>> {
        let policy = self.daily_performance_policy(dsp)?;
        let station = self.profile(dsp)?.station_code;
        let yesterday = today(self, dsp)? - chrono::Duration::days(1);
        let data = self.daily_performance_db(dsp)?;
        let cutoff = at(now() - i64::from(policy.refresh_hours) * 3_600_000);
        let mut jobs = Vec::new();
        for offset in 0..policy.lookback_days {
            let date = (yesterday - chrono::Duration::days(i64::from(offset))).to_string();
            if data.one("SELECT id FROM daily_publications WHERE station=? AND date=? AND active=1 AND collected_at>=?",
                [&station, &date, &cutoff])?.is_none() {
                jobs.push((format!("daily_performance:{date}"), request(self, dsp, &date)?));
            }
        }
        Ok(jobs)
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
        let (mut date, last) = (parse(from)?, parse(to)?);
        ensure(
            date <= last && (last - date).num_days() < 31 && last <= today(self, dsp)?,
            "invalid_date_range",
            400,
        )?;
        let mut jobs = Vec::new();
        while date <= last {
            let date_text = date.to_string();
            jobs.push((
                format!("{key}:{date}"),
                dispatch_cortex::PROVIDER,
                request(self, dsp, &date_text)?,
            ));
            date += chrono::Duration::days(1);
        }
        Ok(serde_json::to_value(
            self.enqueue_batch(dsp, actor, &jobs)?,
        )?)
    }
    fn publish_daily_performance(
        &self,
        dsp: &str,
        job: &str,
        capture: &Capture,
        scope: &Scope,
    ) -> Result<()> {
        let job_request: Value = serde_json::from_str(&self.job_row(job, Some(dsp))?.request)?;
        let bound: Request = serde_json::from_value(bind(self, dsp, s(&job_request, "date"))?)?;
        ensure(
            job_request["station"] == bound.station,
            "daily_performance_scope_mismatch",
            409,
        )?;
        capture.validate_scope(&bound, scope)?;
        let data = self.daily_performance_db(dsp)?;
        data.transaction(|| {
            if let Some(existing) = data.one("SELECT date,station,company_id FROM daily_publications WHERE job_id=?", [job])? {
                ensure(existing["date"] == capture.date && existing["station"] == capture.station
                    && existing["company_id"] == capture.company_id,
                    "daily_performance_publication_conflict", 503)?;
                return Ok(());
            }
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
                    let impact = dataset.impact.and_then(|field| match &row[field] {
                        Value::Bool(value) => Some(i64::from(*value)),
                        Value::Number(value) => value.as_i64().filter(|v| [0, 1].contains(v)),
                        Value::String(value) => match value.to_lowercase().as_str() {
                            "y" | "yes" | "true" | "1" => Some(1),
                            "n" | "no" | "false" | "0" => Some(0),
                            _ => None,
                        },
                        _ => None,
                    });
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
            data.exec(
                "UPDATE daily_publications SET active=0 WHERE station=? AND date=? AND active=1",
                params![capture.station, capture.date],
            )?;
            data.exec(
                "UPDATE daily_publications SET active=1 WHERE id=?",
                [&publication],
            )?;
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
                [s(&row,"id")])?.iter().map(|source| DailyPerformanceDataset {
                    id: dispatch_cortex::daily_performance::dataset(s(source,"dataset")).expect("stored dataset").id.into(),
                    dataset: s(source,"dataset").into(), rows: source["row_count"].as_u64().unwrap_or(0) as usize,
                    coverage: s(source,"coverage").into(),
                }).collect();
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
