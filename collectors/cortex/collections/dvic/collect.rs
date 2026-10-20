use super::capture as dvic;
use super::capture::{Capture, Collection, KnownReport, Report, Request};
use crate as cortex;
use crate::{
    connection::{
        Driver,
        performance::{Api, Found, mismatched},
    },
    discovery::Scope,
};
use chrono::NaiveDate;
use dispatch_core::{
    Error, Result,
    collection::{
        browser::{
            Run,
            http::{FIXTURE, Http, Refusal},
        },
        metrics::Recorder,
    },
    db::now,
    ensure,
    manifest::registry,
};
use futures_util::{StreamExt, stream};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

pub const REPORT_HOST: &str = "flex-peer-performance-reports-prod-usamazon.s3.amazonaws.com";

struct Listed {
    name: String,
    path: String,
    url: String,
    week: String,
    date: String,
    modified_at: i64,
}
/// S3 pre-signed downloads use a separate client with no Amazon cookies or referer.
/// Redirects and every host other than the observed report host fail.
fn reports(origin: &str) -> Result<Http> {
    let url = url::Url::parse(origin).map_err(|_| Error::new("egress_denied", 403))?;
    if url.host_str() != Some(FIXTURE) {
        ensure(
            url.origin().ascii_serialization() == format!("https://{}", cortex::HOST),
            "egress_denied",
            403,
        )?;
    }
    Http::detached(origin, &cortex::REPORT_HOSTS)
}
fn refusal(error: Refusal) -> Error {
    match error {
        Refusal::Unavailable => Error::new("provider_unavailable", 502),
        Refusal::Unreadable("http_signed_out") => Error::new("verification_required", 409),
        Refusal::Unreadable("http_download_expired") => Error::new("dvic_source_changed", 502),
        Refusal::Unreadable(_) => Error::new(cortex::codes::DVIC_SOURCE_UNREADABLE.as_str(), 502),
    }
}
fn listing(
    value: Value,
    week: &str,
    dsp: &str,
    station: &str,
    origin: &str,
    metrics: &Recorder,
) -> Result<Vec<Listed>> {
    let invalid = || Error::new("dvic_listing_invalid", 502);
    let tables = value
        .get("tableData")
        .and_then(Value::as_object)
        .ok_or_else(invalid)?;
    // Amazon omits a dataset when this week has no published supplementary reports.
    let Some(table) = tables.get(dvic::DATASET) else {
        return Ok(Vec::new());
    };
    let rows = table
        .get("rows")
        .and_then(Value::as_array)
        .ok_or_else(invalid)?;
    ensure(rows.len() <= 1000, "dvic_source_too_large", 502)?;
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    for raw in rows {
        let row = if let Some(text) = raw.as_str() {
            serde_json::from_str(text).map_err(|_| invalid())?
        } else {
            raw.clone()
        };
        let name = row
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(invalid)?;
        if !name.contains("_DVIC_") {
            continue;
        }
        // Another DVIC report, or the pre-trip one under an older name: neither is read.
        if !name.ends_with(dvic::REPORT_SUFFIX) {
            metrics.detail("dvic_report_skipped");
            continue;
        }
        ensure(
            row["type"] == "xlsx" && row["date"] == week,
            "dvic_listing_invalid",
            502,
        )?;
        let target = row
            .get("downloadUrl")
            .and_then(Value::as_str)
            .ok_or_else(invalid)?;
        let url = url::Url::parse(target).map_err(|_| invalid())?;
        let fixture = url::Url::parse(origin).is_ok_and(|base| {
            base.host_str() == Some("fixture.dispatch.invalid") && base.origin() == url.origin()
        });
        ensure(
            fixture
                || (url.scheme() == "https"
                    && url.host_str() == Some(REPORT_HOST)
                    && url.port_or_known_default() == Some(443)),
            "dvic_scope_mismatch",
            502,
        )?;
        ensure(
            url.username().is_empty() && url.password().is_none() && url.fragment().is_none(),
            "dvic_scope_mismatch",
            502,
        )?;
        let date = dvic::report_identity(name, url.path(), week, dsp, station)?;
        let created = row
            .get("creationDate")
            .and_then(Value::as_array)
            .filter(|v| v.len() == 6)
            .ok_or_else(invalid)?;
        let parts: Vec<i64> = created
            .iter()
            .map(|v| v.as_i64().ok_or_else(invalid))
            .collect::<Result<_>>()?;
        let converted = parts
            .iter()
            .map(|n| u32::try_from(*n).map_err(|_| invalid()))
            .collect::<Result<Vec<_>>>()?;
        let modified_at = NaiveDate::from_ymd_opt(
            i32::try_from(parts[0]).map_err(|_| invalid())?,
            converted[1],
            converted[2],
        )
        .and_then(|date| date.and_hms_opt(converted[3], converted[4], converted[5]))
        .ok_or_else(invalid)?
        .and_utc()
        .timestamp_millis();
        ensure(
            seen.insert(url.path().to_owned()),
            "dvic_listing_invalid",
            502,
        )?;
        found.push(Listed {
            name: name.into(),
            path: url.path().into(),
            url: target.into(),
            week: week.into(),
            date: date.to_string(),
            modified_at,
        });
    }
    ensure(found.len() <= 14, "dvic_source_too_large", 502)?;
    Ok(found)
}

