//! A day's routes. The scope comes from discovery, as for meals. The day is read first
//! with no page at all (`native`): the browser's session, and every read signed here with
//! the key the list's page carries; the browser closes once that reads it whole.
//!
//! Otherwise the route list is taken as the page's own data response, which the page
//! signs, arrives: one page load. Every itinerary and the newer routes page's list are
//! then signed by that loaded application and downloaded (`signed`). Whatever that does
//! not read is read by moves inside the application: the address changes as a click in
//! the list would change it, and the application fetches just that itinerary. An
//! itinerary the application does not answer for is read by loading its own page instead.
//!
//! A job reads a day in its driver's tab; a feature reading outside a job, in the browser it
//! opened (`Browser`), the same way.
use super::capture::{
    Capture, Collection, ItineraryCapture, MAX_BODY, MAX_CAPTURE_BYTES, MAX_ITINERARIES, Request,
    add_capture_bytes, listed, token,
};
use super::native::Native;
use crate as cortex;
use crate::{
    codes,
    connection::{Driver, Tab},
    discovery::Scope,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use dispatch_core::{
    Error, Result,
    collection::{
        browser::{
            Run,
            http::SignedIn,
            page::{Page, call},
        },
        metrics::Recorder,
    },
    db::{now, s},
    ensure,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    future::Future,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::Duration,
};
use tokio::time::Instant;

pub(super) const SUMMARIES: &str = "/operations/execution/api/summaries";
pub(super) const ROUTE_SUMMARIES: &str = "/operations/execution/api/route-summaries";
pub(super) const ITINERARY: &str = "/operations/execution/api/itineraries/";
/// Where the application's API action asks for itinerary `id`.
pub(crate) fn itinerary_path(id: &str) -> String {
    format!(
        "{ITINERARY}{}",
        url::form_urlencoded::byte_serialize(id.as_bytes()).collect::<String>()
    )
}
/// How long a page may take to ask for its data.
const PAGE_DEADLINE: Duration = Duration::from_secs(45);
/// A move that failed for one of these is read by loading the itinerary's page instead:
/// the application did not answer, or was between documents.
const MOVE_FAILED: &[dispatch_core::Code] = &[
    codes::ROUTES_MOVE_UNANSWERED,
    dispatch_core::Code::BrowserNavigationPending,
    dispatch_core::Code::BrowserScriptFailed,
];
/// How long the application may take to answer a move before the page is loaded instead.
const MOVE_DEADLINE: Duration = Duration::from_secs(20);
/// Moves the loaded application to `url` as its own links do: the address changes and
/// the application is told, so it fetches, and signs, what the new address shows.
pub(crate) const MOVE: &str = r#"(url)=>{const next=new URL(url);if(next.origin!==location.origin)return false;
  history.pushState(history.state,'',next.pathname+next.search);
  dispatchEvent(new PopStateEvent('popstate',{state:history.state}));return true;}"#;

/// How a day's itineraries are read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Navigation {
    /// Each itinerary's own page is loaded.
    Reload,
    /// The list is loaded once per tab and the application moves between itineraries.
    InApp,
    /// The list's application signs every read, which is downloaded; what that does not
    /// read is read as `InApp` does.
    Signed,
    /// Every read is signed here, with no page; a day that does not read whole is read as
    /// `Signed` reads it.
    Native,
}
/// How a day is read. The default was measured against the alternatives on real days.
/// Signed here and downloaded twelve at a time, with no page, a day of 46 itineraries read
/// in 5.7 s and half a CPU-second, where the application's own signatures took 8.2 s and
/// 8.4 CPU-seconds with the browser at 611 MB. Those, on a day of 41
/// (`measure_route_method`), took under a third of the time and CPU of moving in the
/// application in three background tabs with every response withheld, which itself took
/// a quarter of the time and a tenth of the CPU of loading each page.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub(crate) struct Method {
    pub navigation: Navigation,
    /// Tabs reading itineraries at once.
    pub tabs: usize,
    /// Extra tabs each get a window of their own rather than a background tab. A
    /// withheld response needs no window: nothing is rendered.
    pub windows: bool,
    /// A captured response is failed for the page, which then never parses or renders
    /// it.
    pub withhold: bool,
    /// Reads the application signs downloaded at once.
    pub downloads: usize,
}
impl Default for Method {
    fn default() -> Self {
        Self {
            navigation: Navigation::Native,
            tabs: 3,
            windows: false,
            withhold: true,
            downloads: 12,
        }
    }
}
impl Method {
    /// How days were read before in-app moves: a page load per itinerary, two windows.
    #[cfg(feature = "operator-probes")]
    pub(crate) fn reload() -> Self {
        Self {
            navigation: Navigation::Reload,
            tabs: 2,
            windows: true,
            withhold: false,
            ..Self::default()
        }
    }
}

