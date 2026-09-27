//! A day's routes. The scope comes from discovery, as for meals; the route list, the
//! newer routes page's list and every itinerary are then taken as the page's own data
//! responses, which the page signs, arrive. Two tabs read the itineraries.
use super::{collection::TABS, *};
use crate::{
    db::now,
    itineraries::{
        Capture, Collection, ItineraryCapture, MAX_BODY, MAX_ITINERARIES, Request, listed,
    },
    job_metrics::Recorder,
    meals::Scope,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

const SUMMARIES: &str = "/operations/execution/api/summaries";
const ROUTE_SUMMARIES: &str = "/operations/execution/api/route-summaries";
const ITINERARY: &str = "/operations/execution/api/itineraries/";
/// How long a page may take to ask for its data.
const PAGE_DEADLINE: Duration = Duration::from_secs(45);

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
impl Driver {
    /// Loads `url` in `page` and takes the first data response under `path` the page
    /// receives, as JSON. Responses are paused as they arrive and let go once read, so
    /// the page sees them unchanged.
    async fn capture_page(
        &self,
        page: &Page,
        url: &str,
        path: &str,
        metrics: &Recorder,
    ) -> Result<String> {
        let patterns: Vec<Value> = ["XHR", "Fetch"]
            .iter()
            .map(|kind| json!({"urlPattern":format!("*{path}*"),"resourceType":kind,"requestStage":"Response"}))
            .collect();
        page.command("Fetch.enable", json!({"patterns":patterns}))
            .await?;
        let result = self.observe_page(page, url, path, metrics).await;
        let _ = page.command("Fetch.disable", json!({})).await;
        // A response still paused when interception ends must be let go, or the page hangs.
        while let Ok(event) = self.browser.event(&page.id).await {
            if event.is_null() {
                break;
            }
            let _ = page
                .command(
                    "Fetch.continueResponse",
                    json!({"requestId":event["requestId"]}),
                )
                .await;
        }
        result
    }
    async fn observe_page(
        &self,
        page: &Page,
        url: &str,
        path: &str,
        metrics: &Recorder,
    ) -> Result<String> {
        page.start_navigation(url).await?;
        let deadline = Instant::now() + PAGE_DEADLINE;
        loop {
            ensure(Instant::now() < deadline, "cortex_content_incomplete", 502)?;
            let event = self.browser.event(&page.id).await?;
            if event.is_null() {
                continue;
            }
            let request_id = event["requestId"].clone();
            let matched = url::Url::parse(s(&event["request"], "url")).is_ok_and(|u| {
                u.origin().ascii_serialization() == self.origin && u.path().starts_with(path)
            });
            let status = event["responseStatusCode"].as_u64();
            let body = if matched && status.is_some() {
                page.command("Fetch.getResponseBody", json!({"requestId":request_id}))
                    .await
            } else {
                Err(Error::new("cortex_content_incomplete", 502))
            };
            let continued = page
                .command("Fetch.continueResponse", json!({"requestId":request_id}))
                .await;
            if continued.is_err() {
                let _ = page
                    .command("Fetch.continueRequest", json!({"requestId":request_id}))
                    .await;
            }
            if !matched || status.is_none() {
                continue;
            }
            let status = status.unwrap_or(0);
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
            let text = String::from_utf8(bytes)
                .map_err(|_| Error::new("cortex_content_incomplete", 502))?;
            ensure(
                text.trim_start().starts_with('{'),
                "cortex_content_incomplete",
                502,
            )?;
            return Ok(text);
        }
    }
    /// One itinerary's detail, read again once when the page answered with another's.
    async fn itinerary_detail(
        &self,
        page: &Page,
        scope: &Scope,
        id: &str,
        transporter: &str,
        metrics: &Recorder,
    ) -> Result<String> {
        let url = format!("{}{}", self.origin, scope.detail_path(id));
        for attempt in 0..2 {
            let detail = self.capture_page(page, &url, ITINERARY, metrics).await?;
            // Parsed once to check whose it is, then dropped: the text is what is kept.
            let parsed: Value = serde_json::from_str(&detail)
                .map_err(|_| Error::new("cortex_content_incomplete", 502))?;
            let inner = &parsed["itineraryDetails"];
            if inner["itineraryId"] == json!(id) && inner["transporterId"] == json!(transporter) {
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
    pub(super) async fn collect_routes(
        &mut self,
        request: &Request,
        run: &Run<'_>,
    ) -> Result<Capture> {
        request.validate()?;
        let started_at = now();
        run.progress(5, "Finding the station".into()).await?;
        let scope = self
            .resolve_scope(&request.scope_request(), run.metrics)
            .await?;
        ensure(
            scope.date == request.date && scope.station == request.station,
            "routes_scope_mismatch",
            502,
        )?;
        run.progress(10, "Reading the route list".into()).await?;
        let origin = self.origin.clone();
        let parse = |text: String| -> Result<Value> {
            serde_json::from_str(&text).map_err(|_| Error::new("cortex_content_incomplete", 502))
        };
        let summaries = parse(
            self.capture_page(
                &self.page,
                &format!("{origin}{}", scope.list_path()),
                SUMMARIES,
                run.metrics,
            )
            .await?,
        )?;
        let route_summaries = parse(
            self.capture_page(
                &self.page,
                &routes_page(&origin, &scope),
                ROUTE_SUMMARIES,
                run.metrics,
            )
            .await?,
        )?;
        ensure(
            route_summaries["rmsRouteSummaries"].is_array(),
            "cortex_content_incomplete",
            502,
        )?;
        let listed = listed(&summaries, &scope);
        ensure(
            listed.len() <= MAX_ITINERARIES,
            "routes_source_too_large",
            502,
        )?;
        run.progress(15, format!("Reading {} itineraries", listed.len()))
            .await?;
        // Tabs beside the first, each in its own window: Cortex can stop loading a
        // route in a hidden background tab.
        let mut others = Vec::new();
        for _ in 1..TABS.min(listed.len()) {
            let mut page = Page::open_window(self.browser.clone(), self.origin.clone()).await?;
            page.allow_origins(&self.origins.iter().map(String::as_str).collect::<Vec<_>>());
            others.push(page);
        }
        let lanes = Lanes {
            driver: &*self,
            scope: &scope,
            listed: &listed,
            run,
            next: AtomicUsize::new(0),
            done: AtomicUsize::new(0),
            stopped: AtomicBool::new(false),
        };
        // Drain every tab even when one fails. Dropping a sibling's in-flight command
        // closes the shared browser transport.
        let lanes = futures_util::future::join_all(
            std::iter::once(&self.page)
                .chain(&others)
                .map(|page| lanes.read(page)),
        )
        .await;
        for page in &others {
            let _ = page.close().await;
        }
        let mut itineraries: Vec<Option<ItineraryCapture>> = listed.iter().map(|_| None).collect();
        for lane in lanes {
            for (index, captured) in lane? {
                itineraries[index] = Some(captured);
            }
        }
        let itineraries = itineraries
            .into_iter()
            .map(|c| c.ok_or_else(|| Error::new("routes_capture_invalid", 502)))
            .collect::<Result<Vec<_>>>()?;
        run.progress(95, "Validating routes".into()).await?;
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
            prepared: None,
        };
        capture.validate(request)?;
        Ok(capture)
    }
}

/// The itineraries still to read, shared by the tabs.
struct Lanes<'a> {
    driver: &'a Driver,
    scope: &'a Scope,
    listed: &'a [(String, String)],
    run: &'a Run<'a>,
    next: AtomicUsize,
    done: AtomicUsize,
    stopped: AtomicBool,
}
impl Lanes<'_> {
    async fn read(&self, page: &Page) -> Result<Vec<(usize, ItineraryCapture)>> {
        let mut read = Vec::new();
        while !self.stopped.load(Ordering::SeqCst) {
            let index = self.next.fetch_add(1, Ordering::SeqCst);
            let Some((id, transporter)) = self.listed.get(index) else {
                break;
            };
            let ordinal = index + 1;
            self.run.metrics.page_start(ordinal, 1);
            self.run.metrics.page_stage(ordinal, "content");
            let result = self
                .driver
                .itinerary_detail(page, self.scope, id, transporter, self.run.metrics)
                .await;
            self.run
                .metrics
                .page_finish(ordinal, result.as_ref().err().map(|e| e.code.as_str()));
            let detail = match result {
                Ok(detail) => detail,
                Err(error) => {
                    self.stopped.store(true, Ordering::SeqCst);
                    return Err(error);
                }
            };
            let finished = self.done.fetch_add(1, Ordering::SeqCst) + 1;
            self.run
                .progress(
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
            if read.len().is_multiple_of(10) {
                page.collect_garbage().await?;
            }
        }
        Ok(read)
    }
}
