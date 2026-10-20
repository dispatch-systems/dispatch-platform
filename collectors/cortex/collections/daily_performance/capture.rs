//! Captures of the days a collection was asked for, each exactly one date, with source scope
//! verified before publication.
use super::DATASETS;
use crate::{
    discovery::{CollectionRequest, Discovery, Scope},
    performance::{DatasetCapture, MAX_ROWS},
};
use dispatch_core::{Error, Result, db::now, ensure, foundation::validate::token};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
pub const JOB_KIND: &str = "cortex.daily_performance.collect";
/// As many days as one collection reads: a month.
pub const MAX_DATES: usize = 31;
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Collection {
    DailyPerformance,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub collection: Collection,
    /// The days read, oldest first, each once.
    pub dates: Vec<String>,
    pub station: String,
    pub timezone: String,
    pub dsp_name: String,
    pub dsp_abbreviation: String,
}
impl Request {
    pub fn is(value: &Value) -> bool {
        value["collection"] == "daily_performance"
    }
    pub fn parse(value: &Value) -> Result<Option<Self>> {
        if !Self::is(value) {
            return Ok(None);
        }
        let request: Self =
            serde_json::from_value(value.clone()).map_err(|_| Error::new("invalid_input", 400))?;
        request.validate()?;
        Ok(Some(request))
    }
    /// The newest day the collection reads, which discovery resolves the station on.
    fn last(&self) -> &str {
        self.dates.last().map_or("", String::as_str)
    }
    pub fn scope_request(&self) -> CollectionRequest {
        CollectionRequest::Discover(Discovery {
            date: self.last().to_owned(),
            station: self.station.clone(),
            timezone: self.timezone.clone(),
            dsp_name: self.dsp_name.clone(),
            dsp_abbreviation: self.dsp_abbreviation.clone(),
        })
    }
    pub fn validate(&self) -> Result<()> {
        ensure(
            (1..=MAX_DATES).contains(&self.dates.len()),
            "invalid_date_range",
            400,
        )?;
        let mut previous = None;
        for text in &self.dates {
            let date = chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d")
                .map_err(|_| Error::new("invalid_date", 400))?;
            ensure(&date.to_string() == text, "invalid_date", 400)?;
            ensure(previous < Some(date), "invalid_date_range", 400)?;
            previous = Some(date);
        }
        let CollectionRequest::Discover(discovery) = self.scope_request() else {
            unreachable!()
        };
        self.scope_request()
            .validate_scope(&discovery.scope("discovery", "discovery")?)
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Capture {
    pub version: u32,
    pub collection: Collection,
    pub date: String,
    pub station: String,
    pub company_id: String,
    pub dsp_code: String,
    pub started_at: i64,
    pub finished_at: i64,
    pub datasets: Vec<DatasetCapture>,
}
/// What a collection read: each day it was asked for, oldest first.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Days {
    pub days: Vec<Capture>,
}
impl Days {
    /// Each day the request asked for, once and in its order, each valid for `scope`.
    pub fn validate_scope(&self, request: &Request, scope: &Scope) -> Result<()> {
        ensure(
            self.days.len() == request.dates.len()
                && self
                    .days
                    .iter()
                    .zip(&request.dates)
                    .all(|(day, date)| &day.date == date),
            "daily_performance_capture_invalid",
            502,
        )?;
        for day in &self.days {
            day.validate_scope(request, scope)?;
        }
        Ok(())
    }
    pub fn row_count(&self) -> usize {
        self.days.iter().map(Capture::row_count).sum()
    }
}
impl Capture {
    pub fn validate(&self, request: &Request) -> Result<()> {
        request.validate()?;
        ensure(
            self.version == 1 && self.started_at > 0 && self.finished_at >= self.started_at,
            "daily_performance_capture_invalid",
            502,
        )?;
        ensure(
            request.dates.contains(&self.date)
                && self.station == request.station
                && token(&self.company_id, 128)
                && token(&self.dsp_code, 32)
                && self
                    .dsp_code
                    .eq_ignore_ascii_case(&request.dsp_abbreviation),
            "daily_performance_scope_mismatch",
            502,
        )?;
        ensure(
            self.datasets.len() == DATASETS.len(),
            "daily_performance_capture_invalid",
            502,
        )?;
        for (dataset, captured) in DATASETS.iter().zip(&self.datasets) {
            ensure(
                captured.id == dataset.id && captured.from == self.date && captured.to == self.date,
                "daily_performance_capture_invalid",
                502,
            )?;
            ensure(
                captured.rows.len() <= MAX_ROWS
                    && captured.source_url.len() <= 2048
                    && captured.rows.iter().all(Value::is_object),
                "daily_performance_capture_invalid",
                502,
            )?;
            for row in &captured.rows {
                for field in ["station_code", "station"] {
                    if let Some(station) = row[field].as_str().filter(|value| !value.is_empty()) {
                        ensure(
                            station == self.station,
                            "daily_performance_scope_mismatch",
                            502,
                        )?;
                    }
                }
                if let Some(company) = row["company_id"].as_str().filter(|value| !value.is_empty())
                {
                    ensure(
                        company == self.company_id,
                        "daily_performance_scope_mismatch",
                        502,
                    )?;
                }
                if let Some(code) = row["dsp_code"].as_str().filter(|value| !value.is_empty()) {
                    ensure(
                        code.eq_ignore_ascii_case(&self.dsp_code),
                        "daily_performance_scope_mismatch",
                        502,
                    )?;
                }
                if dataset.station
                    && let Some(date) = row["data_date"].as_str()
                {
                    ensure(
                        date.get(..10) == Some(self.date.as_str()),
                        "daily_performance_scope_mismatch",
                        502,
                    )?;
                }
            }
        }
        Ok(())
    }
    pub fn validate_scope(&self, request: &Request, scope: &Scope) -> Result<()> {
        self.validate(request)?;
        request.scope_request().validate_scope(scope)?;
        ensure(
            self.company_id == scope.provider,
            "daily_performance_scope_mismatch",
            502,
        )
    }
    pub fn row_count(&self) -> usize {
        self.datasets.iter().map(|dataset| dataset.rows.len()).sum()
    }
}
/// What fixture mode collects: every day asked for, each a small, complete day.
pub fn fixture(request: &Request) -> Result<Days> {
    request.validate()?;
    let days = Days {
        days: request
            .dates
            .iter()
            .map(|date| fixture_day(request, date))
            .collect::<Result<_>>()?,
    };
    Ok(days)
}
fn fixture_day(request: &Request, date: &str) -> Result<Capture> {
    let started_at = now();
    let datasets = DATASETS
        .iter()
        .map(|dataset| {
            let common = json!({"data_date":date, "station_code":request.station,
            "company_id":"company-fixture", "dsp_code":request.dsp_abbreviation});
            let mut rows = Vec::new();
            match dataset.table {
                "driver_quality" | "driver_safety" | "driver_feedback" | "driver_returns" => {
                    for index in 1..=3 {
                        let mut row = common.clone();
                        row["transporter_id"] = json!(format!("driver-{index}"));
                        row["da_name"] = json!(format!("Fixture Driver {index}"));
                        row["delivered"] = json!(100 + index);
                        row["pod_success_rate"] = json!(98.5);
                        row["negative_response_cnt"] = json!(index - 1);
                        row["positive_response_cnt"] = json!(2);
                        row["driver_mishandled_package_cnt"] = json!(index - 1);
                        row["speeding_events"] = json!(index - 1);
                        rows.push(row);
                    }
                }
                "returns_to_station" => {
                    let mut row = common.clone();
                    row["transporter_id"] = json!("driver-1");
                    row["tracking_id"] = json!("TBA000000000001");
                    row["rts_reason_code"] = json!("BUSINESS CLOSED");
                    row["impacting_dcr"] = json!("Y");
                    row["daily_coaching"] = json!("Contact customer: call or text");
                    row["daily_exemption"] = json!(0);
                    rows.push(row);
                }
                "safety_events" | "live_safety_events" => {
                    let mut row = common.clone();
                    row["transporter_id"] = json!("driver-3");
                    row["event_id"] = json!("event-overlap");
                    row["dashboard_metric_type"] = json!("Speeding");
                    row["oss_impact_flag"] = json!(1);
                    rows.push(row);
                }
                name if name.starts_with("dsp_")
                    && !name.ends_with("_batch")
                    && !name.ends_with("_thresholds") =>
                {
                    rows.push(common);
                }
                _ => {}
            }
            DatasetCapture {
                id: dataset.id.into(),
                from: date.into(),
                to: date.into(),
                source_url: format!(
                    "fixture://performance/api/v1/getData?dataSetId={}",
                    dataset.id
                ),
                rows,
            }
        })
        .collect();
    let capture = Capture {
        version: 1,
        collection: Collection::DailyPerformance,
        date: date.into(),
        station: request.station.clone(),
        company_id: "company-fixture".into(),
        dsp_code: request.dsp_abbreviation.clone(),
        started_at,
        finished_at: now().max(started_at),
        datasets,
    };
    capture.validate(request)?;
    Ok(capture)
}