/// What a job's metrics call a day read natively.
pub const NATIVE: &str = "routes_native";
/// Why a day read whole natively was not kept: it failed the collection's checks.
pub const INVALID: &str = "routes_native_invalid";
/// A day read with a browser, and how.
pub struct Day {
    pub capture: Capture,
    /// What read it, as a job's metrics name it: `routes_native`, `routes_signed` or
    /// `routes_signed_moved`. None in fixture mode, as a fixture job names none.
    pub method: Option<&'static str>,
    /// Why it was not read natively, when it was read another way: `Failure::label`, or
    /// `routes_native_invalid` for a day read whole that failed the collection's checks.
    pub fallback: Option<&'static str>,
    /// A reader to read the next with no browser (`Native::read`): the one that read the day
    /// natively, with the key it found; read another way, one carrying the browser's session as
    /// the read left it (`Browser::read`). None in fixture mode, and for a job, which reads
    /// no next.
    pub native: Option<Native>,
}

/// Only the owner of an itinerary response, read without building the whole tree.
#[derive(Deserialize)]
struct Whose {
    #[serde(rename = "itineraryDetails")]
    details: WhoseDetails,
}
#[derive(Deserialize)]
struct WhoseDetails {
    #[serde(rename = "itineraryId")]
    itinerary: String,
    #[serde(rename = "transporterId")]
    transporter: String,
}
pub(super) fn owned_by(text: &str, id: &str, transporter: &str) -> bool {
    serde_json::from_str::<Whose>(text)
        .is_ok_and(|w| w.details.itinerary == id && w.details.transporter == transporter)
}

/// The itinerary lanes share one exact byte reservation for the day's accepted
/// responses. A failed reservation leaves the total unchanged.
pub(super) struct CaptureBudget(AtomicUsize);
impl CaptureBudget {
    pub(super) fn new(bytes: usize) -> Result<Self> {
        Ok(Self(AtomicUsize::new(add_capture_bytes(0, bytes)?)))
    }
    /// What the day may still hold.
    pub(super) fn remaining(&self) -> usize {
        MAX_CAPTURE_BYTES.saturating_sub(self.0.load(Ordering::SeqCst))
    }
    pub(super) fn add(&self, bytes: usize) -> Result<()> {
        self.0
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |total| {
                add_capture_bytes(total, bytes).ok()
            })
            .map(|_| ())
            .map_err(|_| Error::new("routes_source_too_large", 502))
    }
}

