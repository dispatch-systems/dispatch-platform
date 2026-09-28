use super::*;
use crate::{
    browsers::http::{Http, Refusal},
    db::now,
    dvic::{self, Capture, Collection, Report, Request},
};
use chrono::NaiveDate;
use std::collections::HashSet;

struct Listed {
    name: String,
    path: String,
    url: String,
    week: String,
    date: String,
    modified_at: i64,
}
fn refusal(error: Refusal) -> Error {
    match error {
        Refusal::Unavailable => Error::new("provider_unavailable", 502),
        Refusal::Unreadable("http_signed_out") => Error::new("verification_required", 409),
        Refusal::Unreadable("http_download_expired") => Error::new("dvic_source_changed", 502),
        Refusal::Unreadable(_) => Error::new("dvic_source_unreadable", 502),
    }
}
fn listing(
    value: Value,
    week: &str,
    dsp: &str,
    station: &str,
    origin: &str,
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
                    && url.host_str() == Some(dvic::REPORT_HOST)
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

impl Driver {
    pub(super) async fn collect_dvic(
        &mut self,
        request: &Request,
        run: &Run<'_>,
    ) -> Result<Capture> {
        request.validate()?;
        let started_at = now();
        run.progress(5, "Finding DVIC reports".into()).await?;
        let api = self.performance_api(&request.station, run.metrics).await?;
        let http = Http::signed_in(&self.browser, &self.origin).await?;
        let downloads = Http::cortex_reports(&self.origin)?;
        let job = run.job.to_owned();
        let station = request.station.clone();
        let company = api.company_id.clone();
        let known = run
            .state
            .run(move |store| {
                let dsp = store.job_row(&job, None)?.dsp_id;
                store.dvic_known(&dsp, &station, &company)
            })
            .await?;
        let mut listed = Vec::new();
        for week in &request.weeks {
            run.progress(10, format!("Checking DVIC reports for {week}"))
                .await?;
            let mut url = url::Url::parse(&format!("{}/getData", api.base))
                .map_err(|_| Error::new("dvic_listing_invalid", 502))?;
            url.query_pairs_mut()
                .append_pair("dataSetId", dvic::DATASET)
                .append_pair("dsp", &api.dsp)
                .append_pair("station", &request.station)
                .append_pair("timeFrame", "Weekly")
                .append_pair("from", week)
                .append_pair("to", week);
            let data = http
                .json(
                    url.as_str(),
                    &format!("{}/performance", self.origin),
                    2 * 1024 * 1024,
                )
                .await
                .map_err(refusal)?;
            listed.extend(listing(
                data,
                week,
                &api.dsp,
                &request.station,
                &self.origin,
            )?);
        }
        ensure(
            listed.len() <= dvic::MAX_REPORTS,
            "dvic_source_too_large",
            502,
        )?;
        let mut reports = Vec::new();
        let total = listed.len();
        let mut rows = 0;
        // Four downloads per batch; publish only after every report has validated.
        for batch in listed.chunks(4) {
            run.progress(
                15 + (75 * reports.len() / total.max(1)) as i64,
                format!("Reading DVIC reports ({}/{total})", reports.len()),
            )
            .await?;
            let reads = futures_util::future::join_all(batch.iter().map(|file| async {
                let prior = known.get(&file.path);
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
                    let dsp = api.dsp.clone();
                    let station = request.station.clone();
                    let records = tokio::task::spawn_blocking(move || {
                        dvic::xlsx::parse(&bytes, &dsp, &station)
                    })
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
                Ok::<_, Error>(Report {
                    name: file.name.clone(),
                    source_key: file.path.clone(),
                    week: file.week.clone(),
                    report_date: file.date.clone(),
                    modified_at,
                    etag,
                    sha256,
                    rows: body,
                })
            }))
            .await;
            for report in reads {
                let report = report?;
                rows += report.rows.as_ref().map_or(0, Vec::len);
                ensure(rows <= dvic::MAX_CAPTURE_ROWS, "dvic_source_too_large", 502)?;
                reports.push(report);
            }
        }
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
        capture.validate(request)?;
        Ok(capture)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn listings_use_publication_dates_and_reject_foreign_hosts_paths_and_duplicate_objects() {
        let capture = dvic::fixture(&Request {
            collection: Collection::Dvic,
            station: "TST1".into(),
            weeks: vec!["2026-W39".into()],
        })
        .unwrap();
        let report = &capture.reports[0];
        let row = json!({"name":report.name,"type":"xlsx","date":"2026-W39",
            "downloadUrl":format!("https://{}{}?temporary=signature",dvic::REPORT_HOST,report.source_key),
            "creationDate":[2026,9,27,14,4,0]});
        let wrap = |rows: Value| json!({"tableData":{dvic::DATASET:{"rows":rows}}});
        let read = |value| {
            listing(
                value,
                "2026-W39",
                "FXTR",
                "TST1",
                "https://logistics.amazon.com",
            )
        };
        let parsed = read(wrap(json!([row]))).unwrap();
        assert_eq!(parsed[0].date, "2026-09-27");
        assert!(!parsed[0].path.contains('?'));
        assert_eq!(read(wrap(json!([row.to_string()]))).unwrap().len(), 1);
        assert!(read(wrap(json!([row, row]))).is_err());
        for target in [
            format!("https://evil.example{}", report.source_key),
            format!(
                "https://{}{}",
                dvic::REPORT_HOST,
                report.source_key.replace("/fxtr/", "/other/")
            ),
            format!("https://{}:8443{}", dvic::REPORT_HOST, report.source_key),
        ] {
            let mut wrong = row.clone();
            wrong["downloadUrl"] = json!(target);
            assert!(read(wrap(json!([wrong]))).is_err());
        }
        assert!(read(json!({"tableData":{}})).unwrap().is_empty());
        assert!(read(json!({"message":"signed out"})).is_err());
    }
}
