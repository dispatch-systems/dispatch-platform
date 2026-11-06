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
use super::{
    Day, INVALID, Method, NATIVE, Navigation,
    capture::{Capture, Collection, ItineraryCapture, MAX_ITINERARIES, Request, listed, token},
    native::Native,
    page::{MOVE_FAILED, patterns, routes_page},
    protocol::{CaptureBudget, ITINERARY, ROUTE_SUMMARIES, SUMMARIES, owned_by},
};
use crate as cortex;
use crate::{
    connection::{Driver, Tab},
    discovery::Scope,
};
use dispatch_core::{
    Error, Result,
    collection::{
        browser::{Run, http::SignedIn, page::Page},
        metrics::Recorder,
    },
    db::now,
    ensure,
};
use serde_json::Value;
use std::{
    future::Future,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

impl Tab<'_> {
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
            match read_natively(&session, request, scope, &progress).await? {
                Ok((reader, capture)) => {
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
}

/// The day `request` names, in `scope`, read natively with `session` and checked as every
/// capture is: the day, with the reader that read it, to keep and read the next with; or why
/// it is not kept: what `Failure::label` calls it, or `routes_native_invalid` for a day read
/// whole that failed the checks. A reader that failed is let go, as reading another way may
/// change the browser's session.
pub(super) async fn read_natively<F, Fut>(
    session: &SignedIn,
    request: &Request,
    scope: &Scope,
    progress: &F,
) -> Result<std::result::Result<(Native, Capture), &'static str>>
where
    F: Fn(i64, String) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    let mut reader = Native::new(session)?;
    Ok(match reader.read(scope, request.mode).await {
        Ok(capture) => {
            progress(95, "Validating routes".into()).await?;
            match capture.validate(request) {
                Ok(()) => Ok((reader, capture)),
                Err(_) => Err(INVALID),
            }
        }
        Err(failure) => Err(failure.label()),
    })
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
