//! Reads the roster over plain HTTP with the browser's session: the company the DSP's code
//! names, then every associate in one request and everyone offboarded in another, then the
//! photos the DSP lacks, each from where the application redirects its address, shrunk as it
//! comes; one due a refresh only if Amazon changed it. Nothing after the roster needs a page,
//! so the browser closes as soon as it is in.
use super::capture::{
    Associate, Capture, Collection, JOB_KIND, KeptPhotos, MAX_ASSOCIATES, MAX_PHOTO_BYTES, Photo,
    Request,
};
use super::shrink::shrink;
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
use futures_util::{StreamExt, stream};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use tokio::sync::Semaphore;

/// Where S3 serves the photos the application redirects to.
pub const PHOTO_HOST: &str = "amazon-flex-da-photo-prod-usamazon.s3.amazonaws.com";
pub(crate) const COMPANIES: &str = "/workforce/listCompanies";
const ROSTER: &str = "/workforce/api/v1/fetchDSPAssociates";
/// As much as one reply may hold. A hundred associates are 60 KB.
const LIMIT: usize = 16 * 1024 * 1024;
/// Photo downloads at once: forty took 4.4 s sixteen at a time, against 5.4 s eight at a
/// time and 4.2 s all forty together.
const LANES: usize = 16;
/// What one collection keeps of the photos the DSP lacks, as the capture holds them: about
/// 3,600 shrunk ones. A roster lacking more brings the rest next time.
const PHOTOS_BUDGET: usize = 96 * 1024 * 1024;
/// Photos shrunk at once: each holds a CPU for a moment, and a collection shares the server
/// with everyone using it.
const SHRINKING: usize = 4;

