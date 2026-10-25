//! A day's routes read with no browser, from the session a browser signed in: every read
//! signed here as the application signs it, with the key the list's own page carries.
//!
//! Cortex signs each data request for its method, address, query, session and moment with
//! a key the list's page holds in its state (`hmacSession`). The page is read for it once,
//! and once more when Cortex refuses a signature made with it or keeps sending the session
//! to sign in; a page without a key, or one that sends the session to sign in itself, ends
//! the read. The lists come first, each asked again while Cortex is briefly unavailable or
//! redirects; then every itinerary the list names, a few at a time, each asked for gzipped,
//! decoded and checked for its driver once, away from the threads serving requests.
//! Whatever failed is asked once more. A day is read whole, both lists and every
//! itinerary, or not at all.
use super::capture::{
    Capture, Collection, ItineraryCapture, MAX_BODY, MAX_ITINERARIES, Mode, listed, token,
};
use super::collect::{CaptureBudget, ROUTE_SUMMARIES, SUMMARIES, itinerary_path, owned_by};
use crate as cortex;
use crate::discovery::Scope;
use base64::{Engine, engine::general_purpose::STANDARD};
use dispatch_core::{
    Error, Result,
    collection::browser::http::{Http, Refusal, SignedIn, Wire},
    db::now,
    ensure,
    foundation::clock,
};
use futures_util::stream::{self, StreamExt};
use hmac::{Hmac, KeyInit, Mac};
use serde_json::Value;
use std::time::Duration;

/// Itineraries downloaded at once, as many as the application's signatures are.
const DOWNLOADS: usize = 12;
/// Times a list is asked for while Cortex is briefly unavailable or redirects.
const LIST_ATTEMPTS: u32 = 3;
/// How long Cortex is left before a read is asked for again, times the attempts so far.
const RETRY_PAUSE: Duration = Duration::from_secs(2);
/// As much as the list's page may be. It was 727 KB.
const PAGE_LIMIT: usize = 8 * 1024 * 1024;
/// As long as the key or the session it names may be.
const KEY_BYTES: usize = 1024;
/// The application's name for itself, on every request it signs.
const USER_REF: &str = "cortex-webapp-user";

/// Why a day was not read with no browser.
#[derive(Debug)]
pub enum Failure {
    /// The list's page carried no key to sign with.
    NoKey,
    /// Cortex refused what a key fresh from the page signed.
    Refused,
    /// Cortex sends the session to sign in: it has ended.
    SignedOut,
    /// Cortex did not answer every read, or answered what the day cannot be read from; the
    /// error says which.
    Unavailable(Error),
}
impl From<Error> for Failure {
    fn from(error: Error) -> Self {
        Self::Unavailable(error)
    }
}
impl Failure {
    /// What a job's metrics call it.
    pub fn label(&self) -> &'static str {
        match self {
            Self::NoKey => "routes_native_no_key",
            Self::Refused => "routes_native_refused",
            Self::SignedOut => "routes_native_signed_out",
            Self::Unavailable(_) => "routes_native_unavailable",
        }
    }
    /// The worse of two reads' failures: a refusal says the key, a redirect the session,
    /// before anything a single read went through.
    fn worse(self, other: Self) -> Self {
        let rank = |failure: &Self| match failure {
            Self::Refused => 0,
            Self::SignedOut => 1,
            _ => 2,
        };
        if rank(&other) < rank(&self) {
            other
        } else {
            self
        }
    }
}
impl From<Refusal> for Failure {
    fn from(refusal: Refusal) -> Self {
        match refusal {
            Refusal::Unreadable("http_refused") => Self::Refused,
            Refusal::Unreadable("http_redirected") => Self::SignedOut,
            Refusal::Unreadable("http_too_large") => {
                Self::Unavailable(Error::new("routes_source_too_large", 502))
            }
            Refusal::Unavailable => Self::Unavailable(Error::new("provider_unavailable", 502)),
            Refusal::Unreadable(_) => {
                Self::Unavailable(Error::new("cortex_content_incomplete", 502))
            }
        }
    }
}

