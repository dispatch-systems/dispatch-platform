//! A day's reads signed by the list's own application and downloaded by this process.
//!
//! Cortex signs each data request for its exact address and the moment it is made, with
//! a key only the application holds. The application is asked, through its own API
//! action (`routes_sign.js`), for every itinerary and the routes page's list; each
//! request is stopped and failed the moment it is made, before it leaves the browser, and
//! only its address and Cortex's signature headers are kept. This process downloads them
//! with the browser's session, a few at a time; the application is handed more reads only
//! as downloads finish, so no signature waits long to be used. A read that fails is signed
//! and downloaded once more; whatever is still unread is left to the application's moves.
use super::capture::{ItineraryCapture, MAX_BODY};
use super::collect::{CaptureBudget, ITINERARY, ROUTE_SUMMARIES, owned_by};
use crate as cortex;
use crate::{connection::Driver, discovery::Scope};
use dispatch_core::{
    Code, Result,
    collection::{
        browser::{
            http::{Http, Refusal},
            page::{Page, call},
        },
        metrics::Recorder,
    },
    db::s,
};
use serde_json::{Map, Value, json};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    future::Future,
    sync::Arc,
    time::Duration,
};
use tokio::{task::JoinSet, time::Instant};

const SIGN: &str = include_str!("../../scripts/routes_sign.js");
/// Reads the application is asked for at once. The browser holds each request it stops
/// until it is failed, and its worker gives up past sixteen held at once.
const LANES: usize = 8;
/// Times a read is signed and downloaded before it is left to the application's moves.
const ATTEMPTS: usize = 2;
/// How long a throttled or failing Cortex is left before the second attempt.
const RETRY_PAUSE: Duration = Duration::from_secs(2);
/// How long the application's store may take to appear once its list has arrived.
const STORE_DEADLINE: Duration = Duration::from_secs(10);
/// How long the application may go without asking for another read it was given.
const SIGN_DEADLINE: Duration = Duration::from_secs(10);
/// Most reads handed to the application per command, which keeps each well inside what
/// the browser accepts: itinerary and area ids are at most 128 bytes.
const CHUNK: usize = 50;
/// How long the application's lanes may take to stop once told.
const STOP_DEADLINE: Duration = Duration::from_secs(5);
/// Cortex's signature: the headers a signed request must carry and is sent with.
const SIGNATURE: &[&str] = &[
    "x-cortex-hmac-signature",
    "x-cortex-session",
    "x-cortex-timestamp",
];
/// The application's name for itself, sent when it sends it.
const USER_REF: &str = "user-ref";
/// As long as any one signature header may be.
const HEADER_BYTES: usize = 1024;

/// One read the application is asked to sign: an itinerary, by its place in the list, or
/// the routes page's list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Read {
    Itinerary(usize),
    Routes,
}
/// A read's request as the application signed it: its address and the headers to send.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Signed {
    pub url: String,
    pub headers: Vec<(String, String)>,
}
/// What the signed reads came to; anything still `None` was not read.
pub(super) struct SignedDay {
    pub itineraries: Vec<Option<ItineraryCapture>>,
    pub routes: Option<Value>,
}

/// What one attempt came to.
struct Attempt {
    /// Whether the application took the reads at all.
    started: bool,
    /// Whether it signed any of them.
    signed: bool,
}

