//! Reads the roster over plain HTTP with the browser's session: the company the DSP's code
//! names, from the list the start's session check read when it had one, then every associate
//! in one request and everyone offboarded in another, then the photos the DSP lacks, each from
//! where the application redirects its address, asked at the account's gate, shrunk as it
//! comes; one due a refresh only if Amazon changed it. Nothing after the roster needs a page,
//! so the browser closes as soon as it is in.
use super::capture::{
    Associate, Capture, Collection, JOB_KIND, KeptPhotos, MAX_ASSOCIATES, MAX_PHOTO_BYTES, Photo,
    Request,
};
use super::shrink::shrink;
use crate as cortex;
use crate::connection::{
    Driver,
    companies::{COMPANIES, company, refusal},
    gate::{self, Turn},
};
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
use std::{collections::BTreeSet, sync::Mutex};
use tokio::sync::Semaphore;

/// Where S3 serves the photos the application redirects to.
pub const PHOTO_HOST: &str = "amazon-flex-da-photo-prod-usamazon.s3.amazonaws.com";
/// Where the application sends the photo of an associate Amazon holds none of: its
/// placeholder, on Amazon's image host.
const PLACEHOLDER_HOST: &str = "m.media-amazon.com";
const PLACEHOLDER: &str = "/associate_photo_placeholder.png";
const ROSTER: &str = "/workforce/api/v1/fetchDSPAssociates";
/// As much as one reply may hold. A hundred associates are 60 KB.
const LIMIT: usize = 16 * 1024 * 1024;
/// Photo downloads at once, bounding the requests a roster holds in flight; the redirects
/// among them go to Cortex no more at once than the account's gate admits.
const LANES: usize = 16;
/// What one collection keeps of the photos the DSP lacks, as the capture holds them: about
/// 3,600 shrunk ones. A roster lacking more brings the rest next time.
const PHOTOS_BUDGET: usize = 96 * 1024 * 1024;
/// Photos shrunk at once: each holds a CPU for a moment, and a collection shares the server
/// with everyone using it.
const SHRINKING: usize = 4;

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