pub(crate) fn refusal(error: Refusal) -> Error {
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

/// Every associate the roster lists, from its table's rows: never no one.
pub(crate) fn roster(reply: &Value) -> Result<Vec<Associate>> {
    let associates = rows(reply)?;
    // A roster naming no one would say everyone left: it is a page that failed instead.
    ensure(
        !associates.is_empty(),
        cortex::codes::CORTEX_SOURCE_CHANGED.as_str(),
        502,
    )?;
    Ok(associates)
}
/// Everyone the roster lists, then whoever else Amazon offboarded: someone on both lists, as a
/// rehire, is as the roster lists them.
pub(crate) fn everyone(
    roster: Vec<Associate>,
    offboarded: Vec<Associate>,
) -> Result<Vec<Associate>> {
    let listed: BTreeSet<String> = roster.iter().map(|a| a.transporter_id.clone()).collect();
    let everyone: Vec<Associate> = roster
        .into_iter()
        .chain(
            offboarded
                .into_iter()
                .filter(|a| !listed.contains(&a.transporter_id)),
        )
        .collect();
    ensure(
        everyone.len() <= MAX_ASSOCIATES,
        "associates_source_too_large",
        502,
    )?;
    Ok(everyone)
}
/// The associates a reply's table lists, each row a JSON object or a JSON string holding one.
/// The same associate twice keeps the first.
pub(crate) fn rows(reply: &Value) -> Result<Vec<Associate>> {
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
    Ok(associates)
}

/// A page of the DA Console for `company`, by its page and tab, which the application's own
/// requests come from.
fn page(origin: &str, company: &str, page: &str, tab: &str) -> String {
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    query
        .append_pair("pageId", page)
        .append_pair("station", "ALL")
        .append_pair("companyId", company)
        .append_pair("tabId", tab);
    format!("{origin}/workforce?{}", query.finish())
}
/// The associates of `company` at every station with any of `statuses`: without them, Amazon
/// answers with the active alone.
fn roster_address(origin: &str, company: &str, statuses: &str) -> String {
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    query
        .append_pair("companyId", company)
        .append_pair("station", "ALL")
        .append_pair("operationalStatuses", statuses);
    format!("{origin}{ROSTER}?{}", query.finish())
}

/// The photos Driver Profiles holds for the company: only the rest are downloaded, and those
/// due a refresh only if Amazon changed them.
async fn kept_photos(run: &Run<'_>, company: &str) -> Result<KeptPhotos> {
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
        return Ok(KeptPhotos::default());
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
/// Why one photo wasn't kept, by a fixed label.
#[derive(Debug, PartialEq, Eq)]
enum Missed {
    /// The application refused the session or stopped answering, as it would for every photo
    /// after: no more are asked for.
    Refused(&'static str),
    /// This photo alone.
    Skipped(&'static str),
}
/// One photo's bytes and the ETag they were served under, or None when the photo host still
/// serves the version `etag` names: where the application redirects its address, then the
/// photo from there, which carries none of Amazon's cookies.
async fn photo(
    http: &Http,
    downloads: &Http,
    origin: &str,
    referer: &str,
    address: &str,
    etag: Option<&str>,
) -> std::result::Result<Option<(Vec<u8>, Option<String>)>, Missed> {
    let source = http
        .location(&format!("{origin}{address}"), referer)
        .await
        .map_err(|refusal| match refusal {
            Refusal::Unavailable => Missed::Refused("photo_unavailable"),
            Refusal::Unreadable(label @ "http_signed_out") => Missed::Refused(label),
            Refusal::Unreadable(label) => Missed::Skipped(label),
        })?;
    let fixture = url::Url::parse(origin)
        .is_ok_and(|base| base.host_str() == Some(FIXTURE) && base.origin() == source.origin());
    if !fixture && (source.scheme() != "https" || source.host_str() != Some(PHOTO_HOST)) {
        return Err(Missed::Skipped("photo_redirect_unexpected"));
    }
    let download = downloads
        .download(source.as_str(), etag, MAX_PHOTO_BYTES)
        .await
        .map_err(|refusal| Missed::Skipped(skipped(refusal)))?;
    match download.body {
        Some(bytes) => Ok(Some((bytes, download.etag))),
        None if etag.is_some() => Ok(None),
        None => Err(Missed::Skipped("photo_not_modified")),
    }
}
/// A photo downloaded, as it is kept: shrunk away from the threads that serve requests, a few at
/// a time.
async fn keep(
    shrinking: &Semaphore,
    address: &str,
    bytes: Vec<u8>,
    etag: Option<String>,
) -> std::result::Result<Photo, &'static str> {
    let _turn = shrinking.acquire().await.map_err(|_| "photo_unreadable")?;
    let bytes = tokio::task::spawn_blocking(move || shrink(bytes))
        .await
        .map_err(|_| "photo_unreadable")??;
    Photo::new(address, &bytes, etag).ok_or("photo_not_an_image")
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
        let referer = page(
            &self.origin,
            &company_id,
            "da_console_associates",
            "da-console-associates-tab",
        );
        let listed = http
            .json(
                &roster_address(&self.origin, &company_id, "ACTIVE,INACTIVE"),
                &referer,
                LIMIT,
            )
            .await
            .map_err(refusal)?;
        run.progress(40, "Reading who was offboarded".into())
            .await?;
        let offboarded = http
            .json(
                &roster_address(&self.origin, &company_id, "OFFBOARDED"),
                &page(
                    &self.origin,
                    &company_id,
                    "da_console_offboarded",
                    "da-console-offboarded-tab",
                ),
                LIMIT,
            )
            .await
            .map_err(refusal)?;
        self.browser.close().await;
        let associates = everyone(roster(&listed)?, rows(&offboarded)?)?;
        let kept = kept_photos(run, &company_id).await?;
        // Active associates' photos first, then the rest; each address once, with the ETag of
        // the version kept when it is due a refresh.
        let mut wanted: Vec<(&str, Option<&str>)> = Vec::new();
        for active in [true, false] {
            for associate in &associates {
                if let Some(address) = &associate.photo
                    && (associate.status == "ACTIVE") == active
                    && !kept.standing.contains(address)
                    && !wanted.iter().any(|(wanted, _)| wanted == address)
                {
                    wanted.push((address, kept.versions.get(address).map(String::as_str)));
                }
            }
        }
        let total = wanted.len();
        if total > 0 {
            run.progress(50, format!("Downloading photos (0/{total})"))
                .await?;
        }
        let downloads = Http::detached(&self.origin, &cortex::PHOTO_HOSTS)?;
        let shrinking = Semaphore::new(SHRINKING);
        let (http, downloads, shrinking) = (&http, &downloads, &shrinking);
        let (origin, referer) = (self.origin.as_str(), referer.as_str());
        let mut fetching = stream::iter(0..total)
            .map(|index| {
                let (address, etag) = wanted[index];
                async move {
                    let fetched = async {
                        let Some((bytes, etag)) =
                            photo(http, downloads, origin, referer, address, etag).await?
                        else {
                            return Ok(None);
                        };
                        let photo = keep(shrinking, address, bytes, etag).await;
                        photo.map(Some).map_err(Missed::Skipped)
                    };
                    (index, fetched.await)
                }
            })
            .buffer_unordered(LANES);
        let (mut photos, mut unchanged, mut held, mut done) = (Vec::new(), Vec::new(), 0, 0);
        while held < PHOTOS_BUDGET
            && let Some((index, fetched)) = fetching.next().await
        {
            done += 1;
            match fetched {
                Ok(Some(photo)) => {
                    held += photo.data.len();
                    photos.push((index, photo));
                }
                Ok(None) => unchanged.push(wanted[index].0.to_owned()),
                Err(Missed::Skipped(label)) => run.metrics.detail(label),
                Err(Missed::Refused(label)) => {
                    run.metrics.detail(label);
                    break;
                }
            }
            if done % LANES == 0 && done < total {
                run.progress(
                    50 + (45 * done / total) as i64,
                    format!("Downloading photos ({done}/{total})"),
                )
                .await?;
            }
        }
        drop(fetching);
        photos.sort_by_key(|(index, _)| *index);
        run.progress(95, "Validating the roster".into()).await?;
        let capture = Capture {
            version: 1,
            collection: Collection::Associates,
            company_id,
            company_code,
            started_at,
            finished_at: now().max(started_at),
            photos_waiting: total - photos.len() - unchanged.len(),
            photos: photos.into_iter().map(|(_, photo)| photo).collect(),
            photos_unchanged: unchanged,
            associates,
        };
        capture.validate(request)?;
        Ok(capture)
    }
}

#[cfg(test)]
#[path = "../../tests/backend/collections/associates/collect.rs"]
mod tests;