/// The reads of one scope's day, and the address each is asked at.
pub(super) struct Day<'a> {
    scope: &'a Scope,
    listed: &'a [(String, String)],
    /// Each itinerary's id, to the first place in the list it has.
    places: HashMap<&'a str, usize>,
}
impl<'a> Day<'a> {
    pub(super) fn new(scope: &'a Scope, listed: &'a [(String, String)]) -> Self {
        let mut places = HashMap::new();
        for (index, (id, _)) in listed.iter().enumerate() {
            places.entry(id.as_str()).or_insert(index);
        }
        Self {
            scope,
            listed,
            places,
        }
    }
    /// Every read, but an itinerary the list repeats only once: its repeat is read by moves.
    fn reads(&self) -> impl Iterator<Item = Read> {
        (0..self.listed.len())
            .filter(|index| self.places.get(self.listed[*index].0.as_str()) == Some(index))
            .map(Read::Itinerary)
            .chain([Read::Routes])
    }
    fn path(&self, read: Read) -> String {
        match read {
            Read::Itinerary(index) => itinerary_path(&self.listed[index].0),
            Read::Routes => ROUTE_SUMMARIES.into(),
        }
    }
    /// What a read asks for, as the routes and itinerary pages ask. The application adds
    /// whether the day is past itself.
    fn params(&self, read: Read) -> Vec<(&'static str, &'a str)> {
        let area = self.scope.service_area_id.as_str();
        match read {
            Read::Itinerary(index) => vec![
                ("documentType", "Itinerary"),
                ("itineraryId", self.listed[index].0.as_str()),
                ("serviceAreaId", area),
            ],
            Read::Routes => vec![
                ("localDate", self.scope.date.as_str()),
                ("serviceAreaId", area),
                ("statsFromSummaries", "true"),
            ],
        }
    }
    fn ask(&self, read: Read) -> Value {
        let params: Map<String, Value> = self
            .params(read)
            .into_iter()
            .map(|(name, value)| (name.to_owned(), json!(value)))
            .collect();
        json!({"endpoint": self.path(read), "params": params})
    }
    /// The read a paused request is, with the headers to send it with: only a GET of
    /// exactly one read's address on the application's origin, carrying Cortex's whole
    /// signature, and of its headers only the signature's.
    pub(super) fn signed(&self, origin: &str, request: &Value) -> Option<(Read, Signed)> {
        if s(request, "method") != "GET" {
            return None;
        }
        let text = s(request, "url");
        let url = url::Url::parse(text).ok()?;
        if url.origin().ascii_serialization() != origin
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return None;
        }
        let read = if url.path() == ROUTE_SUMMARIES {
            Read::Routes
        } else {
            let id = decoded(url.path().strip_prefix(ITINERARY)?)?;
            Read::Itinerary(*self.places.get(id.as_str())?)
        };
        let mut expected = self.params(read);
        let mut historical = false;
        for (name, value) in url.query_pairs() {
            if name == "historicalDay" && !historical && ["true", "false"].contains(&&*value) {
                historical = true;
                continue;
            }
            let at = expected
                .iter()
                .position(|(n, v)| *n == name && *v == value)?;
            expected.swap_remove(at);
        }
        if !expected.is_empty() {
            return None;
        }
        let mut headers: Vec<(String, String)> = Vec::new();
        for (name, value) in request["headers"].as_object()? {
            let name = name.to_ascii_lowercase();
            if !SIGNATURE.contains(&name.as_str()) && name != USER_REF {
                continue;
            }
            let value = value.as_str()?;
            let printable = value.bytes().all(|b| (0x20..0x7f).contains(&b));
            if value.is_empty()
                || value.len() > HEADER_BYTES
                || !printable
                || headers.iter().any(|(n, _)| *n == name)
            {
                return None;
            }
            headers.push((name, value.to_owned()));
        }
        if !SIGNATURE
            .iter()
            .all(|wanted| headers.iter().any(|(n, _)| n == wanted))
        {
            return None;
        }
        headers.sort();
        Some((
            read,
            Signed {
                url: text.to_owned(),
                headers,
            },
        ))
    }
}
/// One path segment as the server reads it, its escapes decoded.
fn decoded(segment: &str) -> Option<String> {
    let bytes = segment.as_bytes();
    let mut text = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'/' => return None,
            b'%' => {
                let hex = bytes.get(at + 1..at + 3)?;
                if !hex.iter().all(u8::is_ascii_hexdigit) {
                    return None;
                }
                text.push(u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?);
                at += 3;
            }
            byte => {
                text.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8(text).ok()
}
fn itinerary_path(id: &str) -> String {
    format!(
        "{ITINERARY}{}",
        url::form_urlencoded::byte_serialize(id.as_bytes()).collect::<String>()
    )
}
/// What the tab pauses while the application signs: its requests for either API, before
/// they are sent.
fn request_patterns() -> Value {
    let patterns: Vec<Value> = [ITINERARY, ROUTE_SUMMARIES]
        .iter()
        .flat_map(|path| {
            ["XHR", "Fetch"].map(|kind| {
                json!({"urlPattern":format!("*{path}*"),"resourceType":kind,"requestStage":"Request"})
            })
        })
        .collect();
    json!({ "patterns": patterns })
}
fn refused(refusal: &Refusal) -> &'static str {
    match refusal {
        Refusal::Unavailable => "http_unavailable",
        Refusal::Unreadable(label) => label,
    }
}