/// The key Cortex signs a session's requests with, and the session it names.
struct Key {
    secret: String,
    session: String,
}
impl Key {
    /// The page's `hmacSession` state, from its HTML alone.
    fn of_page(html: &str) -> Option<Self> {
        let at = html.find("\"hmacSession\":")?;
        let open = html[at..].find('{')? + at;
        let state = serde_json::Deserializer::from_str(&html[open..])
            .into_iter::<Value>()
            .next()?
            .ok()?;
        let fit = |value: &Value| {
            value
                .as_str()
                .filter(|text| {
                    (1..=KEY_BYTES).contains(&text.len())
                        && text.bytes().all(|b| b.is_ascii_graphic())
                })
                .map(str::to_owned)
        };
        Some(Self {
            secret: fit(&state["sessionKey"])?,
            session: fit(&state["sessionId"])?,
        })
    }
    /// The address and headers of a GET of `endpoint` with `params`, signed as the
    /// application signs it: method, path, the query sorted by name, the session and the
    /// moment, with the session's key.
    fn sign(
        &self,
        origin: &str,
        endpoint: &str,
        params: &[(&str, &str)],
    ) -> (String, Vec<(String, String)>) {
        let mut sorted = params.to_vec();
        sorted.sort_by_key(|(name, _)| *name);
        let query = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(sorted)
            .finish();
        let stamp = now().to_string();
        let canonical = [
            "GET",
            endpoint,
            &query,
            &format!("x-cortex-session:{}", self.session),
            &format!("x-cortex-timestamp:{stamp}"),
            "",
            "x-cortex-session;x-cortex-timestamp",
        ]
        .join("\n");
        let mut mac = <Hmac<sha2::Sha256> as KeyInit>::new_from_slice(self.secret.as_bytes())
            .expect("HMAC takes a key of any length");
        mac.update(canonical.as_bytes());
        (
            format!("{origin}{endpoint}?{query}"),
            vec![
                ("user-ref".into(), USER_REF.into()),
                (
                    "x-cortex-hmac-signature".into(),
                    STANDARD.encode(mac.finalize().into_bytes()),
                ),
                ("x-cortex-session".into(), self.session.clone()),
                ("x-cortex-timestamp".into(), stamp),
            ],
        )
    }
}

/// A body decoded within `limit` and `check`ed, away from the threads serving requests.
async fn decoded(
    wire: Wire,
    limit: usize,
    check: impl FnOnce(&str) -> Result<()> + Send + 'static,
) -> std::result::Result<String, Failure> {
    tokio::task::spawn_blocking(move || -> std::result::Result<String, Failure> {
        let text = wire.into_text(limit)?;
        check(&text)?;
        Ok(text)
    })
    .await
    .map_err(|_| Failure::Unavailable(Error::new("cortex_content_incomplete", 502)))?
}
/// Whether asking again may help: Cortex was briefly unavailable, or redirected for a
/// moment, as it once did every request of a poll.
fn passing(refusal: &Refusal) -> bool {
    matches!(
        refusal,
        Refusal::Unavailable | Refusal::Unreadable("http_redirected")
    )
}
fn parsed(text: &str) -> Result<Value> {
    serde_json::from_str(text).map_err(|_| Error::new("cortex_content_incomplete", 502))
}

