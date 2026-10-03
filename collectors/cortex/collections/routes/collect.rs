//! A day's routes. The scope comes from discovery, as for meals; the route list, the
//! newer routes page's list and every itinerary are then taken as the page's own data
//! responses, which the page signs, arrive.
//!
//! The list is one page load. Every itinerary after it is one move inside that loaded
//! application: the address changes as a click in the list would change it, and the
//! application fetches just that itinerary. An itinerary the application does not
//! answer for is read by loading its own page instead.
use super::*;
use crate::{
    collectors::cortex::{
        codes,
        discovery::Scope,
        routes::{
            Capture, Collection, ItineraryCapture, MAX_BODY, MAX_ITINERARIES, Request,
            add_capture_bytes, listed,
        },
    },
    db::now,
    job_metrics::Recorder,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::{
    future::Future,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

const SUMMARIES: &str = "/operations/execution/api/summaries";
const ROUTE_SUMMARIES: &str = "/operations/execution/api/route-summaries";
const ITINERARY: &str = "/operations/execution/api/itineraries/";
/// How long a page may take to ask for its data.
const PAGE_DEADLINE: Duration = Duration::from_secs(45);
/// A move that failed for one of these is read by loading the itinerary's page instead:
/// the application did not answer, or was between documents.
const MOVE_FAILED: &[crate::Code] = &[
    codes::ROUTES_MOVE_UNANSWERED,
    crate::Code::BrowserNavigationPending,
    crate::Code::BrowserScriptFailed,
];
/// How long the application may take to answer a move before the page is loaded instead.
const MOVE_DEADLINE: Duration = Duration::from_secs(20);
/// Moves the loaded application to `url` as its own links do: the address changes and
/// the application is told, so it fetches, and signs, what the new address shows.
pub(super) const MOVE: &str = r#"(url)=>{const next=new URL(url);if(next.origin!==location.origin)return false;
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
}
/// How a day is read. The default was measured against the alternatives on a real day of
/// 36 itineraries (`measure_route_method`): moving in the application, with every
/// response withheld from the page, in three background tabs, read the day in about a
/// third of the time, a fifth of the CPU and half the memory of loading each page.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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
}
impl Default for Method {
    fn default() -> Self {
        Self {
            navigation: Navigation::InApp,
            tabs: 3,
            windows: false,
            withhold: true,
        }
    }
}
impl Method {
    /// How days were read before in-app moves: a page load per itinerary, two windows.
    #[cfg(all(test, feature = "operator-probes"))]
    pub(crate) fn reload() -> Self {
        Self {
            navigation: Navigation::Reload,
            tabs: 2,
            windows: true,
            withhold: false,
        }
    }
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
fn owned_by(text: &str, id: &str, transporter: &str) -> bool {
    serde_json::from_str::<Whose>(text)
        .is_ok_and(|w| w.details.itinerary == id && w.details.transporter == transporter)
}

/// The itinerary lanes share one exact byte reservation for the day's accepted
/// responses. A failed reservation leaves the total unchanged.
struct CaptureBudget(AtomicUsize);
impl CaptureBudget {
    fn new(bytes: usize) -> Result<Self> {
        Ok(Self(AtomicUsize::new(add_capture_bytes(0, bytes)?)))
    }
    fn add(&self, bytes: usize) -> Result<()> {
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
impl Driver {
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
    pub(super) async fn collect_routes(
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
    pub(super) async fn collect_routes_with<F, Fut>(
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
        request.validate()?;
        ensure(
            (1..=4).contains(&method.tabs),
            "routes_capture_invalid",
            502,
        )?;
        let started_at = now();
        progress(5, "Finding the station".into()).await?;
        let scope = self
            .resolve_scope(&request.scope_request(), metrics)
            .await?;
        ensure(
            scope.date == request.date && scope.station == request.station,
            "routes_scope_mismatch",
            502,
        )?;
        progress(10, "Reading the route list".into()).await?;
        let origin = self.origin.clone();
        let parse = |text: String| -> Result<Value> {
            serde_json::from_str(&text).map_err(|_| Error::new("cortex_content_incomplete", 502))
        };
        let summaries_body = self
            .capture_page(
                &self.page,
                &format!("{origin}{}", scope.list_path()),
                SUMMARIES,
                false,
                metrics,
            )
            .await?;
        let bytes = CaptureBudget::new(summaries_body.len())?;
        let summaries = parse(summaries_body)?;
        let listed = listed(&summaries, &scope);
        ensure(
            listed.len() <= MAX_ITINERARIES,
            "routes_source_too_large",
            502,
        )?;
        // Loading the routes page leaves the list's application; the original reading
        // reloads every itinerary anyway, so it reads that page first.
        let mut route_summaries = None;
        if method.navigation == Navigation::Reload {
            let body = self
                .capture_page(
                    &self.page,
                    &routes_page(&origin, &scope),
                    ROUTE_SUMMARIES,
                    false,
                    metrics,
                )
                .await?;
            bytes.add(body.len())?;
            route_summaries = Some(parse(body)?);
        }
        progress(15, format!("Reading {} itineraries", listed.len())).await?;
        let mut others = Vec::new();
        for _ in 1..method.tabs.min(listed.len()) {
            let mut page = if method.windows {
                // Its own window stays visible, as a rendered page expects.
                Page::open_window(self.browser.clone(), self.origin.clone()).await?
            } else {
                Page::open(self.browser.clone(), self.origin.clone()).await?
            };
            page.allow_origins(&self.origins.iter().map(String::as_str).collect::<Vec<_>>());
            others.push(page);
        }
        let lanes = Lanes {
            driver: &*self,
            scope: &scope,
            method,
            listed: &listed,
            metrics,
            progress: &progress,
            next: AtomicUsize::new(0),
            done: AtomicUsize::new(0),
            stopped: AtomicBool::new(false),
            bytes,
        };
        // Drain every tab even when one fails. Dropping a sibling's in-flight command
        // closes the shared browser transport. The first tab shows the list's
        // application already; the others load their first itinerary's page.
        let lanes_read = futures_util::future::join_all(
            std::iter::once((&self.page, true))
                .chain(others.iter().map(|page| (page, false)))
                .map(|(page, loaded)| lanes.read(page, loaded)),
        )
        .await;
        let mut itineraries: Vec<Option<ItineraryCapture>> = listed.iter().map(|_| None).collect();
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
        let route_summaries = match route_summaries {
            Some(value) => value,
            None => {
                let body = self.route_summaries(&scope, method, metrics).await?;
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
        progress(95, "Validating routes".into()).await?;
        let capture = Capture {
            version: 1,
            collection: Collection::Routes,
            mode: request.mode,
            scope,
            started_at,
            finished_at: now().max(started_at),
            summaries,
            route_summaries,
            itineraries,
        };
        capture.validate(request)?;
        Ok(capture)
    }
    /// The newer routes page's list: a move in the first tab's application, or the
    /// page loaded when the application does not answer.
    async fn route_summaries(
        &self,
        scope: &Scope,
        method: &Method,
        metrics: &Recorder,
    ) -> Result<String> {
        let url = routes_page(&self.origin, scope);
        self.page
            .command("Fetch.enable", patterns(&[ROUTE_SUMMARIES]))
            .await?;
        let moved = self
            .move_to(&self.page, &url, ROUTE_SUMMARIES, method, metrics, |_| true)
            .await;
        self.stop_pausing(&self.page).await;
        match moved {
            Err(error) if error.is_any(MOVE_FAILED) => {
                metrics.detail("routes_page_reloaded");
                self.capture_page(&self.page, &url, ROUTE_SUMMARIES, false, metrics)
                    .await
            }
            result => result,
        }
    }
}

/// The itineraries still to read, shared by the tabs.
struct Lanes<'a, F> {
    driver: &'a Driver,
    scope: &'a Scope,
    method: &'a Method,
    listed: &'a [(String, String)],
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
        let in_app = self.method.navigation == Navigation::InApp;
        let mut ready = in_app && loaded;
        if ready {
            page.command("Fetch.enable", patterns(&[ITINERARY])).await?;
        }
        let result = self.lane(page, &mut ready).await;
        if ready {
            self.driver.stop_pausing(page).await;
        }
        if result.is_err() {
            self.stopped.store(true, Ordering::SeqCst);
        }
        result
    }
    async fn lane(&self, page: &Page, ready: &mut bool) -> Result<Vec<(usize, ItineraryCapture)>> {
        let driver = self.driver;
        let mut read = Vec::new();
        while !self.stopped.load(Ordering::SeqCst) {
            let index = self.next.fetch_add(1, Ordering::SeqCst);
            let Some((id, transporter)) = self.listed.get(index) else {
                break;
            };
            let ordinal = index + 1;
            self.metrics.page_start(ordinal, 1);
            self.metrics.page_stage(ordinal, "content");
            let mut result = Err(Error::new("routes_move_unanswered", 502));
            if *ready {
                let url = format!("{}{}", driver.origin, self.scope.detail_path(id));
                result = driver
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
                    driver.stop_pausing(page).await;
                    *ready = false;
                    self.metrics
                        .page_finish(ordinal, result.as_ref().err().map(|e| e.code.as_str()));
                    self.metrics.page_start(ordinal, 2);
                    self.metrics.page_stage(ordinal, "content");
                }
                result = driver
                    .itinerary_page(
                        page,
                        self.scope,
                        (id, transporter),
                        self.method,
                        self.metrics,
                    )
                    .await;
                if result.is_ok() && self.method.navigation == Navigation::InApp {
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
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn concurrent_lanes_cannot_overbook_the_capture_budget() {
        let budget = Arc::new(
            CaptureBudget::new(crate::collectors::cortex::routes::MAX_CAPTURE_BYTES - 1).unwrap(),
        );
        let results = std::thread::scope(|scope| {
            let first = scope.spawn({
                let budget = budget.clone();
                move || budget.add(1)
            });
            let second = scope.spawn({
                let budget = budget.clone();
                move || budget.add(1)
            });
            [first.join().unwrap(), second.join().unwrap()]
        });
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert!(results.iter().any(|result| result.is_err()));
    }
    #[test]
    fn an_itinerary_is_owned_by_its_driver_without_reading_the_rest() {
        let text = r#"{"itineraryDetails":{"stops":[{"tasks":[]}],"itineraryId":"a","transporterId":"t"},"addresses":[]}"#;
        assert!(owned_by(text, "a", "t"));
        assert!(!owned_by(text, "a", "other"));
        assert!(!owned_by(text, "b", "t"));
        assert!(!owned_by("{}", "a", "t"));
    }
    #[test]
    fn a_tab_pauses_only_its_data_responses() {
        let value = patterns(&[ITINERARY]);
        let patterns = value["patterns"].as_array().unwrap();
        assert_eq!(patterns.len(), 2);
        assert!(patterns.iter().all(|p| p["requestStage"] == "Response"));
    }
}
