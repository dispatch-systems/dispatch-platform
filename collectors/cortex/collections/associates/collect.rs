//! Reads the roster over plain HTTP with the browser's session: the company the DSP's code
//! names, then every associate in one request, then the photos the DSP lacks, each from where
//! the application redirects its address. Nothing after the roster needs a page, so the
//! browser closes as soon as it is in.
use super::capture::{
    Associate, Capture, Collection, JOB_KIND, MAX_ASSOCIATES, MAX_PHOTO_BYTES, Photo, Request,
};
use crate as cortex;
use crate::connection::Driver;
use dispatch_core::{
    Error, Result,
    collection::browser::{
        Run,
        http::{FIXTURE, Http, Refusal},
    },
    db::now,
    ensure,
    foundation::validate::token,
    manifest::registry,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Where S3 serves the photos the application redirects to.
pub const PHOTO_HOST: &str = "amazon-flex-da-photo-prod-usamazon.s3.amazonaws.com";
pub(crate) const COMPANIES: &str = "/workforce/listCompanies";
const ROSTER: &str = "/workforce/api/v1/fetchDSPAssociates";
/// As much as one reply may hold. A hundred associates are 60 KB.
const LIMIT: usize = 16 * 1024 * 1024;
/// Photo downloads at once: twenty took no longer than one.
const LANES: usize = 8;
/// What one collection downloads of the photos the DSP lacks, about sixty of Amazon's; the
/// next brings the rest.
const PHOTOS_BUDGET: usize = 96 * 1024 * 1024;

fn refusal(error: Refusal) -> Error {
    match error {
        Refusal::Unavailable => Error::new("provider_unavailable", 502),
        Refusal::Unreadable("http_signed_out") => Error::new("verification_required", 409),
        Refusal::Unreadable(_) => Error::new(cortex::codes::CORTEX_SOURCE_CHANGED.as_str(), 502),
    }
}

/// The company whose short code is the DSP's, of those the sign-in sees.
pub(crate) fn company(reply: &Value, code: &str) -> Result<(String, String)> {
    let records = reply["records"]
        .as_array()
        .ok_or_else(|| Error::new(cortex::codes::CORTEX_SOURCE_CHANGED.as_str(), 502))?;
    let named: Vec<(String, String)> = records
        .iter()
        .filter_map(|record| {
            let id = record["companyId"].as_str()?;
            let short = record["shortCode"].as_str()?;
            (token(id, 128) && token(short, 32) && short.eq_ignore_ascii_case(code))
                .then(|| (id.to_owned(), short.to_owned()))
        })
        .collect();
    match named.as_slice() {
        [one] => Ok(one.clone()),
        [] => Err(Error::new(
            cortex::codes::CORTEX_SCOPE_MISMATCH.as_str(),
            502,
        )),
        _ => Err(Error::new(
            cortex::codes::CORTEX_PROVIDER_AMBIGUOUS.as_str(),
            502,
        )),
    }
}

/// Every associate the roster lists, from its table's rows, each a JSON object or a JSON
/// string holding one. The same associate twice keeps the first.
pub(crate) fn roster(reply: &Value) -> Result<Vec<Associate>> {
    let changed = || Error::new(cortex::codes::CORTEX_SOURCE_CHANGED.as_str(), 502);
    let rows = reply["tableData"]["dsp-associates-table-data"]["rows"]
        .as_array()
        .ok_or_else(changed)?;
    ensure(
        rows.len() <= MAX_ASSOCIATES,
        "associates_source_too_large",
        502,
    )?;
    let mut seen = BTreeSet::new();
    let mut associates = Vec::new();
    for row in rows {
        let row = match row {
            Value::String(text) => serde_json::from_str(text).map_err(|_| changed())?,
            other => other.clone(),
        };
        if let Some(associate) = Associate::read(row)?
            && seen.insert(associate.transporter_id.clone())
        {
            associates.push(associate);
        }
    }
    // A roster naming no one would say everyone left: it is a page that failed instead.
    ensure(
        !associates.is_empty(),
        cortex::codes::CORTEX_SOURCE_CHANGED.as_str(),
        502,
    )?;
    Ok(associates)
}

/// The associates page for `company`, which the application's own requests come from.
fn page(origin: &str, company: &str) -> String {
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    query
        .append_pair("pageId", "da_console_associates")
        .append_pair("station", "ALL")
        .append_pair("companyId", company)
        .append_pair("tabId", "da-console-associates-tab");
    format!("{origin}/workforce?{}", query.finish())
}
/// The roster of `company` at every station, active and inactive alike: without the
/// statuses, Amazon answers with the active alone.
fn roster_address(origin: &str, company: &str) -> String {
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    query
        .append_pair("companyId", company)
        .append_pair("station", "ALL")
        .append_pair("operationalStatuses", "ACTIVE,INACTIVE");
    format!("{origin}{ROSTER}?{}", query.finish())
}

/// The photos Driver Profiles holds for the company, by address: only the rest are
/// downloaded.
async fn kept_photos(run: &Run<'_>, company: &str) -> Result<BTreeSet<String>> {
    let job = run.job.to_owned();
    let question = json!({"company": company});
    let kept = run
        .state
        .read(move |store| {
            let dsp = store.job_row(&job, None)?.dsp_id;
            registry().keeper(JOB_KIND).kept(store, &dsp, &question)
        })
        .await?;
    if kept.is_null() {
        return Ok(BTreeSet::new());
    }
    Ok(serde_json::from_value(kept)?)
}

/// Why one photo was not downloaded, by a fixed label.
fn skipped(refusal: Refusal) -> &'static str {
    match refusal {
        Refusal::Unavailable => "photo_unavailable",
        Refusal::Unreadable(label) => label,
    }
}
/// One photo: where the application redirects its address, then the bytes from there, which
/// carry none of Amazon's cookies.
async fn photo(
    http: &Http,
    downloads: &Http,
    origin: &str,
    referer: &str,
    address: &str,
) -> std::result::Result<Photo, &'static str> {
    let source = http
        .location(&format!("{origin}{address}"), referer)
        .await
        .map_err(skipped)?;
    let fixture = url::Url::parse(origin)
        .is_ok_and(|base| base.host_str() == Some(FIXTURE) && base.origin() == source.origin());
    if !fixture && (source.scheme() != "https" || source.host_str() != Some(PHOTO_HOST)) {
        return Err("photo_redirect_unexpected");
    }
    let bytes = downloads
        .download(source.as_str(), None, MAX_PHOTO_BYTES)
        .await
        .map_err(skipped)?
        .body
        .ok_or("photo_not_modified")?;
    Photo::new(address, &bytes).ok_or("photo_not_an_image")
}