/// A DSP's routes read over plain HTTP with its session, keeping the key between reads.
pub struct Native {
    http: Http,
    origin: String,
    key: Option<Key>,
}
impl Native {
    /// A reader carrying `session`: the cookies and user agent a browser signed in with.
    pub fn new(session: &SignedIn) -> Result<Self> {
        Ok(Self {
            http: Http::with_session(session, &cortex::HOSTS)?,
            origin: session.origin.clone(),
            key: None,
        })
    }
    /// The day `scope` names as `mode` reads it, whole: both lists and every itinerary the
    /// list names, each its driver's. It is checked as every capture is by its caller.
    pub async fn read(
        &mut self,
        scope: &Scope,
        mode: Mode,
    ) -> std::result::Result<Capture, Failure> {
        let started_at = now();
        let mut asked_again = false;
        loop {
            let key = match self.key.take() {
                Some(key) => key,
                None => self.page_key(scope).await?,
            };
            let reading = Reading {
                http: &self.http,
                origin: &self.origin,
                key: &key,
                scope,
                referer: format!("{}{}", self.origin, scope.list_path()),
                historical: historical(scope),
            };
            match reading.day(mode, started_at).await {
                // The page is asked once more: Cortex stopped accepting its key, or sends the
                // session to sign in, which the page then shows.
                Err(Failure::Refused | Failure::SignedOut) if !asked_again => asked_again = true,
                result => {
                    if !matches!(result, Err(Failure::Refused | Failure::SignedOut)) {
                        self.key = Some(key);
                    }
                    return result;
                }
            }
        }
    }
    /// The key the list's page carries. A page that sends the session to sign in, or is
    /// not the console's own, has none because the session ended.
    async fn page_key(&self, scope: &Scope) -> std::result::Result<Key, Failure> {
        let url = format!("{}{}", self.origin, scope.list_path());
        let referer = format!("{}/", self.origin);
        let html = match self
            .http
            .wire(&url, "text/html", &[], &referer, PAGE_LIMIT)
            .await
        {
            Ok(wire) => decoded(wire, PAGE_LIMIT, |_| Ok(())).await?,
            Err(Refusal::Unreadable("http_redirected" | "http_refused")) => {
                return Err(Failure::SignedOut);
            }
            Err(refusal) => return Err(refusal.into()),
        };
        match Key::of_page(&html) {
            Some(key) => Ok(key),
            None if crate::connection::console(&html) => Err(Failure::NoKey),
            None => Err(Failure::SignedOut),
        }
    }
}

/// Whether the day is past where its station is, as the application tells Cortex.
fn historical(scope: &Scope) -> &'static str {
    let zone: chrono_tz::Tz = scope.timezone.parse().unwrap_or(chrono_tz::UTC);
    if scope.date < clock::today(zone).to_string() {
        "true"
    } else {
        "false"
    }
}

