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
use super::capture::{Capture, Collection, ItineraryCapture, MAX_BODY, Mode};
use super::protocol::{
    CaptureBudget, ITINERARY, ROUTE_SUMMARIES, SUMMARIES, itineraries_listed, itinerary_path,
    owned_by, parsed,
};
use crate as cortex;
use crate::discovery::Scope;
use base64::{Engine, engine::general_purpose::STANDARD};
use dispatch_core::{
    Error, Result,
    collection::browser::http::{Http, Refusal, SignedIn, Wire, WireError},
    db::now,
    ensure,
    foundation::{clock, observability},
};
use futures_util::stream::{self, StreamExt};
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{Value, json};
use std::{sync::Mutex, time::Duration};
use tokio::time::{Instant, sleep_until};

/// Itineraries downloaded at once, as many as the application's signatures are.
const DOWNLOADS: usize = 12;
/// Times a list is asked for while Cortex is briefly unavailable or redirects.
const LIST_ATTEMPTS: u32 = 3;
/// How long Cortex is left before a read is asked for again, times the attempts so far.
const RETRY_PAUSE: Duration = Duration::from_secs(2);
/// A throttle without Retry-After leaves the provider alone before another attempt.
const THROTTLE_PAUSE: Duration = Duration::from_secs(10);
/// Bound a provider's retry delay before adding it to a monotonic instant.
const MAX_COOLDOWN: Duration = Duration::from_secs(24 * 60 * 60);
/// A read spends at most this much of its turn waiting on throttling. Longer cooldowns
/// fail promptly and stay with the reader for a later scheduled attempt.
const RETRY_BUDGET: Duration = Duration::from_secs(30);
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
    /// Cortex asked for a cooldown. Neither signing again nor opening a page helps it.
    Throttled(Error),
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
            Self::Throttled(_) => "routes_native_throttled",
            Self::Unavailable(_) => "routes_native_unavailable",
        }
    }
    /// Throttling stops the attempt before another key or browser is asked for. Otherwise
    /// a refusal says the key, a redirect the session, before a single unread response.
    fn worse(self, other: Self) -> Self {
        let rank = |failure: &Self| match failure {
            Self::Throttled(_) => 0,
            Self::Refused => 1,
            Self::SignedOut => 2,
            _ => 3,
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
fn passing(failure: &Failure) -> bool {
    match failure {
        Failure::Throttled(_) | Failure::SignedOut => true,
        Failure::Unavailable(error) => error.is(dispatch_core::Code::ProviderUnavailable),
        _ => false,
    }
}

/// Shared by the day's requests and retained with its reader. A throttle stops admission,
/// while requests already in flight drain. Keeping the deadline here survives cancellation.
#[derive(Default)]
struct Throttle {
    until: Mutex<Option<Instant>>,
}
impl Throttle {
    fn failure(&self) -> Failure {
        Failure::Throttled(Error::new(cortex::codes::CORTEX_THROTTLED.as_str(), 502))
    }
    fn active(&self) -> bool {
        self.until
            .lock()
            .expect("route cooldown")
            .is_some_and(|until| until > Instant::now())
    }
    fn refused(&self, endpoint: &'static str, error: WireError) -> Failure {
        observability::event(
            "warn",
            "cortex.routes_read_failed",
            json!({"endpoint":endpoint,"status":error.status,
                "retryAfterMs":error.retry_after.map(|d|d.as_millis().min(u64::MAX as u128) as u64)}),
        );
        if error.status != Some(429) {
            return error.refusal.into();
        }
        let retry = error
            .retry_after
            .unwrap_or(THROTTLE_PAUSE)
            .min(MAX_COOLDOWN);
        let until = Instant::now() + retry;
        let mut held = self.until.lock().expect("route cooldown");
        *held = Some(held.map_or(until, |previous| previous.max(until)));
        self.failure()
    }
    /// Whether a throttle was observed, including one whose delay passed while in-flight
    /// requests drained. A healthy read has no timer or added wait.
    async fn wait(&self, before: Instant) -> std::result::Result<bool, Failure> {
        let until = *self.until.lock().expect("route cooldown");
        let Some(until) = until else {
            return Ok(false);
        };
        if until > before && until > Instant::now() {
            return Err(self.failure());
        }
        sleep_until(until).await;
        *self.until.lock().expect("route cooldown") = None;
        Ok(true)
    }
}

/// A DSP's routes read over plain HTTP with its session, keeping the key between reads.
pub struct Native {
    http: Http,
    origin: String,
    key: Option<Key>,
    throttle: Throttle,
}
impl Native {
    /// A reader carrying `session`: the cookies and user agent a browser signed in with.
    pub fn new(session: &SignedIn) -> Result<Self> {
        Ok(Self {
            http: Http::with_session(session, &cortex::HOSTS)?,
            origin: session.origin.clone(),
            key: None,
            throttle: Throttle::default(),
        })
    }
    /// A throttle's cooldown still belongs to this reader, even if it elapsed since a
    /// cancelled read. Keep the reader rather than reopening a browser before observing it.
    pub fn has_cooldown(&self) -> bool {
        self.throttle
            .until
            .lock()
            .expect("route cooldown")
            .is_some()
    }
    /// The day `scope` names as `mode` reads it, whole: both lists and every itinerary the
    /// list names, each its driver's. It is checked as every capture is by its caller.
    pub async fn read(
        &mut self,
        scope: &Scope,
        mode: Mode,
    ) -> std::result::Result<Capture, Failure> {
        let retry_before = Instant::now() + RETRY_BUDGET;
        self.throttle.wait(retry_before).await?;
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
                throttle: &self.throttle,
                retry_before,
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
            Err(error) => {
                return Err(
                    match self
                        .throttle
                        .refused("/operations/execution/itineraries", error)
                    {
                        Failure::Refused | Failure::SignedOut => Failure::SignedOut,
                        failure => failure,
                    },
                );
            }
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
    throttle: &'a Throttle,
    retry_before: Instant,
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
        // A list that is not one would list no itinerary, and replace the day with none.
        ensure(
            summaries["itinerarySummaries"].is_array(),
            "cortex_content_incomplete",
            502,
        )?;
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
        let listed = itineraries_listed(&summaries, scope)?;
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
    ) -> std::result::Result<Wire, Failure> {
        if self.throttle.active() {
            return Err(self.throttle.failure());
        }
        let (url, headers) = self.key.sign(self.origin, endpoint, params);
        self.http
            .wire(&url, "application/json", &headers, &self.referer, limit)
            .await
            .map_err(|error| {
                let endpoint = match endpoint {
                    SUMMARIES => SUMMARIES,
                    ROUTE_SUMMARIES => ROUTE_SUMMARIES,
                    _ if endpoint.starts_with(ITINERARY) => {
                        "/operations/execution/api/itineraries/{id}"
                    }
                    _ => "unknown",
                };
                self.throttle.refused(endpoint, error)
            })
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
                Err(failure) if passing(&failure) && attempt < LIST_ATTEMPTS => {
                    if !self.throttle.wait(self.retry_before).await? {
                        tokio::time::sleep(RETRY_PAUSE * attempt).await;
                    }
                    attempt += 1;
                }
                Err(failure) => return Err(failure),
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
            if attempt > 0 && !self.throttle.wait(self.retry_before).await? {
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