impl Driver {
    pub(crate) async fn collect_associates(
        &mut self,
        request: &Request,
        run: &Run<'_>,
    ) -> Result<Capture> {
        request.validate()?;
        let started_at = now();
        run.progress(15, "Finding the DSP's company".into()).await?;
        let http = Http::signed_in(&self.browser, &self.origin, &cortex::HOSTS).await?;
        let companies = http
            .json(
                &format!("{}{COMPANIES}", self.origin),
                &format!("{}/workforce", self.origin),
                LIMIT,
            )
            .await
            .map_err(refusal)?;
        let (company_id, company_code) = company(&companies, &request.dsp_abbreviation)?;
        run.progress(30, "Reading the roster".into()).await?;
        let referer = page(&self.origin, &company_id);
        let reply = http
            .json(&roster_address(&self.origin, &company_id), &referer, LIMIT)
            .await
            .map_err(refusal)?;
        self.browser.close().await;
        let associates = roster(&reply)?;
        let kept = kept_photos(run, &company_id).await?;
        // Active associates' photos first, then the rest; each address once.
        let mut wanted: Vec<&str> = Vec::new();
        for active in [true, false] {
            for associate in &associates {
                if let Some(address) = &associate.photo
                    && (associate.status == "ACTIVE") == active
                    && !kept.contains(address)
                    && !wanted.contains(&address.as_str())
                {
                    wanted.push(address);
                }
            }
        }
        if !wanted.is_empty() {
            run.progress(50, format!("Downloading {} photos", wanted.len()))
                .await?;
        }
        let downloads = Http::detached(&self.origin, &cortex::PHOTO_HOSTS)?;
        let next = AtomicUsize::new(0);
        let spent = AtomicUsize::new(0);
        let lanes = futures_util::future::join_all((0..LANES).map(|_| async {
            let mut got = Vec::new();
            while spent.load(Ordering::SeqCst) < PHOTOS_BUDGET {
                let index = next.fetch_add(1, Ordering::SeqCst);
                let Some(address) = wanted.get(index) else {
                    break;
                };
                match photo(&http, &downloads, &self.origin, &referer, address).await {
                    Ok(photo) => {
                        spent.fetch_add(photo.data.len() / 4 * 3, Ordering::SeqCst);
                        got.push((index, photo));
                    }
                    Err(label) => run.metrics.detail(label),
                }
            }
            got
        }))
        .await;
        let mut photos: Vec<(usize, Photo)> = lanes.into_iter().flatten().collect();
        photos.sort_by_key(|(index, _)| *index);
        run.progress(95, "Validating the roster".into()).await?;
        let capture = Capture {
            version: 1,
            collection: Collection::Associates,
            company_id,
            company_code,
            started_at,
            finished_at: now().max(started_at),
            photos_waiting: wanted.len() - photos.len(),
            photos: photos.into_iter().map(|(_, photo)| photo).collect(),
            associates,
        };
        capture.validate(request)?;
        Ok(capture)
    }
}

#[cfg(test)]
#[path = "../../tests/backend/collections/associates/collect.rs"]
mod tests;