/// One read of a day with one key.
struct Reading<'a> {
    http: &'a Http,
    origin: &'a str,
    key: &'a Key,
    scope: &'a Scope,
    referer: String,
    historical: &'static str,
}
impl Reading<'_> {
    async fn day(&self, mode: Mode, started_at: i64) -> std::result::Result<Capture, Failure> {
        let scope = self.scope;
        let (date, area) = (scope.date.as_str(), scope.service_area_id.as_str());
        let lists = [
            ("historicalDay", self.historical),
            ("localDate", date),
            ("serviceAreaId", area),
        ];
        let text = self.list(SUMMARIES, &lists).await?;
        let bytes = CaptureBudget::new(text.len())?;
        let summaries = parsed(&text)?;
        let text = self
            .list(
                ROUTE_SUMMARIES,
                &[lists.as_slice(), &[("statsFromSummaries", "true")]].concat(),
            )
            .await?;
        bytes.add(text.len())?;
        let route_summaries = parsed(&text)?;
        ensure(
            route_summaries["rmsRouteSummaries"].is_array(),
            "cortex_content_incomplete",
            502,
        )?;
        let listed = listed(&summaries, scope);
        ensure(
            listed.len() <= MAX_ITINERARIES,
            "routes_source_too_large",
            502,
        )?;
        // As the capture is checked at the end; every itinerary is asked for by these.
        ensure(
            listed
                .iter()
                .all(|(id, transporter)| token(id, 128) && token(transporter, 64)),
            "routes_scope_mismatch",
            502,
        )?;
        let itineraries = self.itineraries(&listed, &bytes).await?;
        Ok(Capture {
            version: 1,
            collection: Collection::Routes,
            mode,
            scope: scope.clone(),
            started_at,
            finished_at: now().max(started_at),
            summaries,
            route_summaries,
            itineraries,
        })
    }
    /// `endpoint` with `params`, signed and downloaded as it came.
    async fn get(
        &self,
        endpoint: &str,
        params: &[(&str, &str)],
        limit: usize,
    ) -> std::result::Result<Wire, Refusal> {
        let (url, headers) = self.key.sign(self.origin, endpoint, params);
        self.http
            .wire(&url, "application/json", &headers, &self.referer, limit)
            .await
    }
    /// A list's text, asked again while Cortex is briefly unavailable or redirects: a
    /// redirect that lasts sends the session to sign in.
    async fn list(
        &self,
        endpoint: &str,
        params: &[(&str, &str)],
    ) -> std::result::Result<String, Failure> {
        let mut attempt = 1;
        loop {
            match self.get(endpoint, params, MAX_BODY).await {
                Ok(wire) => return decoded(wire, MAX_BODY, |_| Ok(())).await,
                Err(refusal) if passing(&refusal) && attempt < LIST_ATTEMPTS => {
                    tokio::time::sleep(RETRY_PAUSE * attempt).await;
                    attempt += 1;
                }
                Err(refusal) => return Err(refusal.into()),
            }
        }
    }
    /// One itinerary's detail, when it is the one asked for, of the driver listed.
    async fn itinerary(
        &self,
        (id, transporter): &(String, String),
        limit: usize,
    ) -> std::result::Result<String, Failure> {
        let params = [
            ("documentType", "Itinerary"),
            ("historicalDay", self.historical),
            ("itineraryId", id.as_str()),
            ("serviceAreaId", self.scope.service_area_id.as_str()),
        ];
        let wire = self.get(&itinerary_path(id), &params, limit).await?;
        let (id, transporter) = (id.clone(), transporter.clone());
        decoded(wire, limit, move |text| {
            ensure(
                owned_by(text, &id, &transporter),
                "routes_scope_mismatch",
                502,
            )
        })
        .await
    }
    /// Every itinerary `listed` names, `DOWNLOADS` at a time, then what failed once more.
    async fn itineraries(
        &self,
        listed: &[(String, String)],
        bytes: &CaptureBudget,
    ) -> std::result::Result<Vec<ItineraryCapture>, Failure> {
        let mut read: Vec<Option<String>> = vec![None; listed.len()];
        let mut failure = None;
        for attempt in 0..2 {
            let pending: Vec<usize> = (0..listed.len()).filter(|i| read[*i].is_none()).collect();
            if pending.is_empty() {
                break;
            }
            if attempt > 0 {
                tokio::time::sleep(RETRY_PAUSE).await;
            }
            failure = None;
            let mut answers = stream::iter(pending)
                .map(|index| async move {
                    let limit = MAX_BODY.min(bytes.remaining());
                    (index, self.itinerary(&listed[index], limit).await)
                })
                .buffer_unordered(DOWNLOADS);
            while let Some((index, answer)) = answers.next().await {
                match answer {
                    Ok(detail) => {
                        bytes.add(detail.len())?;
                        read[index] = Some(detail);
                    }
                    Err(missed) => {
                        failure = Some(match failure {
                            Some(earlier) => missed.worse(earlier),
                            None => missed,
                        })
                    }
                }
            }
        }
        read.into_iter()
            .zip(listed)
            .map(|(detail, (id, transporter))| {
                Some(ItineraryCapture {
                    id: id.clone(),
                    transporter_id: transporter.clone(),
                    detail: detail?,
                })
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| {
                failure.unwrap_or_else(|| Error::new("routes_capture_invalid", 502).into())
            })
    }
}

#[cfg(test)]
#[path = "../../tests/backend/collections/routes/native.rs"]
mod tests;