/// The newer routes page for the scope.
fn routes_page(origin: &str, scope: &Scope) -> String {
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    query
        .append_pair("navMenuVariant", "external")
        .append_pair("provider", &scope.provider)
        .append_pair("selectedDay", &scope.date)
        .append_pair("serviceAreaId", &scope.service_area_id);
    format!("{origin}/operations/execution/dv/routes?{}", query.finish())
}
/// What a tab pauses: data responses under `paths`.
fn patterns(paths: &[&str]) -> Value {
    let patterns: Vec<Value> = paths
        .iter()
        .flat_map(|path| {
            ["XHR", "Fetch"].map(|kind| {
                json!({"urlPattern":format!("*{path}*"),"resourceType":kind,"requestStage":"Response"})
            })
        })
        .collect();
    json!({ "patterns": patterns })
}
impl Tab<'_> {
    /// Lets one paused response go on, or hands it back when it is a data response under
    /// `path` from the application's origin.
    async fn settle(&self, page: &Page, event: &Value, path: &str) -> Result<Option<Value>> {
        let matched = url::Url::parse(s(&event["request"], "url")).is_ok_and(|u| {
            u.origin().ascii_serialization() == self.origin && u.path().starts_with(path)
        });
        if matched && event["responseStatusCode"].is_u64() {
            return Ok(Some(event.clone()));
        }
        self.release(page, &event["requestId"], false).await;
        Ok(None)
    }
    /// Sends a paused response on to the page, or fails it so the page never renders it.
    async fn release(&self, page: &Page, request: &Value, withhold: bool) {
        let released = if withhold {
            page.command(
                "Fetch.failRequest",
                json!({"requestId":request,"errorReason":"Aborted"}),
            )
            .await
        } else {
            page.command("Fetch.continueResponse", json!({"requestId":request}))
                .await
        };
        if released.is_err() {
            let _ = page
                .command("Fetch.continueRequest", json!({"requestId":request}))
                .await;
        }
    }
    /// The body of a paused data response, as JSON text, then the response released.
    async fn body(
        &self,
        page: &Page,
        event: &Value,
        withhold: bool,
        metrics: &Recorder,
    ) -> Result<String> {
        let status = event["responseStatusCode"].as_u64().unwrap_or(0);
        let body = if status == 200 {
            page.command(
                "Fetch.getResponseBody",
                json!({"requestId":event["requestId"]}),
            )
            .await
        } else {
            Err(Error::new("cortex_content_incomplete", 502))
        };
        self.release(page, &event["requestId"], withhold && status == 200)
            .await;
        if status == 401 || status == 403 {
            metrics.detail("routes_signed_out");
            return Err(Error::new("verification_required", 409));
        }
        ensure(status == 200, "provider_unavailable", 502)?;
        let body = body?;
        let bytes = if body["base64Encoded"] == true {
            STANDARD
                .decode(s(&body, "body"))
                .map_err(|_| Error::new("cortex_content_incomplete", 502))?
        } else {
            s(&body, "body").as_bytes().to_vec()
        };
        ensure(bytes.len() <= MAX_BODY, "routes_source_too_large", 502)?;
        let text =
            String::from_utf8(bytes).map_err(|_| Error::new("cortex_content_incomplete", 502))?;
        ensure(
            text.trim_start().starts_with('{'),
            "cortex_content_incomplete",
            502,
        )?;
        Ok(text)
    }
    /// Stops pausing the tab's requests and lets every one still paused go.
    async fn stop_pausing(&self, page: &Page) {
        let _ = page.command("Fetch.disable", json!({})).await;
        // A request still paused when interception ends must be let go, or the page hangs.
        while let Ok(event) = self.browser.event(&page.id).await {
            if event.is_null() {
                break;
            }
            self.release(page, &event["requestId"], false).await;
        }
    }
    /// Waits for the next data response under `path`, settling every other paused request.
    async fn next_response(
        &self,
        page: &Page,
        path: &str,
        deadline: Instant,
        late: &'static str,
    ) -> Result<Value> {
        loop {
            ensure(Instant::now() < deadline, late, 502)?;
            let event = self.browser.event(&page.id).await?;
            if event.is_null() {
                continue;
            }
            if let Some(event) = self.settle(page, &event, path).await? {
                return Ok(event);
            }
        }
    }
    /// Loads `url` in `page` and takes the first data response under `path` the page
    /// receives, as JSON.
    async fn capture_page(
        &self,
        page: &Page,
        url: &str,
        path: &str,
        withhold: bool,
        metrics: &Recorder,
    ) -> Result<String> {
        page.command("Fetch.enable", patterns(&[path])).await?;
        let result = async {
            page.start_navigation(url).await?;
            let event = self
                .next_response(
                    page,
                    path,
                    Instant::now() + PAGE_DEADLINE,
                    "cortex_content_incomplete",
                )
                .await?;
            self.body(page, &event, withhold, metrics).await
        }
        .await;
        self.stop_pausing(page).await;
        result
    }
    /// One itinerary's detail from its own page, loaded again once when the page
    /// answered with another's.
    async fn itinerary_page(
        &self,
        page: &Page,
        scope: &Scope,
        (id, transporter): (&str, &str),
        method: &Method,
        metrics: &Recorder,
    ) -> Result<String> {
        let url = format!("{}{}", self.origin, scope.detail_path(id));
        for attempt in 0..2 {
            let detail = self
                .capture_page(page, &url, ITINERARY, method.withhold, metrics)
                .await?;
            if owned_by(&detail, id, transporter) {
                return Ok(detail);
            }
            metrics.detail(if attempt == 0 {
                "route_changed"
            } else {
                "route_mismatch"
            });
        }
        Err(Error::new("routes_scope_mismatch", 502))
    }
    /// Moves the tab's loaded application to `url` and takes the data response under
    /// `path` it answers with that `accepts`. Earlier moves' late answers are let go.
    async fn move_to(
        &self,
        page: &Page,
        url: &str,
        path: &str,
        method: &Method,
        metrics: &Recorder,
        accepts: impl Fn(&str) -> bool,
    ) -> Result<String> {
        let moved = page.evaluate(&call(MOVE, &json!(url))).await?;
        ensure(moved == true, "routes_move_unanswered", 502)?;
        let deadline = Instant::now() + MOVE_DEADLINE;
        loop {
            let event = self
                .next_response(page, path, deadline, "routes_move_unanswered")
                .await?;
            let text = self.body(page, &event, method.withhold, metrics).await?;
            if accepts(&text) {
                return Ok(text);
            }
            metrics.detail("route_changed");
        }
    }
    /// The scope of the day `request` names: the service area and provider it names, or
    /// found on Cortex as discovery finds them.
    pub(super) async fn routes_scope(
        &mut self,
        request: &Request,
        metrics: &Recorder,
    ) -> Result<Scope> {
        request.validate()?;
        self.resolve_scope(&request.scope_request(), metrics).await
    }
    /// The day `request` names, in `scope`, as `method` reads it. A day read natively closes
    /// the browser once it is read whole and checked: nothing needs it after.
    pub(super) async fn read_routes<F, Fut>(
        &mut self,
        request: &Request,
        scope: &Scope,
        started_at: i64,
        method: &Method,
        metrics: &Recorder,
        progress: F,
    ) -> Result<Day>
    where
        F: Fn(i64, String) -> Fut,
        Fut: Future<Output = Result<()>>,
    {
        request.validate()?;
        ensure(
            (1..=4).contains(&method.tabs) && (1..=16).contains(&method.downloads),
            "routes_capture_invalid",
            502,
        )?;
        // The day's, as discovery checks what it finds: one its caller kept too.
        request.scope_request().validate_scope(scope)?;
        progress(10, "Reading the route list".into()).await?;
        let mut method = *method;
        let mut fallback = None;
        if method.navigation == Navigation::Native {
            let session = SignedIn::of(self.browser, self.origin, &cortex::HOSTS).await?;
            // A day is checked before the browser closes: one that fails is read with it.
            let (reader, read) = read_natively(&session, request, scope, &progress).await?;
            match read {
                Ok(capture) => {
                    // Nothing needs the browser now: it closes before the day is kept.
                    self.browser.close().await;
                    metrics.method(NATIVE);
                    return Ok(Day {
                        capture,
                        method: Some(NATIVE),
                        fallback: None,
                        native: Some(reader),
                    });
                }
                // The reader that failed is let go: its session may not outlast the read.
                Err(label) => {
                    metrics.detail(label);
                    metrics.fallback(label);
                    fallback = Some(label);
                    method.navigation = Navigation::Signed;
                }
            }
        }
        let method = &method;
        self.ensure_page().await?;
        let origin = self.origin;
        let parse = |text: String| -> Result<Value> {
            serde_json::from_str(&text).map_err(|_| Error::new("cortex_content_incomplete", 502))
        };
        let summaries_body = self
            .capture_page(
                self.page,
                &format!("{origin}{}", scope.list_path()),
                SUMMARIES,
                false,
                metrics,
            )
            .await?;
        let bytes = CaptureBudget::new(summaries_body.len())?;
        let summaries = parse(summaries_body)?;
        let listed = listed(&summaries, scope);
        ensure(
            listed.len() <= MAX_ITINERARIES,
            "routes_source_too_large",
            502,
        )?;
        // As the capture is checked at the end; every read asks by these.
        ensure(
            listed
                .iter()
                .all(|(id, transporter)| token(id, 128) && token(transporter, 64)),
            "routes_scope_mismatch",
            502,
        )?;
        // Loading the routes page leaves the list's application; the original reading
        // reloads every itinerary anyway, so it reads that page first.
        let mut route_summaries: Option<Value> = None;
        if method.navigation == Navigation::Reload {
            let body = self
                .capture_page(
                    self.page,
                    &routes_page(origin, scope),
                    ROUTE_SUMMARIES,
                    false,
                    metrics,
                )
                .await?;
            bytes.add(body.len())?;
            route_summaries = Some(parse(body)?);
        }
        progress(15, format!("Reading {} itineraries", listed.len())).await?;
        let mut itineraries: Vec<Option<ItineraryCapture>> = vec![None; listed.len()];
        if method.navigation == Navigation::Signed {
            let read = self
                .read_signed(scope, &listed, method.downloads, &bytes, metrics, &progress)
                .await?;
            itineraries = read.itineraries;
            route_summaries = read.routes;
        }
        let todo: Vec<usize> = (0..listed.len())
            .filter(|index| itineraries[*index].is_none())
            .collect();
        if method.navigation == Navigation::Signed && !todo.is_empty() {
            metrics.detail("routes_signed_moved");
        }
        let mut others = Vec::new();
        for _ in 1..method.tabs.min(todo.len()) {
            let mut page = if method.windows {
                // Its own window stays visible, as a rendered page expects.
                Page::open_window(self.browser.clone(), origin.to_owned()).await?
            } else {
                Page::open(self.browser.clone(), origin.to_owned()).await?
            };
            page.allow_origins(&self.origins.iter().map(String::as_str).collect::<Vec<_>>());
            others.push(page);
        }
        let lanes = Lanes {
            tab: &*self,
            scope,
            method,
            listed: &listed,
            todo: &todo,
            metrics,
            progress: &progress,
            next: AtomicUsize::new(0),
            done: AtomicUsize::new(listed.len() - todo.len()),
            stopped: AtomicBool::new(false),
            bytes,
        };
        // Drain every tab even when one fails. Dropping a sibling's in-flight command
        // closes the shared browser transport. The first tab shows the list's
        // application already; the others load their first itinerary's page.
        let lanes_read = if todo.is_empty() {
            Vec::new()
        } else {
            futures_util::future::join_all(
                std::iter::once((&*self.page, true))
                    .chain(others.iter().map(|page| (page, false)))
                    .map(|(page, loaded)| lanes.read(page, loaded)),
            )
            .await
        };
        let mut failure = None;
        for lane in lanes_read {
            match lane {
                Ok(read) => {
                    for (index, captured) in read {
                        itineraries[index] = Some(captured);
                    }
                }
                Err(error) => failure = failure.or(Some(error)),
            }
        }
        for page in &others {
            let _ = page.close().await;
        }
        if let Some(error) = failure {
            return Err(error);
        }
        // Whether moves read any of the day, after the application's signatures.
        let moved = !todo.is_empty() || route_summaries.is_none();
        let route_summaries = match route_summaries {
            Some(value) => value,
            None => {
                let body = self.route_summaries(scope, method, metrics).await?;
                lanes.bytes.add(body.len())?;
                parse(body)?
            }
        };
        ensure(
            route_summaries["rmsRouteSummaries"].is_array(),
            "cortex_content_incomplete",
            502,
        )?;
        let itineraries = itineraries
            .into_iter()
            .map(|c| c.ok_or_else(|| Error::new("routes_capture_invalid", 502)))
            .collect::<Result<Vec<_>>>()?;
        // The application's signatures alone read the day, or moves as well.
        let read_by = (method.navigation == Navigation::Signed).then_some(if moved {
            "routes_signed_moved"
        } else {
            "routes_signed"
        });
        if let Some(label) = read_by {
            metrics.method(label);
        }
        progress(95, "Validating routes".into()).await?;
        let capture = Capture {
            version: 1,
            collection: Collection::Routes,
            mode: request.mode,
            scope: scope.clone(),
            started_at,
            finished_at: now().max(started_at),
            summaries,
            route_summaries,
            itineraries,
        };
        capture.validate(request)?;
        Ok(Day {
            capture,
            method: read_by,
            fallback,
            native: None,
        })
    }
    /// The newer routes page's list: a move in the first tab's application, or the
    /// page loaded when the application does not answer.
    async fn route_summaries(
        &self,
        scope: &Scope,
        method: &Method,
        metrics: &Recorder,
    ) -> Result<String> {
        let url = routes_page(self.origin, scope);
        self.page
            .command("Fetch.enable", patterns(&[ROUTE_SUMMARIES]))
            .await?;
        let moved = self
            .move_to(self.page, &url, ROUTE_SUMMARIES, method, metrics, |_| true)
            .await;
        self.stop_pausing(self.page).await;
        match moved {
            Err(error) if error.is_any(MOVE_FAILED) => {
                metrics.detail("routes_page_reloaded");
                self.capture_page(self.page, &url, ROUTE_SUMMARIES, false, metrics)
                    .await
            }
            result => result,
        }
    }
}