type Downloaded = (Read, std::result::Result<String, Refusal>);

/// One attempt's state: what is left to read, and what came of it.
struct Signing<'a, 'b, F> {
    day: &'a Day<'b>,
    read: &'a mut SignedDay,
    bytes: &'a CaptureBudget,
    metrics: &'a Recorder,
    progress: &'a F,
    attempt: usize,
    /// Whether Cortex refused for load: the next attempt waits a little first.
    throttled: bool,
    /// Whether Cortex answered as if signed out: the moves find out for certain.
    signed_out: bool,
}
impl<F, Fut> Signing<'_, '_, F>
where
    F: Fn(i64, String) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    /// Keeps one download's answer when it is the read asked for.
    async fn take(
        &mut self,
        joined: std::result::Result<Downloaded, tokio::task::JoinError>,
    ) -> Result<()> {
        let Ok((read, answer)) = joined else {
            return Ok(());
        };
        let text = match answer {
            Ok(text) => text,
            Err(refusal) => {
                self.throttled |= matches!(refusal, Refusal::Unavailable);
                self.signed_out |= matches!(refusal, Refusal::Unreadable("http_signed_out"));
                if let Read::Itinerary(index) = read {
                    self.metrics.page_finish(index + 1, Some(refused(&refusal)));
                }
                return Ok(());
            }
        };
        let Read::Itinerary(index) = read else {
            if let Ok(value) = serde_json::from_str::<Value>(&text)
                && value["rmsRouteSummaries"].is_array()
            {
                self.bytes.add(text.len())?;
                self.read.routes = Some(value);
            }
            return Ok(());
        };
        let (id, transporter) = &self.day.listed[index];
        if !owned_by(&text, id, transporter) {
            self.metrics
                .page_finish(index + 1, Some("routes_scope_mismatch"));
            return Ok(());
        }
        self.bytes.add(text.len())?;
        self.read.itineraries[index] = Some(ItineraryCapture {
            id: id.clone(),
            transporter_id: transporter.clone(),
            detail: text,
        });
        self.metrics.page_finish(index + 1, None);
        self.metrics.direct();
        let total = self.read.itineraries.len();
        let done = self.read.itineraries.iter().flatten().count();
        (self.progress)(
            15 + (75 * done / total.max(1)) as i64,
            format!("Reading itineraries ({done}/{total})"),
        )
        .await
    }
}