/// Reports downloaded at once: two weeks' fourteen downloaded fourteen at a time in about
/// a third of the time four at a time took.
const DOWNLOADS: usize = 14;

/// One week's listing, as the overview page asks for it.
async fn list_week(
    http: &Http,
    api: &Api,
    station: &str,
    week: &str,
    origin: &str,
    metrics: &Recorder,
) -> Result<Vec<Listed>> {
    let mut url = url::Url::parse(&format!("{}/getData", api.base))
        .map_err(|_| Error::new("dvic_listing_invalid", 502))?;
    url.query_pairs_mut()
        .append_pair("dataSetId", dvic::DATASET)
        .append_pair("dsp", &api.dsp)
        .append_pair("station", station)
        .append_pair("timeFrame", "Weekly")
        .append_pair("from", week)
        .append_pair("to", week);
    let data = http
        .json(
            url.as_str(),
            &format!("{origin}/performance"),
            2 * 1024 * 1024,
        )
        .await
        .map_err(refusal)?;
    listing(data, week, &api.dsp, station, origin, metrics)
}
/// One listed report, downloaded only when DVIC lacks it or it changed, and read.
async fn report(
    downloads: &Http,
    file: &Listed,
    prior: Option<&KnownReport>,
    dsp: &str,
    station: &str,
) -> Result<Report> {
    let download = downloads
        .download(
            &file.url,
            prior.and_then(|r| r.etag.as_deref()),
            dvic::MAX_FILE_BYTES,
        )
        .await
        .map_err(refusal)?;
    let (sha256, body, etag, modified_at) = if let Some(bytes) = download.body {
        let sha256 = dvic::hash(&bytes);
        let (dsp, station) = (dsp.to_owned(), station.to_owned());
        let records =
            tokio::task::spawn_blocking(move || dvic::xlsx::parse(&bytes, &dsp, &station))
                .await
                .map_err(|_| Error::new("dvic_workbook_invalid", 502))??;
        (
            sha256,
            Some(records),
            download.etag,
            download.modified_at.unwrap_or(file.modified_at),
        )
    } else {
        let prior = prior
            .filter(|p| p.etag.is_some())
            .ok_or_else(|| Error::new("dvic_cache_changed", 409))?;
        ensure(
            download
                .etag
                .as_ref()
                .is_none_or(|e| Some(e) == prior.etag.as_ref()),
            "dvic_source_changed",
            502,
        )?;
        (
            prior.sha256.clone(),
            None,
            prior.etag.clone(),
            prior.modified_at,
        )
    };
    Ok(Report {
        name: file.name.clone(),
        source_key: file.path.clone(),
        week: file.week.clone(),
        report_date: file.date.clone(),
        modified_at,
        etag,
        sha256,
        rows: body,
    })
}