/// One of the roster's lists, asked for once `turn` admits it.
async fn list(http: &Http, turn: &Turn, address: &str, referer: &str) -> Result<Value> {
    turn.ask(ROSTER, || http.json_with_status(address, referer, LIMIT))
        .await?
        .map_err(|error| refusal(error.refusal))
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
    /// Amazon serves no photo at this address: its placeholder.
    Unserved(&'static str),
    /// Cortex refused this address with a 403, which `Refusals` tells apart from a session
    /// Amazon ended.
    Forbidden,
}

/// The addresses Cortex refuses with a 403, counted as its redirects answer. Amazon refuses
/// some offboarded associates' addresses alone, but a session it ended refuses every one: as
/// many refused in a row as are asked at once are that. Any other answer from Cortex makes
/// the refusals before it their addresses' own.
#[derive(Default)]
struct Refusals(Mutex<Refused>);
#[derive(Default)]
struct Refused {
    /// Refused in a row since Cortex last answered otherwise.
    run: Vec<String>,
    /// Refused with Cortex answering otherwise after.
    alone: Vec<String>,
    /// A run long enough to be a session Amazon ended.
    ended: bool,
}
impl Refusals {
    /// Cortex answered for an address otherwise than with a 403.
    fn answered(&self) {
        let mut refused = self.0.lock().expect("photo refusals");
        let run = std::mem::take(&mut refused.run);
        refused.alone.extend(run);
    }
    /// Cortex refused `address` with a 403: false once that ends the session.
    fn refused(&self, address: &str) -> bool {
        let mut refused = self.0.lock().expect("photo refusals");
        if refused.ended {
            return false;
        }
        refused.run.push(address.to_owned());
        if refused.run.len() >= LANES {
            refused.ended = true;
            refused.run.clear();
        }
        !refused.ended
    }
    /// The addresses Cortex refused alone; with `whole`, a read that ran to its end or to the
    /// photos it can hold, the last run of refusals too, which no session's end followed.
    fn alone(self, whole: bool) -> Vec<String> {
        let refused = self.0.into_inner().expect("photo refusals");
        let mut alone = refused.alone;
        if whole && !refused.ended {
            alone.extend(refused.run);
        }
        alone
    }
}

/// What the photos asked for came to, as they answer.
#[derive(Default)]
struct Tally {
    photos: Vec<(usize, Photo)>,
    held: usize,
    unchanged: Vec<String>,
    unserved: Vec<String>,
    /// Whether a refusal ended the read.
    refused: bool,
}
impl Tally {
    /// Counts the answer for `address`, the `index`th asked for: false once no more are asked,
    /// for a session Amazon refuses or the photos the capture can hold.
    fn add(
        &mut self,
        index: usize,
        address: &str,
        fetched: std::result::Result<Option<Photo>, Missed>,
        metrics: &Recorder,
    ) -> bool {
        match fetched {
            Ok(Some(photo)) => {
                self.held += photo.data.len();
                self.photos.push((index, photo));
            }
            Ok(None) => self.unchanged.push(address.to_owned()),
            Err(Missed::Unserved(label)) => {
                metrics.detail(label);
                self.unserved.push(address.to_owned());
            }
            // Its address is the refusals' to keep or not.
            Err(Missed::Forbidden) => metrics.detail("photo_forbidden"),
            Err(Missed::Skipped(label)) => metrics.detail(label),
            Err(Missed::Refused(label)) => {
                metrics.detail(label);
                self.refused = true;
                return false;
            }
        }
        self.held < PHOTOS_BUDGET
    }
}
/// The photos to ask for: active associates' first, then the rest, each address once, with
/// the ETag of the version kept when it is due a refresh. None that stands, nor an
/// offboarded associate's Amazon lately served nothing at.
fn wanted<'a>(
    associates: &'a [Associate],
    kept: &'a KeptPhotos,
) -> Vec<(&'a str, Option<&'a str>)> {
    let mut wanted: Vec<(&str, Option<&str>)> = Vec::new();
    for active in [true, false] {
        for associate in associates {
            if let Some(address) = &associate.photo
                && (associate.status == "ACTIVE") == active
                && !kept.standing.contains(address)
                // An address Amazon lately served nothing at waits only while the roster lists
                // whoever it names as offboarded.
                && !(associate.status == "OFFBOARDED" && kept.unserved.contains(address))
                && !wanted.iter().any(|(wanted, _)| wanted == address)
            {
                wanted.push((address, kept.versions.get(address).map(String::as_str)));
            }
        }
    }
    wanted
}
/// One photo's bytes and the ETag they were served under, or None when the photo host still
/// serves the version `etag` names: where the application redirects its address, asked at
/// the account's gate and again once a throttle has cooled while `turn` lasts, then the photo
/// from there, which carries none of Amazon's cookies.
async fn photo(
    http: &Http,
    downloads: &Http,
    turn: &Turn,
    refusals: &Refusals,
    origin: &str,
    referer: &str,
    address: &str,
    etag: Option<&str>,
) -> std::result::Result<Option<(Vec<u8>, Option<String>)>, Missed> {
    let url = format!("{origin}{address}");
    let answered = turn
        .ask("/workforce photo", || http.location(&url, referer))
        .await
        .map_err(|_| Missed::Refused("photo_throttled"))?;
    let source = match answered {
        Err(error) if error.status == Some(403) => {
            return Err(match refusals.refused(address) {
                true => Missed::Forbidden,
                false => Missed::Refused("http_signed_out"),
            });
        }
        Err(error) => {
            return Err(match error.refusal {
                Refusal::Unavailable => Missed::Refused("photo_unavailable"),
                Refusal::Unreadable(label @ "http_signed_out") => Missed::Refused(label),
                Refusal::Unreadable(label) => {
                    refusals.answered();
                    Missed::Skipped(label)
                }
            });
        }
        Ok(source) => {
            refusals.answered();
            source
        }
    };
    if source.host_str() == Some(PLACEHOLDER_HOST) && source.path().ends_with(PLACEHOLDER) {
        return Err(Missed::Unserved("photo_placeholder"));
    }
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
        let http = self.http().await?;
        let companies = match self.companies.take() {
            Some(companies) => companies,
            None => http
                .json(
                    &format!("{}{COMPANIES}", self.origin),
                    &format!("{}/workforce", self.origin),
                    LIMIT,
                )
                .await
                .map_err(refusal)?,
        };
        let (company_id, company_code) = company(&companies, &request.dsp_abbreviation)?;
        run.progress(30, "Reading the roster".into()).await?;
        let referer = page(
            &self.origin,
            &company_id,
            "da_console_associates",
            "da-console-associates-tab",
        );
        let offboarded_referer = page(
            &self.origin,
            &company_id,
            "da_console_offboarded",
            "da-console-offboarded-tab",
        );
        let turn = Turn::of(&self.origin, &company_id, gate::job(run));
        // Who is listed and who was offboarded, asked for together.
        let listed = roster_address(&self.origin, &company_id, "ACTIVE,INACTIVE");
        let offboarded = roster_address(&self.origin, &company_id, "OFFBOARDED");
        let (listed, offboarded) = tokio::try_join!(
            list(&http, &turn, &listed, &referer),
            list(&http, &turn, &offboarded, &offboarded_referer)
        )?;
        // It exits while the photos come, rather than holding them up.
        self.stop_browser();
        let associates = everyone(roster(&listed)?, rows(&offboarded)?)?;
        let kept = kept_photos(run, &company_id).await?;
        let wanted = wanted(&associates, &kept);
        let total = wanted.len();
        if total > 0 {
            run.progress(50, format!("Downloading photos (0/{total})"))
                .await?;
        }
        let downloads = Http::detached(&self.origin, &cortex::PHOTO_HOSTS)?;
        let shrinking = Semaphore::new(SHRINKING);
        let refusals = Refusals::default();
        let (http, downloads, turn, shrinking, refused) =
            (&http, &downloads, &turn, &shrinking, &refusals);
        let (origin, referer) = (self.origin.as_str(), referer.as_str());
        let mut fetching = stream::iter(0..total)
            .map(|index| {
                let (address, etag) = wanted[index];
                async move {
                    let fetched = async {
                        let Some((bytes, etag)) = photo(
                            http, downloads, turn, refused, origin, referer, address, etag,
                        )
                        .await?
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
        let (mut tally, mut done) = (Tally::default(), 0);
        while let Some((index, fetched)) = fetching.next().await {
            done += 1;
            if !tally.add(index, wanted[index].0, fetched, run.metrics) {
                break;
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
        if done == total {
            turn.finished();
        }
        let Tally {
            mut photos,
            unchanged,
            mut unserved,
            refused,
            ..
        } = tally;
        unserved.extend(refusals.alone(!refused));
        photos.sort_by_key(|(index, _)| *index);
        run.progress(95, "Validating the roster".into()).await?;
        let capture = Capture {
            version: 1,
            collection: Collection::Associates,
            company_id,
            company_code,
            started_at,
            finished_at: now().max(started_at),
            photos_waiting: total - photos.len() - unchanged.len() - unserved.len(),
            photos: photos.into_iter().map(|(_, photo)| photo).collect(),
            photos_unchanged: unchanged,
            photos_unserved: unserved,
            associates,
        };
        capture.validate(request)?;
        Ok(capture)
    }
}

#[cfg(test)]
#[path = "../../tests/backend/collections/associates/collect.rs"]
mod tests;