impl Driver {
    /// Every itinerary of `listed` and the routes page's list, as far as signing and
    /// downloading them reads them, `downloads` at a time.
    pub(super) async fn read_signed<F, Fut>(
        &self,
        scope: &Scope,
        listed: &[(String, String)],
        downloads: usize,
        bytes: &CaptureBudget,
        metrics: &Recorder,
        progress: &F,
    ) -> Result<SignedDay>
    where
        F: Fn(i64, String) -> Fut,
        Fut: Future<Output = Result<()>>,
    {
        let day = Day::new(scope, listed);
        let mut read = SignedDay {
            itineraries: vec![None; listed.len()],
            routes: None,
        };
        let http = Arc::new(Http::signed_in(&self.browser, &self.origin, &cortex::HOSTS).await?);
        let referer: Arc<str> = format!("{}{}", self.origin, scope.list_path()).into();
        let mut throttled = false;
        for attempt in 1..=ATTEMPTS {
            let pending: Vec<Read> = day
                .reads()
                .filter(|r| match r {
                    Read::Itinerary(index) => read.itineraries[*index].is_none(),
                    Read::Routes => read.routes.is_none(),
                })
                .collect();
            if pending.is_empty() {
                break;
            }
            if attempt > 1 {
                metrics.detail("routes_signed_retried");
                if throttled {
                    tokio::time::sleep(RETRY_PAUSE).await;
                }
            }
            let mut signing = Signing {
                day: &day,
                read: &mut read,
                bytes,
                metrics,
                progress,
                attempt,
                throttled: false,
                signed_out: false,
            };
            self.page
                .command("Fetch.enable", request_patterns())
                .await?;
            let result = self
                .sign(&mut signing, &pending, &http, &referer, downloads)
                .await;
            throttled = signing.throttled;
            let signed_out = signing.signed_out;
            let stopped = self.stop_signing(&self.page).await;
            let attempt = result?;
            stopped?;
            // Signing again cannot help an application that signs nothing, nor a session
            // Cortex no longer accepts.
            if !attempt.started || !attempt.signed || signed_out {
                break;
            }
        }
        Ok(read)
    }
    /// One attempt at `pending`. The application is handed reads only while fewer than
    /// `downloads` and its lanes' worth are asked, signed or downloading; each request it
    /// makes is failed at once and its read downloaded as soon as a download is free.
    async fn sign<F, Fut>(
        &self,
        signing: &mut Signing<'_, '_, F>,
        pending: &[Read],
        http: &Arc<Http>,
        referer: &Arc<str>,
        downloads: usize,
    ) -> Result<Attempt>
    where
        F: Fn(i64, String) -> Fut,
        Fut: Future<Output = Result<()>>,
    {
        let day = signing.day;
        let asks = |reads: &[Read]| reads.iter().map(|r| day.ask(*r)).collect::<Vec<_>>();
        let window = downloads + LANES;
        let mut unasked = pending.iter().copied();
        let first: Vec<Read> = unasked.by_ref().take(window.min(CHUNK)).collect();
        let mut attempt = Attempt {
            started: false,
            signed: false,
        };
        if first.is_empty() {
            return Ok(attempt);
        }
        let start = json!({"action":"sign","lanes":LANES,"reads":asks(&first)});
        // The list's response arrives before the application has rendered its tree.
        let deadline = Instant::now() + STORE_DEADLINE;
        loop {
            match self
                .browser
                .evaluate(&self.page.id, &call(SIGN, &start))
                .await
            {
                Ok(answer) if answer["started"] == true => break,
                // Found, but the application cannot sign as asked: asking again won't help.
                Ok(answer) if answer["busy"] == true || answer["store"] == true => {
                    signing.metrics.detail("routes_signing_unavailable");
                    return Ok(attempt);
                }
                Ok(_) => (),
                Err(error) if error.is(Code::BrowserScriptFailed) => (),
                Err(error) => return Err(error),
            }
            if Instant::now() >= deadline {
                signing.metrics.detail("routes_signing_unavailable");
                return Ok(attempt);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        attempt.started = true;
        // Asked of the application and not yet signed; signed and not yet downloading.
        let mut asked: HashSet<Read> = first.into_iter().collect();
        let mut ready: VecDeque<(Read, Signed)> = VecDeque::new();
        let mut tasks: JoinSet<Downloaded> = JoinSet::new();
        let mut open = true;
        let mut quiet_since = Instant::now();
        loop {
            let room = window.saturating_sub(asked.len() + ready.len() + tasks.len());
            if open && room > 0 {
                let more: Vec<Read> = unasked.by_ref().take(room.min(CHUNK)).collect();
                if !more.is_empty() {
                    let add = call(SIGN, &json!({"action":"add","reads":asks(&more)}));
                    let added = self.browser.evaluate(&self.page.id, &add).await;
                    if added.is_ok_and(|answer| answer["started"] == true) {
                        asked.extend(more);
                        quiet_since = Instant::now();
                    } else {
                        signing.metrics.detail("routes_signing_unavailable");
                        open = false;
                    }
                }
            }
            while tasks.len() < downloads
                && let Some((read, signed)) = ready.pop_front()
            {
                if let Read::Itinerary(index) = read {
                    signing.metrics.page_start(index + 1, signing.attempt);
                    signing.metrics.page_stage(index + 1, "content");
                }
                let limit = MAX_BODY.min(signing.bytes.remaining());
                let (http, referer) = (http.clone(), referer.clone());
                tasks.spawn(async move {
                    let answer = http
                        .json_text(&signed.url, &signed.headers, &referer, limit)
                        .await;
                    (read, answer)
                });
            }
            if asked.is_empty() && ready.is_empty() && tasks.is_empty() {
                break;
            }
            // Every paused request is answered as soon as it is seen, before anything else,
            // and never raced against anything: an abandoned browser command ends the whole
            // session.
            let event = self.browser.event(&self.page.id).await?;
            if !event.is_null() {
                let signed = day.signed(&self.origin, &event["request"]);
                // None leaves the browser.
                self.page
                    .command(
                        "Fetch.failRequest",
                        json!({"requestId":event["requestId"],"errorReason":"BlockedByClient"}),
                    )
                    .await?;
                match signed.filter(|(read, _)| asked.remove(read)) {
                    Some(signed) => {
                        attempt.signed = true;
                        quiet_since = Instant::now();
                        ready.push_back(signed);
                    }
                    None => signing.metrics.detail("routes_signed_unexpected"),
                }
            }
            while let Some(joined) = tasks.try_join_next() {
                signing.take(joined).await?;
            }
            // Only a read it was asked for counts as the application answering.
            if !asked.is_empty() && quiet_since.elapsed() >= SIGN_DEADLINE {
                // What it was asked and never signed is read some other way.
                signing.metrics.detail("routes_signing_stalled");
                asked.clear();
                open = false;
            }
        }
        Ok(attempt)
    }
    /// Lets no lane ask for another read, fails each request they still make, then stops
    /// pausing. Nothing is left paused: a request released after pausing ends would be a
    /// failed command, which ends the browser session.
    async fn stop_signing(&self, page: &Page) -> Result<()> {
        let stop = call(SIGN, &json!({"action":"stop"}));
        let deadline = Instant::now() + STOP_DEADLINE;
        while Instant::now() < deadline {
            let running = match self.browser.evaluate(&page.id, &stop).await {
                Ok(answer) => answer["running"].as_u64().unwrap_or(0),
                Err(error) if error.is(Code::BrowserScriptFailed) => 0,
                Err(error) => return Err(error),
            };
            let event = self.browser.event(&page.id).await?;
            if !event.is_null() {
                page.command(
                    "Fetch.failRequest",
                    json!({"requestId":event["requestId"],"errorReason":"BlockedByClient"}),
                )
                .await?;
                continue;
            }
            if running == 0 {
                break;
            }
        }
        page.command("Fetch.disable", json!({})).await?;
        // Anything paused since is dropped unanswered: answering it now could be a failed
        // command, which ends the session. At worst one of the application's own requests
        // is left waiting, and a later attempt finds its lane busy and reads by moves.
        while !self.browser.event(&page.id).await?.is_null() {}
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../tests/backend/collections/routes/signed.rs"]
mod tests;