/// The day `request` names, in `scope`, read natively with `session` and checked as every
/// capture is: the reader, to keep and read the next with, and the day, or why it is not
/// kept: what `Failure::label` calls it, or `routes_native_invalid` for a day read whole
/// that failed the checks.
pub(super) async fn read_natively<F, Fut>(
    session: &SignedIn,
    request: &Request,
    scope: &Scope,
    progress: &F,
) -> Result<(Native, std::result::Result<Capture, &'static str>)>
where
    F: Fn(i64, String) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    let mut reader = Native::new(session)?;
    let read = match reader.read(scope, request.mode).await {
        Ok(capture) => {
            progress(95, "Validating routes".into()).await?;
            capture
                .validate(request)
                .map(|()| capture)
                .map_err(|_| INVALID)
        }
        Err(failure) => Err(failure.label()),
    };
    Ok((reader, read))
}

impl Driver {
    pub(crate) async fn collect_routes(
        &mut self,
        request: &Request,
        run: &Run<'_>,
    ) -> Result<Capture> {
        self.collect_routes_with(
            request,
            &Method::default(),
            run.metrics,
            |progress, message| run.progress(progress, message),
        )
        .await
    }
    /// The day `request` names, its scope found and the day read as `method` reads it.
    pub(crate) async fn collect_routes_with<F, Fut>(
        &mut self,
        request: &Request,
        method: &Method,
        metrics: &Recorder,
        progress: F,
    ) -> Result<Capture>
    where
        F: Fn(i64, String) -> Fut,
        Fut: Future<Output = Result<()>>,
    {
        let started_at = now();
        progress(5, "Finding the station".into()).await?;
        let mut tab = self.tab();
        let scope = tab.routes_scope(request, metrics).await?;
        let day = tab
            .read_routes(request, &scope, started_at, method, metrics, progress)
            .await?;
        Ok(day.capture)
    }
}

