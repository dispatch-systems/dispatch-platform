//! Short pre-trip inspections from Cortex's rolling supplementary workbooks.
//! Publication weeks are ISO Monday–Sunday; inspection dates come from the rows.
pub mod hidden;
mod storage;
pub mod xlsx;

use crate::{
    Error, Result,
    collectors::AddedStorage,
    db::{Db, Kind},
    ensure,
    meals::{CollectionRequest as ScopeRequest, Discovery, Scope},
    scorecard, validate,
};
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub const JOB_KIND: &str = "cortex.dvic.collect";
pub const MAX_WEEKS: usize = 26;
pub const MAX_REPORTS: usize = MAX_WEEKS * 14;
pub const MAX_ROWS: usize = 10_000;
pub const MAX_CAPTURE_ROWS: usize = 100_000;
pub const MAX_FILE_BYTES: usize = 8 * 1024 * 1024;
pub const REPORT_HOST: &str = "flex-peer-performance-reports-prod-usamazon.s3.amazonaws.com";
pub const DATASET: &str = "dsp_station_weekly_supp_reports";
pub static STORAGE: AddedStorage = AddedStorage {
    id: "dvic",
    kind: Kind::Dvic,
    marker: "storage.dvic",
    source: "dvic-v1",
    verify,
};
fn verify(db: &Db) -> Result<()> {
    for table in [
        "dvic_reports",
        "dvic_revisions",
        "dvic_inspections",
        "dvic_weeks",
        "dvic_runs",
        "dvic_hidden_drivers",
    ] {
        db.one(&format!("SELECT count(*) FROM {table} WHERE 0"), [])?;
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub enum Collection {
    #[serde(rename = "dvic")]
    Dvic,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub collection: Collection,
    pub station: String,
    pub weeks: Vec<String>,
    pub date: String,
    pub timezone: String,
    pub dsp_name: String,
    pub dsp_abbreviation: String,
}
impl Request {
    pub fn is(value: &Value) -> bool {
        value.get("collection") == Some(&json!("dvic"))
    }
    pub fn parse(value: &Value) -> Result<Option<Self>> {
        if !Self::is(value) {
            return Ok(None);
        }
        let request: Self = serde_json::from_value(value.clone())
            .map_err(|_| Error::new("invalid_dvic_request", 400))?;
        request.validate()?;
        Ok(Some(request))
    }
    pub fn validate(&self) -> Result<()> {
        ensure(
            station_code(&self.station)
                && scorecard::token(&self.dsp_abbreviation, 32)
                && !self.dsp_name.trim().is_empty()
                && self.dsp_name.len() <= 256
                && !self.dsp_name.chars().any(char::is_control),
            "invalid_station_code",
            400,
        )?;
        ensure(
            (1..=MAX_WEEKS).contains(&self.weeks.len()),
            "invalid_dvic_request",
            400,
        )?;
        let mut seen = HashSet::new();
        for week in &self.weeks {
            scorecard::parse_week(week)?;
            ensure(seen.insert(week), "invalid_dvic_request", 400)?;
        }
        self.scope_request()
            .validate_scope(&self.discovery().scope("discovery", "discovery")?)
    }
    fn discovery(&self) -> Discovery {
        Discovery {
            date: self.date.clone(),
            station: self.station.clone(),
            timezone: self.timezone.clone(),
            dsp_name: self.dsp_name.clone(),
            dsp_abbreviation: self.dsp_abbreviation.clone(),
        }
    }
    pub fn scope_request(&self) -> ScopeRequest {
        ScopeRequest::Discover(self.discovery())
    }
}
fn station_code(station: &str) -> bool {
    (3..=8).contains(&station.len())
        && station
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
pub fn report_week(day: NaiveDate) -> String {
    let week = day.iso_week();
    format!("{}-W{:02}", week.year(), week.week())
}
pub fn weeks_ending(week: &str, count: usize) -> Result<Vec<String>> {
    ensure(
        (1..=MAX_WEEKS).contains(&count),
        "invalid_dvic_request",
        400,
    )?;
    scorecard::weeks_before(week, count - 1)
}

/// No timezone is invented for the provider's offset-free timestamps.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Inspection {
    pub start_date: String,
    pub dsp: String,
    pub station: String,
    pub transporter_id: String,
    pub transporter_name: String,
    pub vin: String,
    pub fleet_type: String,
    pub inspection_type: String,
    pub inspection_status: String,
    pub start_time: String,
    pub end_time: String,
    pub duration: f64,
}
impl Inspection {
    pub fn minimum_seconds(&self) -> Result<i64> {
        match self.fleet_type.as_str() {
            "CV" | "CDV" => Ok(90),
            "SV" => Ok(300),
            _ => Err(Error::new("dvic_fleet_type_unknown", 502)),
        }
    }
    pub fn is_short(&self) -> Result<bool> {
        Ok(self.duration < self.minimum_seconds()? as f64)
    }
    pub fn key(&self) -> String {
        let start = timestamp(&self.start_time)
            .map(|t| t.format("%Y-%m-%d %H:%M:%S%.f").to_string())
            .unwrap_or_else(|_| self.start_time.clone());
        // Length-delimited JSON prevents ambiguous concatenation and ignores mutable names/durations.
        hash(
            serde_json::to_string(&[
                &self.dsp,
                &self.station,
                &self.transporter_id,
                &self.vin,
                &self.inspection_type,
                &start,
            ])
            .expect("strings serialize")
            .as_bytes(),
        )
    }
    pub fn validate(&self, dsp: &str, station: &str) -> Result<()> {
        ensure(
            self.dsp == dsp && self.station == station,
            "dvic_scope_mismatch",
            502,
        )?;
        for value in [&self.transporter_id, &self.vin, &self.inspection_status] {
            ensure(scorecard::token(value, 128), "dvic_row_invalid", 502)?;
        }
        ensure(
            !self.transporter_name.is_empty()
                && self.transporter_name.len() <= 512
                && !self.transporter_name.chars().any(char::is_control),
            "dvic_row_invalid",
            502,
        )?;
        ensure(
            self.inspection_type == "PRE_TRIP_DVIC",
            "dvic_row_invalid",
            502,
        )?;
        self.minimum_seconds()?;
        let start = timestamp(&self.start_time)?;
        let end = timestamp(&self.end_time)?;
        let elapsed = (end - start).num_milliseconds() as f64 / 1000.0;
        ensure(
            self.start_date == start.date().to_string()
                && self.duration.is_finite()
                && (0.0..=86400.0).contains(&self.duration)
                && elapsed >= 0.0
                && (elapsed - self.duration).abs() < 1.0,
            "dvic_duration_invalid",
            502,
        )
    }
}
fn timestamp(value: &str) -> Result<NaiveDateTime> {
    ensure(value.len() <= 32, "dvic_row_invalid", 502)?;
    NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f")
        .map_err(|_| Error::new("dvic_row_invalid", 502))
}
pub fn hash(bytes: &[u8]) -> String {
    crate::crypto::hex(&Sha256::digest(bytes))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Report {
    pub name: String,
    pub source_key: String,
    pub week: String,
    pub report_date: String,
    /// Source modification time, in UTC milliseconds (never ingestion order).
    pub modified_at: i64,
    pub etag: Option<String>,
    pub sha256: String,
    /// None means the server returned 304 against a known, successfully imported revision.
    pub rows: Option<Vec<Inspection>>,
}
impl Report {
    pub fn validate(&self, dsp: &str, station: &str) -> Result<()> {
        let date = report_identity(&self.name, &self.source_key, &self.week, dsp, station)?;
        ensure(
            date.to_string() == self.report_date && self.modified_at > 0,
            "dvic_report_invalid",
            502,
        )?;
        ensure(
            self.sha256.len() == 64 && self.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
            "dvic_report_invalid",
            502,
        )?;
        if let Some(etag) = &self.etag {
            ensure(
                etag.len() <= 256 && !etag.chars().any(char::is_control),
                "dvic_report_invalid",
                502,
            )?;
        }
        if let Some(rows) = &self.rows {
            ensure(rows.len() <= MAX_ROWS, "dvic_source_too_large", 502)?;
            let mut keys = HashSet::new();
            for row in rows {
                row.validate(dsp, station)?;
                ensure(keys.insert(row.key()), "dvic_duplicate_inspection", 502)?;
                // The observed eight-date window is a discovery aid, never a row filter.
            }
        }
        Ok(())
    }
}
/// Validate an observed object's full scope before any download; never persist its signed query.
pub fn report_identity(
    name: &str,
    path: &str,
    week: &str,
    dsp: &str,
    station: &str,
) -> Result<NaiveDate> {
    let (year, number) = scorecard::parse_week(week)?;
    ensure(
        scorecard::token(dsp, 32) && station_code(station),
        "dvic_scope_mismatch",
        502,
    )?;
    let prefix = format!("US_{dsp}_{station}_{year}_week-{number}_");
    let date = name
        .strip_prefix(&prefix)
        .and_then(|s| s.strip_suffix("_DVIC_PreTrip_u90s-NonDOT_u300s-DOT_last7days.xlsx"))
        .ok_or_else(|| Error::new("dvic_report_invalid", 502))?;
    ensure(
        date.len() == 8 && date.bytes().all(|b| b.is_ascii_digit()),
        "dvic_report_invalid",
        502,
    )?;
    let date = NaiveDate::parse_from_str(date, "%Y%m%d")
        .map_err(|_| Error::new("dvic_report_invalid", 502))?;
    ensure(
        report_week(date) == week
            && path
                == format!(
                    "/us/{}/{}/{year}/week-{number}/{name}",
                    dsp.to_ascii_lowercase(),
                    station.to_ascii_lowercase()
                ),
        "dvic_scope_mismatch",
        502,
    )?;
    Ok(date)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Capture {
    pub version: u32,
    pub collection: Collection,
    pub station: String,
    pub company_id: String,
    pub dsp_code: String,
    pub weeks: Vec<String>,
    pub started_at: i64,
    pub finished_at: i64,
    pub reports: Vec<Report>,
}
impl Capture {
    pub fn validate(&self, request: &Request) -> Result<()> {
        request.validate()?;
        ensure(
            self.version == 1
                && self.station == request.station
                && self.weeks == request.weeks
                && scorecard::token(&self.company_id, 128)
                && scorecard::token(&self.dsp_code, 32)
                && self
                    .dsp_code
                    .eq_ignore_ascii_case(&request.dsp_abbreviation)
                && self.started_at > 0
                && self.finished_at >= self.started_at,
            "dvic_capture_invalid",
            502,
        )?;
        ensure(
            self.reports.len() <= MAX_REPORTS
                && self
                    .reports
                    .iter()
                    .filter_map(|r| r.rows.as_ref())
                    .map(Vec::len)
                    .sum::<usize>()
                    <= MAX_CAPTURE_ROWS,
            "dvic_source_too_large",
            502,
        )?;
        let mut seen = HashSet::new();
        for report in &self.reports {
            ensure(
                self.weeks.contains(&report.week) && seen.insert(&report.source_key),
                "dvic_capture_invalid",
                502,
            )?;
            report.validate(&self.dsp_code, &self.station)?;
        }
        Ok(())
    }
    pub fn validate_scope(&self, request: &Request, scope: &Scope) -> Result<()> {
        self.validate(request)?;
        request.scope_request().validate_scope(scope)?;
        ensure(
            self.company_id == scope.provider,
            "dvic_scope_mismatch",
            502,
        )
    }
}

pub fn fixture(request: &Request) -> Result<Capture> {
    request.validate()?;
    let mut reports = Vec::new();
    for week in &request.weeks {
        let (_, saturday) = scorecard::week_days(week)?;
        let date = saturday + Duration::days(1);
        let (year, number) = scorecard::parse_week(week)?;
        let name = format!(
            "US_{}_{}_{year}_week-{number}_{}_DVIC_PreTrip_u90s-NonDOT_u300s-DOT_last7days.xlsx",
            request.dsp_abbreviation,
            request.station,
            date.format("%Y%m%d")
        );
        let source_key = format!(
            "/us/{}/{}/{year}/week-{number}/{name}",
            request.dsp_abbreviation.to_ascii_lowercase(),
            request.station.to_ascii_lowercase()
        );
        let day = saturday.to_string();
        let rows = vec![Inspection {
            start_date: day.clone(),
            dsp: request.dsp_abbreviation.clone(),
            station: request.station.clone(),
            transporter_id: "driver-1".into(),
            transporter_name: "Fixture Driver".into(),
            vin: "1FIXTURE000000001".into(),
            fleet_type: "CV".into(),
            inspection_type: "PRE_TRIP_DVIC".into(),
            inspection_status: "PASSED".into(),
            start_time: format!("{day} 09:00:00"),
            end_time: format!("{day} 09:01:15"),
            duration: 75.0,
        }];
        reports.push(Report {
            name,
            source_key,
            week: week.clone(),
            report_date: date.to_string(),
            modified_at: date
                .and_hms_opt(14, 4, 0)
                .unwrap()
                .and_utc()
                .timestamp_millis(),
            etag: Some("\"fixture-v1\"".into()),
            sha256: hash(serde_json::to_string(&rows)?.as_bytes()),
            rows: Some(rows),
        });
    }
    let now = crate::db::now();
    let capture = Capture {
        version: 1,
        collection: Collection::Dvic,
        station: request.station.clone(),
        company_id: "company-fixture".into(),
        dsp_code: request.dsp_abbreviation.clone(),
        weeks: request.weeks.clone(),
        started_at: now,
        finished_at: now,
        reports,
    };
    capture.validate(request)?;
    Ok(capture)
}
