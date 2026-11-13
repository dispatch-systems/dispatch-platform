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
//! Whatever failed is asked once more; what Cortex throttled is asked again after each
//! cooldown, fewer at a time. Every request waits its place at the account's gate
//! (`connection::gate`), which every reader of the account on this server shares, so a
//! throttle one of them meets cools and narrows them all. A day is read whole, both lists
//! and every itinerary, or not at all.
use super::capture::{Capture, Collection, Detail, ItineraryCapture, MAX_BODY, Mode};
use super::protocol::{
    CaptureBudget, ITINERARY, ROUTE_SUMMARIES, SUMMARIES, itineraries_listed, itinerary_path,
    owned_by, parsed,
};
use crate as cortex;
use crate::connection::gate::{self, Gate};
use crate::discovery::Scope;
use base64::{Engine, engine::general_purpose::STANDARD};
use dispatch_core::{
    Error, Result,
    collection::browser::{
        Pending,
        http::{Http, Refusal, SignedIn, Wire, WireError},
    },
    db::now,
    ensure,
    foundation::{clock, observability},
};
use futures_util::stream::{self, StreamExt};
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    hash::{DefaultHasher, Hash, Hasher},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::time::{Instant, sleep_until};

/// Rounds of itineraries a read asks for at most, so one Cortex throttles with no delay ends.
const ROUNDS: usize = 12;
/// Times a list is asked for while Cortex is briefly unavailable or redirects.
const LIST_ATTEMPTS: u32 = 3;
/// How long Cortex is left before a read is asked for again, times the attempts so far.
const RETRY_PAUSE: Duration = Duration::from_secs(2);
/// Bound a provider's retry delay before adding it to a monotonic instant.
const MAX_COOLDOWN: Duration = Duration::from_secs(24 * 60 * 60);
/// A read spends at most this much of its turn waiting on throttling, well inside the
/// shortest interval between live reads. Longer cooldowns fail promptly and stay with the
/// reader for a later scheduled attempt.
const RETRY_BUDGET: Duration = Duration::from_secs(120);
/// As much as the list's page may be. It was 727 KB.
const PAGE_LIMIT: usize = 8 * 1024 * 1024;
/// As long as the key or the session it names may be.
const KEY_BYTES: usize = 1024;
/// The application's name for itself, on every request it signs.
const USER_REF: &str = "cortex-webapp-user";

/// The itineraries a reader wants of a day, each by its id and driver, as it finds them in the
/// day's list, which it is handed whole. Its error ends the read.
pub type Wanted = dyn Fn(&Value) -> Result<Vec<(String, String)>> + Send + Sync;
/// What a read asks of Cortex with one key.
type Asked<'r, T> = Pin<Box<dyn Future<Output = std::result::Result<T, Failure>> + Send + 'r>>;

