//! Route data read from page loads and in-app moves, with interception cleanup kept here.
use super::{
    Method,
    capture::MAX_BODY,
    protocol::{ITINERARY, ROUTE_SUMMARIES, owned_by},
};
use crate::{codes, connection::Tab, discovery::Scope};
use base64::{Engine, engine::general_purpose::STANDARD};
use dispatch_core::{
    Error, Result,
    collection::{
        browser::page::{Page, call},
        metrics::Recorder,
    },
    db::s,
    ensure,
};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::time::Instant;

/// How long a page may take to ask for its data.
const PAGE_DEADLINE: Duration = Duration::from_secs(45);
/// A move that failed for one of these is read by loading the itinerary's page instead:
/// the application did not answer, or was between documents.
pub(super) const MOVE_FAILED: &[dispatch_core::Code] = &[
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

/// The newer routes page for the scope.
pub(super) fn routes_page(origin: &str, scope: &Scope) -> String {
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    query
        .append_pair("navMenuVariant", "external")
        .append_pair("provider", &scope.provider)
        .append_pair("selectedDay", &scope.date)
        .append_pair("serviceAreaId", &scope.service_area_id);
    format!("{origin}/operations/execution/dv/routes?{}", query.finish())
}
/// What a tab pauses: data responses under `paths`.
pub(super) fn patterns(paths: &[&str]) -> Value {
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
    pub(super) async fn stop_pausing(&self, page: &Page) {
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
    pub(super) async fn capture_page(
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
    pub(super) async fn itinerary_page(
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
    pub(super) async fn move_to(
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
    /// The newer routes page's list: a move in the first tab's application, or the
    /// page loaded when the application does not answer.
    pub(super) async fn route_summaries(
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