impl Driver {
    pub(crate) async fn collect_dvic(
        &mut self,
        request: &Request,
        run: &Run<'_>,
    ) -> Result<(Capture, Scope)> {
        request.validate()?;
        run.progress(5, "Finding DVIC reports".into()).await?;
        // A kept scope whose reads named another station, company or DSP is found again.
        let (scope, found) = self.scope_for(&request.scope_request(), false, run).await?;
        match self.dvic_capture(request, &scope, run).await {
            Err(error) if found == Found::Known && mismatched(&error) => {
                run.metrics.detail("known_scope_mismatched");
                let _ = self.known.forget(&request.station);
                let (scope, _) = self.scope_for(&request.scope_request(), true, run).await?;
                self.dvic_capture(request, &scope, run).await
            }
            captured => captured,
        }
    }
    /// Every week's listing, read together, in the weeks' order.
    async fn dvic_listing(
        &self,
        http: &Http,
        api: &Api,
        request: &Request,
        run: &Run<'_>,
    ) -> Result<Vec<Listed>> {
        let weeks: Vec<_> = request
            .weeks
            .iter()
            .map(|week| list_week(http, api, &request.station, week, &self.origin, run.metrics))
            .collect();
        let lists: Vec<_> = stream::iter(weeks).buffered(DOWNLOADS).collect().await;
        let mut listed = Vec::new();
        for list in lists {
            listed.extend(list?);
        }
        Ok(listed)
    }
    async fn dvic_capture(
        &mut self,
        request: &Request,
        scope: &Scope,
        run: &Run<'_>,
    ) -> Result<(Capture, Scope)> {
        let started_at = now();
        let (mut api, mut kept) = match self.known_api(scope, &request.dsp_abbreviation) {
            Some(api) => (api, true),
            None => (
                self.performance_api(scope, &request.dsp_abbreviation, run.metrics)
                    .await?,
                false,
            ),
        };
        let http = Http::signed_in(&self.browser, &self.origin, &cortex::HOSTS).await?;
        let downloads = reports(&self.origin)?;
        let job = run.job.to_owned();
        let station = request.station.clone();
        let company = api.company_id.clone();
        // DVIC keeps the reports; only those it lacks or that changed are downloaded.
        let known = run
            .state
            .read(move |store| {
                let dsp = store.job_row(&job, None)?.dsp_id;
                let question = json!({"station":station,"company":company});
                registry()
                    .keeper(dvic::JOB_KIND)
                    .kept(store, &dsp, &question)
            })
            .await?;
        let known: HashMap<String, KnownReport> = serde_json::from_value(known)?;
        run.progress(10, "Checking DVIC reports".into()).await?;
        let listed = loop {
            match self.dvic_listing(&http, &api, request, run).await {
                Ok(listed) => break listed,
                // Nothing answers where the API last answered: the overview names it again.
                Err(error) if kept && error.is(cortex::codes::DVIC_SOURCE_UNREADABLE) => {
                    api = self
                        .performance_api(scope, &request.dsp_abbreviation, run.metrics)
                        .await?;
                    kept = false;
                }
                Err(error) => return Err(error),
            }
        };
        self.keep_api(scope, &api, run);
        // Nothing after the listing needs the browser: reports download without its session.
        self.browser.close().await;
        ensure(
            listed.len() <= dvic::MAX_REPORTS,
            "dvic_source_too_large",
            502,
        )?;
        let total = listed.len();
        run.progress(15, format!("Reading DVIC reports (0/{total})"))
            .await?;
        let reads: Vec<_> = listed
            .iter()
            .map(|file| {
                report(
                    &downloads,
                    file,
                    known.get(&file.path),
                    &api.dsp,
                    &request.station,
                )
            })
            .collect();
        let mut reads = stream::iter(reads).buffered(DOWNLOADS);
        // Publish only after every report has validated.
        let mut reports = Vec::new();
        let mut rows = 0;
        while let Some(report) = reads.next().await {
            let report = report?;
            rows += report.rows.as_ref().map_or(0, Vec::len);
            ensure(rows <= dvic::MAX_CAPTURE_ROWS, "dvic_source_too_large", 502)?;
            reports.push(report);
            if reports.len() % DOWNLOADS == 0 && reports.len() < total {
                run.progress(
                    15 + (75 * reports.len() / total) as i64,
                    format!("Reading DVIC reports ({}/{total})", reports.len()),
                )
                .await?;
            }
        }
        drop(reads);
        run.progress(95, "Validating DVIC inspections".into())
            .await?;
        let capture = Capture {
            version: 1,
            collection: Collection::Dvic,
            station: request.station.clone(),
            company_id: api.company_id,
            dsp_code: api.dsp,
            weeks: request.weeks.clone(),
            started_at,
            finished_at: now().max(started_at),
            reports,
        };
        capture.validate_scope(request, scope)?;
        Ok((capture, scope.clone()))
    }
}

#[cfg(test)]
#[path = "../../tests/backend/collections/dvic/collect.rs"]
mod tests;