/// The itineraries still to read, shared by the tabs.
struct Lanes<'a, F> {
    tab: &'a Tab<'a>,
    scope: &'a Scope,
    method: &'a Method,
    listed: &'a [(String, String)],
    /// The places in `listed` still to read.
    todo: &'a [usize],
    metrics: &'a Recorder,
    progress: &'a F,
    next: AtomicUsize,
    done: AtomicUsize,
    stopped: AtomicBool,
    bytes: CaptureBudget,
}
impl<F, Fut> Lanes<'_, F>
where
    F: Fn(i64, String) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    /// Reads itineraries in one tab until none are left. `loaded` is whether the tab
    /// already shows the application it can move within.
    async fn read(&self, page: &Page, loaded: bool) -> Result<Vec<(usize, ItineraryCapture)>> {
        let in_app = self.method.navigation != Navigation::Reload;
        let mut ready = in_app && loaded;
        if ready {
            page.command("Fetch.enable", patterns(&[ITINERARY])).await?;
        }
        let result = self.lane(page, &mut ready).await;
        if ready {
            self.tab.stop_pausing(page).await;
        }
        if result.is_err() {
            self.stopped.store(true, Ordering::SeqCst);
        }
        result
    }
    async fn lane(&self, page: &Page, ready: &mut bool) -> Result<Vec<(usize, ItineraryCapture)>> {
        let tab = self.tab;
        let mut read = Vec::new();
        while !self.stopped.load(Ordering::SeqCst) {
            let Some(&index) = self.todo.get(self.next.fetch_add(1, Ordering::SeqCst)) else {
                break;
            };
            let (id, transporter) = &self.listed[index];
            let ordinal = index + 1;
            self.metrics.page_start(ordinal, 1);
            self.metrics.page_stage(ordinal, "content");
            let mut result = Err(Error::new("routes_move_unanswered", 502));
            if *ready {
                let url = format!("{}{}", tab.origin, self.scope.detail_path(id));
                result = tab
                    .move_to(page, &url, ITINERARY, self.method, self.metrics, |text| {
                        owned_by(text, id, transporter)
                    })
                    .await;
                if result.is_ok() {
                    self.metrics.direct();
                }
            }
            if result.as_ref().err().is_some_and(|e| e.is_any(MOVE_FAILED)) {
                // Not moved, or not answered: load the itinerary's own page, which
                // leaves the tab showing an application to move within.
                if *ready {
                    tab.stop_pausing(page).await;
                    *ready = false;
                    self.metrics
                        .page_finish(ordinal, result.as_ref().err().map(|e| e.code.as_str()));
                    self.metrics.page_start(ordinal, 2);
                    self.metrics.page_stage(ordinal, "content");
                }
                result = tab
                    .itinerary_page(
                        page,
                        self.scope,
                        (id, transporter),
                        self.method,
                        self.metrics,
                    )
                    .await;
                if result.is_ok() && self.method.navigation != Navigation::Reload {
                    page.command("Fetch.enable", patterns(&[ITINERARY])).await?;
                    *ready = true;
                }
            }
            self.metrics
                .page_finish(ordinal, result.as_ref().err().map(|e| e.code.as_str()));
            let detail = result?;
            self.bytes.add(detail.len())?;
            let finished = self.done.fetch_add(1, Ordering::SeqCst) + 1;
            (self.progress)(
                15 + (75 * finished / self.listed.len().max(1)) as i64,
                format!("Reading itineraries ({finished}/{})", self.listed.len()),
            )
            .await?;
            read.push((
                index,
                ItineraryCapture {
                    id: id.clone(),
                    transporter_id: transporter.clone(),
                    detail,
                },
            ));
            // A reloaded page keeps what it rendered until collected; a withheld one never
            // rendered anything.
            if !self.method.withhold && read.len().is_multiple_of(10) {
                page.collect_garbage().await?;
            }
        }
        Ok(read)
    }
}

#[cfg(test)]
#[path = "../../tests/backend/collections/routes/collect.rs"]
mod tests;