/// The caller's current authority to read with this session. Owned context lets each
/// check run after a cooldown without borrowing a stale database snapshot.
pub type Check = dyn Fn() -> Pending<'static, ()> + Send + Sync;

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
    /// The caller withdrew or changed its authority while a read was waiting.
    Stopped(Error),
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
            Self::Stopped(_) => "routes_native_stopped",
            Self::Unavailable(_) => "routes_native_unavailable",
        }
    }
    /// Throttling stops the attempt before another key or browser is asked for. Otherwise
    /// a refusal says the key, a redirect the session, before a single unread response.
    fn worse(self, other: Self) -> Self {
        let rank = |failure: &Self| match failure {
            Self::Throttled(_) => 0,
            Self::Stopped(_) => 0,
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
/// An itinerary's body `check`ed as text and kept compressed, as it came when it came so, away
/// from the threads serving requests: its text lives only for the check.
async fn detail(
    wire: Wire,
    limit: usize,
    check: impl FnOnce(&str) -> Result<()> + Send + 'static,
) -> std::result::Result<Detail, Failure> {
    tokio::task::spawn_blocking(move || -> std::result::Result<Detail, Failure> {
        let (text, gzip) = wire.into_parts(limit)?;
        check(&text)?;
        Ok(match gzip {
            Some(gzip) => Detail::of_gzip(gzip, text.len()),
            None => Detail::of_text(&text)?,
        })
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

/// What a read meets of the account's throttling: its gate, which every reader of the account
/// shares, and the requests Cortex throttled during this read.
struct Throttle {
    gate: Arc<Gate>,
    /// Requests Cortex throttled during the read under way.
    refused: AtomicU64,
}
impl Throttle {
    fn failure(&self) -> Failure {
        Failure::Throttled(Error::new(cortex::codes::CORTEX_THROTTLED.as_str(), 502))
    }
    fn refused(&self, endpoint: &'static str, error: WireError) -> Failure {
        observability::event(
            "warn",
            "cortex.routes_read_failed",
            json!({"endpoint":endpoint,"status":error.status,"downloads":self.gate.width(),
                "retryAfterMs":error.retry_after.map(|d|d.as_millis().min(u64::MAX as u128) as u64)}),
        );
        if error.status != Some(429) {
            return error.refusal.into();
        }
        self.refused.fetch_add(1, Ordering::Relaxed);
        self.gate
            .throttled(error.retry_after.unwrap_or(gate::PAUSE).min(MAX_COOLDOWN));
        self.failure()
    }
    /// A request's place among the account's, which a throttle met while it waited for one
    /// ends. One that waited behind another reader asks `check` again before it is sent: its
    /// authority to read with this session may have gone meanwhile.
    async fn admitted(&self, check: &Check) -> std::result::Result<gate::Admitted, Failure> {
        if self.gate.cooling() {
            return Err(self.failure());
        }
        let (admitted, waited) = self.gate.admit().await;
        if waited {
            check().await.map_err(Failure::Stopped)?;
        }
        if self.gate.cooling() {
            return Err(self.failure());
        }
        Ok(admitted)
    }
    /// Whether a throttle was observed, including one whose delay passed while in-flight
    /// requests drained. A healthy read has no timer or added wait.
    async fn wait(&self, budget: &Mutex<Duration>) -> std::result::Result<bool, Failure> {
        let Some(until) = self.gate.cooldown() else {
            return Ok(false);
        };
        let delay = until.saturating_duration_since(Instant::now());
        {
            let mut remaining = budget.lock().expect("route retry budget");
            if delay > *remaining {
                return Err(self.failure());
            }
            *remaining = remaining.saturating_sub(delay);
        }
        sleep_until(until).await;
        self.gate.cooled(until);
        Ok(true)
    }
}

/// Reads of today in a row that reuse settled routes' itineraries, counting the one that
/// asks for them all: one read in six asks Cortex for every itinerary again, so a change
/// the list did not show is in live route data within half an hour at the default pace.
const WHOLE_EVERY: u64 = 6;
/// What a route's entry in the list says that moves with the clock rather than with what
/// its driver did: left out of what tells a settled route changed.
const CLOCKED: [&str; 6] = [
    "timeRemainingSecs",
    "projectedCompletionTime",
    "stateOfChargeRemainingPercent",
    "stateOfChargeRemainingPercentUpdatedAt",
    "centroid",
    "weatherData",
];

/// What a reader keeps of today as it last read it whole or in part, to ask Cortex for less:
/// each itinerary, compressed, with what the list said of its route then.
#[derive(Default)]
struct Kept {
    scope: Option<Scope>,
    /// Reads of the scope kept so far.
    reads: u64,
    /// Each itinerary by its id, with its route's entry in the list as it was, clock aside.
    routes: HashMap<String, (u64, Detail)>,
}
/// What the list says of a route, clock aside, to tell whether it changed.
fn said(entry: &Value) -> u64 {
    let mut entry = entry.clone();
    if let Some(fields) = entry.as_object_mut() {
        for field in CLOCKED {
            fields.remove(field);
        }
    }
    let mut hasher = DefaultHasher::new();
    entry.to_string().hash(&mut hasher);
    hasher.finish()
}
/// Whether the list shows a route nothing is happening on: finished, its driver's session
/// ended, or not begun.
fn settled(entry: &Value) -> bool {
    let finished = entry["executionStatus"] == "COMPLETE" && entry["driverSessionEnded"] == true;
    let unbegun = entry["itineraryStartTime"].is_null()
        && entry["stopProgress"]["completed"].as_u64() == Some(0);
    finished || unbegun
}
impl Kept {
    /// The itinerary kept for a settled route the list says the same of as when it was read.
    fn settled(&self, id: &str, entry: &Value) -> Option<&Detail> {
        let (was, detail) = self.routes.get(id)?;
        (settled(entry) && *was == said(entry)).then_some(detail)
    }
}
/// Each route's entry in the day's list, by its itinerary.
fn entries(summaries: &Value) -> HashMap<&str, &Value> {
    summaries["itinerarySummaries"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| Some((entry["itineraryId"].as_str()?, entry)))
        .collect()
}

/// A DSP's routes read over plain HTTP with its session, keeping the key between reads.
pub struct Native {
    http: Http,
    origin: String,
    key: Option<Key>,
    /// The gate of the account it last read, and what that read met of its throttling.
    throttle: Option<Throttle>,
    kept: Kept,
    /// Itineraries the last read reused from the read before, asking Cortex for none of them.
    reused: u64,
    /// Settled routes whose itinerary the last read, asking for them all, found changed
    /// though the list said the same: what a read that reuses them would not have seen.
    unseen: u64,
}
impl Native {
    /// A reader carrying `session`: the cookies and user agent a browser signed in with.
    pub fn new(session: &SignedIn) -> Result<Self> {
        Ok(Self {
            http: Http::with_session(session, &cortex::HOSTS)?,
            origin: session.origin.clone(),
            key: None,
            throttle: None,
            kept: Kept::default(),
            reused: 0,
            unseen: 0,
        })
    }
    /// What the last read met, as its record counts it: the requests Cortex throttled
    /// (`throttled`), how many requests the account's gate admits at once since
    /// (`downloads`), the itineraries it reused for settled routes (`reused`), and those a
    /// read of them all found changed though the list said the same (`unseen`). Nothing for
    /// a read at full width that met no throttle, reused none and found none.
    pub fn counts(&self) -> Vec<(&'static str, u64)> {
        let mut counts = Vec::new();
        if self.reused > 0 {
            counts.push(("reused", self.reused));
        }
        if self.unseen > 0 {
            counts.push(("unseen", self.unseen));
        }
        let Some(throttle) = &self.throttle else {
            return counts;
        };
        let refused = throttle.refused.load(Ordering::Relaxed);
        if refused > 0 {
            counts.push(("throttled", refused));
        }
        if throttle.gate.narrowed() {
            counts.push(("downloads", throttle.gate.width() as u64));
        }
        counts
    }
    /// A throttle's cooldown still belongs to this reader, even if it elapsed since a
    /// cancelled read. Keep the reader rather than reopening a browser before observing it.
    pub fn has_cooldown(&self) -> bool {
        self.throttle
            .as_ref()
            .is_some_and(|throttle| throttle.gate.cooldown().is_some())
    }
    /// The day `scope` names as `mode` reads it, whole: both lists and every itinerary the
    /// list names, each its driver's. It is checked as every capture is by its caller.
    ///
    /// A snapshot of the day this reader read last asks Cortex only for the routes
    /// something may be happening on: a route the list shows finished, or not begun, and says
    /// the same of as at its last read keeps the itinerary read then. One read in `WHOLE_EVERY`
    /// read asks for them all, as does a first read, another day's or a completed day's.
    pub async fn read(
        &mut self,
        scope: &Scope,
        mode: Mode,
        check: &Check,
    ) -> std::result::Result<Capture, Failure> {
        if mode != Mode::Snapshot || self.kept.scope.as_ref() != Some(scope) {
            self.kept = Kept::default();
        }
        let whole = self.kept.reads.is_multiple_of(WHOLE_EVERY);
        (self.reused, self.unseen) = (0, 0);
        let started_at = now();
        let capture = self
            .keyed(scope, check, !whole, None, move |reading| {
                Box::pin(reading.day(mode, started_at))
            })
            .await?;
        if mode == Mode::Snapshot {
            let entries = entries(&capture.summaries);
            let read = (capture.itineraries.iter())
                .filter_map(|i| Some((i, *entries.get(i.id.as_str())?)));
            if whole {
                // Each as text: Cortex may compress the same itinerary another way, which
                // is no change to it.
                let settled: Vec<(Detail, Detail)> = (read.clone())
                    .filter_map(|(i, entry)| {
                        let kept = self.kept.settled(&i.id, entry)?;
                        Some((kept.clone(), i.detail.clone()))
                    })
                    .collect();
                self.unseen = tokio::task::spawn_blocking(move || {
                    (settled.iter())
                        .filter(|(kept, read)| kept.text().ok() != read.text().ok())
                        .count() as u64
                })
                .await
                .unwrap_or(0);
            } else {
                self.reused = (read.clone())
                    .filter(|(i, entry)| self.kept.settled(&i.id, entry).is_some())
                    .count() as u64;
            }
            self.kept = Kept {
                scope: Some(scope.clone()),
                reads: self.kept.reads + 1,
                routes: read
                    .map(|(i, entry)| (i.id.clone(), (said(entry), i.detail.clone())))
                    .collect(),
            };
        }
        Ok(capture)
    }
    /// The day's list for `scope`, and of its itineraries those `wanted` picks from it, each
    /// its driver's: for a reader that needs only some routes of a day, as the meal collection
    /// does. Nothing of the day is kept for a next read.
    pub async fn read_some(
        &mut self,
        scope: &Scope,
        check: &Check,
        wanted: &Wanted,
    ) -> std::result::Result<Vec<ItineraryCapture>, Failure> {
        self.keyed(scope, check, false, Some(wanted), |reading| {
            Box::pin(reading.some())
        })
        .await
    }
    /// What `ask` reads of `scope`'s day with the key the list's page carries, at the
    /// account's gate: the page asked for its key once more when Cortex refuses what it
    /// signed or sends the session to sign in. `reuse` is whether the read may keep settled
    /// routes' itineraries from the read before.
    async fn keyed<T>(
        &mut self,
        scope: &Scope,
        check: &Check,
        reuse: bool,
        wanted: Option<&Wanted>,
        ask: impl for<'r> Fn(&'r Reading<'r>) -> Asked<'r, T>,
    ) -> std::result::Result<T, Failure> {
        self.throttle = Some(Throttle {
            gate: gate::of(&self.origin, &scope.provider),
            refused: AtomicU64::new(0),
        });
        let throttle = self.throttle.as_ref().expect("the read's throttle");
        let retry_budget = Mutex::new(RETRY_BUDGET);
        throttle.wait(&retry_budget).await?;
        check().await.map_err(Failure::Stopped)?;
        let mut asked_again = false;
        loop {
            let key = match self.key.take() {
                Some(key) => key,
                None => self.page_key(scope, throttle, check).await?,
            };
            let reading = Reading {
                http: &self.http,
                origin: &self.origin,
                key: &key,
                scope,
                referer: format!("{}{}", self.origin, scope.list_path()),
                historical: historical(scope),
                throttle,
                retry_budget: &retry_budget,
                check,
                kept: reuse.then_some(&self.kept),
                wanted,
            };
            match ask(&reading).await {
                // The page is asked once more: Cortex stopped accepting its key, or sends the
                // session to sign in, which the page then shows.
                Err(Failure::Refused | Failure::SignedOut) if !asked_again => asked_again = true,
                result => {
                    if !matches!(result, Err(Failure::Refused | Failure::SignedOut)) {
                        self.key = Some(key);
                    }
                    if result.is_ok() && throttle.refused.load(Ordering::Relaxed) == 0 {
                        throttle.gate.read_clean();
                    }
                    return result;
                }
            }
        }
    }
    /// The key the list's page carries. A page that sends the session to sign in, or is
    /// not the console's own, has none because the session ended.
    async fn page_key(
        &self,
        scope: &Scope,
        throttle: &Throttle,
        check: &Check,
    ) -> std::result::Result<Key, Failure> {
        let url = format!("{}{}", self.origin, scope.list_path());
        let referer = format!("{}/", self.origin);
        let _admitted = throttle.admitted(check).await?;
        let html = match self
            .http
            .wire(&url, "text/html", &[], &referer, PAGE_LIMIT)
            .await
        {
            Ok(wire) => decoded(wire, PAGE_LIMIT, |_| Ok(())).await?,
            Err(error) => {
                return Err(
                    match throttle.refused("/operations/execution/itineraries", error) {
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
    retry_budget: &'a Mutex<Duration>,
    check: &'a Check,
    /// What the reader kept of the day, when this read may reuse settled routes' itineraries.
    kept: Option<&'a Kept>,
    /// The itineraries a read of some of the day's wants.
    wanted: Option<&'a Wanted>,
}
impl Reading<'_> {
    /// What a list is asked for with: the day, and whether it is past.
    fn lists(&self) -> [(&str, &str); 3] {
        [
            ("historicalDay", self.historical),
            ("localDate", self.scope.date.as_str()),
            ("serviceAreaId", self.scope.service_area_id.as_str()),
        ]
    }
    /// The itineraries its reader wants of the day's list: none for a reader that wants the day.
    async fn some(&self) -> std::result::Result<Vec<ItineraryCapture>, Failure> {
        let text = self.list(SUMMARIES, &self.lists()).await?;
        let bytes = CaptureBudget::new(text.len())?;
        let summaries = parsed(&text)?;
        let listed = match self.wanted {
            Some(wanted) => wanted(&summaries)?,
            None => Vec::new(),
        };
        self.itineraries(&listed, &HashMap::new(), &bytes).await
    }
    async fn day(&self, mode: Mode, started_at: i64) -> std::result::Result<Capture, Failure> {
        let scope = self.scope;
        let lists = self.lists();
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
        let itineraries = self
            .itineraries(&listed, &entries(&summaries), &bytes)
            .await?;
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
        let _admitted = self.throttle.admitted(self.check).await?;
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
                    if !self.throttle.wait(self.retry_budget).await? {
                        tokio::time::sleep(RETRY_PAUSE * attempt).await;
                    }
                    (self.check)().await.map_err(Failure::Stopped)?;
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
    ) -> std::result::Result<Detail, Failure> {
        let params = [
            ("documentType", "Itinerary"),
            ("historicalDay", self.historical),
            ("itineraryId", id.as_str()),
            ("serviceAreaId", self.scope.service_area_id.as_str()),
        ];
        let wire = self.get(&itinerary_path(id), &params, limit).await?;
        let (id, transporter) = (id.clone(), transporter.clone());
        detail(wire, limit, move |text| {
            ensure(
                owned_by(text, &id, &transporter),
                "routes_scope_mismatch",
                502,
            )
        })
        .await
    }
    /// Every itinerary `listed` names, a few at a time, then what failed once more. What
    /// Cortex throttled is asked again after each cooldown, fewer at a time, while the
    /// read's wait budget and `ROUNDS` last.
    async fn itineraries(
        &self,
        listed: &[(String, String)],
        entries: &HashMap<&str, &Value>,
        bytes: &CaptureBudget,
    ) -> std::result::Result<Vec<ItineraryCapture>, Failure> {
        let mut read: Vec<Option<Detail>> = vec![None; listed.len()];
        // A settled route the list says the same of keeps the itinerary read before.
        if let Some(kept) = self.kept {
            for (place, (id, _)) in read.iter_mut().zip(listed) {
                let entry = entries.get(id.as_str());
                if let Some(detail) = entry.and_then(|entry| kept.settled(id, entry)) {
                    bytes.add(detail.bytes())?;
                    *place = Some(detail.clone());
                }
            }
        }
        let mut failure = None;
        let mut asked_again = false;
        for round in 0..ROUNDS {
            let pending: Vec<usize> = (0..listed.len()).filter(|i| read[*i].is_none()).collect();
            if pending.is_empty() {
                break;
            }
            if round > 0 {
                if !self.throttle.wait(self.retry_budget).await? {
                    if asked_again {
                        break;
                    }
                    asked_again = true;
                    tokio::time::sleep(RETRY_PAUSE).await;
                }
                (self.check)().await.map_err(Failure::Stopped)?;
            }
            self.throttle.gate.narrow();
            failure = None;
            let mut answers = stream::iter(pending)
                .map(|index| async move {
                    let limit = MAX_BODY.min(bytes.remaining());
                    (index, self.itinerary(&listed[index], limit).await)
                })
                // The gate admits as many at once as the account may ask for.
                .buffer_unordered(self.throttle.gate.width());
            while let Some((index, answer)) = answers.next().await {
                match answer {
                    Ok(detail) => {
                        bytes.add(detail.bytes())?;
                        read[index] = Some(detail);
                    }
                    // Its authority gone, the read ends before another request is admitted
                    // in the one's place, which would not have waited and so not asked.
                    Err(stopped @ Failure::Stopped(_)) => return Err(stopped),
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
