use super::*;
use crate::{
    collectors::cortex::{
        discovery::Scope,
        meals::{Capture, Itinerary},
    },
    db::now,
    job_metrics::Recorder,
    live_collection::Writer,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    future::Future,
    sync::{
        LazyLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
// The route's content has not settled yet; read it again.
pub(super) const CONTENT_NOT_READY: &[crate::Code] = &[
    crate::Code::CortexContentIncomplete,
    crate::Code::BrowserNavigationPending,
    crate::Code::BrowserScriptFailed,
    crate::Code::CortexScopeMismatch,
    crate::Code::VerificationRequired,
];
/// A page script without the trailing `;` its formatter adds, so it can be called.
fn script(source: &str) -> &str {
    source.trim().trim_end_matches(';')
}
const RULES: &str = include_str!("../../scripts/meal_rules.js");
/// `meal.js`, given the rules it shares with the hook.
static EXTRACT: LazyLock<String> = LazyLock::new(|| {
    format!(
        "(input)=>({})(input,{})",
        script(include_str!("../../scripts/meal.js")),
        script(RULES)
    )
});
/// `meal_hook.js`, installed with the rules it shares with `meal.js`.
static HOOK: LazyLock<String> = LazyLock::new(|| {
    format!(
        "({})({})",
        script(include_str!("../../scripts/meal_hook.js")),
        script(RULES)
    )
});
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Candidate {
    id: String,
    transporter_id: String,
    driver: String,
    route: String,
    route_complete: bool,
    meals: Vec<Punch>,
    revision: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Punch {
    id: String,
    start: i64,
    end: Option<i64>,
}
/// How a day's meals are read. The default was measured against the alternatives on a
/// real day of 38 routes (`measure_meal_method`): moving in the application, with
/// `meal_hook.js` reading each itinerary inside the page, in three background tabs, read
/// the day in 22.5 s instead of 138.5 s, with an eighth of the CPU and half the memory
/// of loading and reading each rendered page, and the same records.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct MealMethod {
    /// Tabs reading routes at once.
    pub tabs: usize,
    /// Extra tabs each get a window of their own rather than a background tab. Only a
    /// rendered page needs one: Cortex doesn't render a hidden tab.
    pub windows: bool,
    /// Each route is read by moving the application to it, and the hook reads the
    /// itinerary it fetches, which the page never renders. Otherwise each route's page is
    /// loaded and `meal.js` reads it once rendered.
    pub hook: bool,
}
impl Default for MealMethod {
    fn default() -> Self {
        Self {
            tabs: 3,
            windows: false,
            hook: true,
        }
    }
}
impl MealMethod {
    /// How meals were read before the hook: a rendered page per route, two windows.
    #[cfg(all(test, feature = "operator-probes"))]
    pub(crate) fn rendered() -> Self {
        Self {
            tabs: 2,
            windows: true,
            hook: false,
        }
    }
}
impl Driver {
    async fn meal_read(
        &self,
        page: &Page,
        scope: &Scope,
        candidate: Option<&Candidate>,
        metrics: &Recorder,
    ) -> Result<Value> {
        // Main-world access is needed for the observed React props. Bound both
        // the CDP target and the in-page URL before inspecting application data.
        let frame = page.frame().await?;
        let url = url::Url::parse(s(&frame, "url"))
            .map_err(|_| Error::new("cortex_content_incomplete", 502))?;
        ensure(
            url.origin().ascii_serialization() == self.origin,
            "verification_required",
            409,
        )?;
        let input = json!({"kind":if candidate.is_some(){"detail"}else{"list"},"scope":scope,"candidate":candidate,"origin":self.origin});
        let result = self
            .browser
            .evaluate(&page.id, &call(&EXTRACT, &input))
            .await?;
        evidence(result, metrics)
    }
    pub(super) async fn meal_page(
        &self,
        page: &Page,
        scope: &Scope,
        candidate: Option<&Candidate>,
        metrics: &Recorder,
    ) -> Result<Value> {
        let url = self.meal_url(scope, candidate);
        page.start_navigation(&url).await?;
        let deadline = Instant::now() + Duration::from_secs(30);
        // Cortex occasionally settles on another route's details and never
        // corrects itself. One reload recovers it without hiding a real mismatch.
        let mut reload = Some(Instant::now() + Duration::from_secs(10));
        let mut last = None;
        let mut stable = 0;
        let mut last_error = "cortex_content_incomplete".to_owned();
        while Instant::now() < deadline {
            match self.meal_read(page, scope, candidate, metrics).await {
                Ok(value) => {
                    let mut evidence = value.clone();
                    if let Some(itinerary) = evidence["itinerary"].as_object_mut() {
                        itinerary.remove("observedAt");
                    }
                    if last.as_ref() == Some(&evidence) {
                        stable += 1;
                    } else {
                        stable = 0;
                    }
                    last = Some(evidence);
                    if stable >= 2 {
                        return Ok(value);
                    }
                }
                Err(error) if error.is_any(CONTENT_NOT_READY) => {
                    last = None;
                    stable = 0;
                    if error.is(crate::Code::CortexScopeMismatch)
                        && reload.is_some_and(|at| Instant::now() >= at)
                    {
                        reload = None;
                        page.start_navigation(&url).await?;
                    }
                    last_error = error.code;
                }
                Err(error) => return Err(error),
            }
            sleep(Duration::from_millis(300)).await;
        }
        Err(Error::new(&last_error, 502))
    }
    fn meal_url(&self, scope: &Scope, candidate: Option<&Candidate>) -> String {
        let path = candidate
            .map(|c| scope.detail_path(&c.id))
            .unwrap_or_else(|| scope.list_path());
        format!("{}{path}", self.origin)
    }
    /// One route's evidence as `method` reads it. `moving` is whether the tab shows
    /// Cortex's application, which can move to the route, and is kept up to date.
    async fn read_route(
        &self,
        page: &Page,
        scope: &Scope,
        candidate: &Candidate,
        method: &MealMethod,
        moving: &mut bool,
        metrics: &Recorder,
    ) -> Result<Value> {
        if !method.hook {
            return self.meal_page(page, scope, Some(candidate), metrics).await;
        }
        let url = self.meal_url(scope, Some(candidate));
        let expect = format!(
            "!!window.__dispatchMeals && window.__dispatchMeals.expect({})",
            json!({"candidate": candidate, "scope": scope, "origin": self.origin})
        );
        if *moving {
            let told = self.browser.evaluate(&page.id, &expect).await;
            let moved = told.is_ok_and(|v| v == true)
                && page
                    .evaluate(&call(super::routedata::MOVE, &json!(url)))
                    .await
                    .is_ok_and(|v| v == true);
            if moved && let Some(result) = self.hook_result(page, candidate, metrics).await? {
                return evidence(result, metrics);
            }
            // The application did not answer, or its request failed: load the page.
            metrics.detail("meal_move_reloaded");
        }
        // A loaded page is told its route once its own document has the hook: the page
        // it leaves has one too, and must not be the one told.
        let previous = page.start_navigation(&url).await?;
        let deadline = Instant::now() + Duration::from_secs(30);
        while page.navigation(&previous).await?.is_null() {
            ensure(Instant::now() < deadline, "cortex_content_incomplete", 502)?;
            sleep(Duration::from_millis(50)).await;
        }
        while !self
            .browser
            .evaluate(&page.id, &expect)
            .await
            .is_ok_and(|v| v == true)
        {
            if Instant::now() >= deadline {
                // meal.js's check: a tab sent elsewhere was sent to sign in.
                let frame = page.frame().await?;
                let cortex = url::Url::parse(s(&frame, "url"))
                    .is_ok_and(|u| u.origin().ascii_serialization() == self.origin);
                return Err(if cortex {
                    Error::new("cortex_content_incomplete", 502)
                } else {
                    Error::new("verification_required", 409)
                });
            }
            sleep(Duration::from_millis(50)).await;
        }
        *moving = true;
        match self.hook_result(page, candidate, metrics).await? {
            Some(result) => evidence(result, metrics),
            None => self.rendered(scope, candidate, metrics).await,
        }
    }
    /// What the hook read for `candidate`, or none when the application's request for it
    /// failed or no request came within 20 s.
    async fn hook_result(
        &self,
        page: &Page,
        candidate: &Candidate,
        metrics: &Recorder,
    ) -> Result<Option<Value>> {
        let deadline = Instant::now() + Duration::from_secs(20);
        let take = format!(
            "window.__dispatchMeals && window.__dispatchMeals.take({})",
            json!(candidate.id)
        );
        while Instant::now() < deadline {
            match self.browser.evaluate(&page.id, &take).await {
                Ok(value) if value["unanswered"] == true => {
                    metrics.detail("meal_hook_unanswered");
                    return Ok(None);
                }
                Ok(value) if value.is_object() => return Ok(Some(value)),
                Ok(_) => (),
                Err(error) if error.is_any(CONTENT_NOT_READY) => (),
                Err(error) => return Err(error),
            }
            sleep(Duration::from_millis(50)).await;
        }
        Ok(None)
    }
    /// A route the hook could not read, read from its rendered page as `meal.js` always
    /// has, in a window of its own without the hook: Cortex doesn't render a hidden tab.
    async fn rendered(
        &self,
        scope: &Scope,
        candidate: &Candidate,
        metrics: &Recorder,
    ) -> Result<Value> {
        metrics.detail("meal_hook_missed");
        let mut page = Page::open_window(self.browser.clone(), self.origin.clone()).await?;
        page.allow_origins(&self.origins.iter().map(String::as_str).collect::<Vec<_>>());
        let result = self.meal_page(&page, scope, Some(candidate), metrics).await;
        let _ = page.close().await;
        result
    }
    pub(super) async fn candidates(
        &self,
        scope: &Scope,
        metrics: &Recorder,
    ) -> Result<Vec<Candidate>> {
        let value = self.meal_page(&self.page, scope, None, metrics).await?;
        let rows: Vec<Candidate> = serde_json::from_value(value["candidates"].clone())
            .map_err(|_| Error::new("cortex_content_incomplete", 502))?;
        ensure(rows.len() <= 1000, "cortex_source_too_large", 502)?;
        Ok(rows)
    }
    /// Every route of the scope's day, read as `method` says until one pass finds each
    /// route's record at its latest revision.
    pub async fn collect<F, Fut>(
        &mut self,
        scope: &Scope,
        metrics: &Recorder,
        live: Option<&Writer>,
        progress: F,
        method: &MealMethod,
    ) -> Result<Value>
    where
        F: Fn(i64, String) -> Fut,
        Fut: Future<Output = Result<()>>,
    {
        scope.validate()?;
        ensure((1..=4).contains(&method.tabs), "invalid_cortex_scope", 400)?;
        let hook = json!({"source": *HOOK});
        let installed = if method.hook {
            let added = self
                .page
                .command("Page.addScriptToEvaluateOnNewDocument", hook.clone())
                .await?;
            Some(added["identifier"].clone())
        } else {
            None
        };
        let result = self
            .read_day(scope, metrics, live, &progress, method, &hook)
            .await;
        // The session's tab goes on to other collections, whose pages keep what they fetch.
        if let Some(identifier) = installed {
            let _ = self
                .page
                .command(
                    "Page.removeScriptToEvaluateOnNewDocument",
                    json!({"identifier": identifier}),
                )
                .await;
        }
        result
    }
    async fn read_day<F, Fut>(
        &self,
        scope: &Scope,
        metrics: &Recorder,
        live: Option<&Writer>,
        progress: &F,
        method: &MealMethod,
        hook: &Value,
    ) -> Result<Value>
    where
        F: Fn(i64, String) -> Fut,
        Fut: Future<Output = Result<()>>,
    {
        let started_at = now();
        let candidates = self.candidates(scope, metrics).await?;
        if let Some(live) = live {
            live.start_cortex(scope, drivers(&candidates)).await?;
        }
        // Tabs beside the first, opened once and kept for every pass.
        let mut others = Vec::new();
        for _ in 1..method.tabs.min(candidates.len()) {
            let mut page = if method.windows {
                Page::open_window(self.browser.clone(), self.origin.clone()).await?
            } else {
                Page::open(self.browser.clone(), self.origin.clone()).await?
            };
            page.allow_origins(&self.origins.iter().map(String::as_str).collect::<Vec<_>>());
            if method.hook {
                page.command("Page.addScriptToEvaluateOnNewDocument", hook.clone())
                    .await?;
            }
            others.push(page);
        }
        let result = self
            .passes(
                scope, metrics, live, progress, candidates, &others, started_at, method,
            )
            .await;
        // Close the extra windows, so they hold no memory while the capture is published.
        for page in &others {
            let _ = page.close().await;
        }
        result
    }
    /// Reads until one pass finds every listed route's record at its latest revision.
    #[allow(clippy::too_many_arguments)]
    async fn passes<F, Fut>(
        &self,
        scope: &Scope,
        metrics: &Recorder,
        live: Option<&Writer>,
        progress: &F,
        mut candidates: Vec<Candidate>,
        others: &[Page],
        started_at: i64,
        method: &MealMethod,
    ) -> Result<Value>
    where
        F: Fn(i64, String) -> Fut,
        Fut: Future<Output = Result<()>>,
    {
        let mut records: BTreeMap<String, (String, Itinerary)> = BTreeMap::new();
        let mut known = HashSet::new();
        let reads = AtomicUsize::new(0);
        // Later passes only re-read routes whose meals changed, so they are short.
        // Swipes arrive every few minutes at midday; allow for several of them.
        for pass in 0..6 {
            known.extend(candidates.iter().map(|c| c.id.clone()));
            let lanes = {
                let routes = Routes {
                    driver: self,
                    scope,
                    pending: candidates
                        .iter()
                        .filter(|c| {
                            records
                                .get(&c.id)
                                .is_none_or(|(revision, _)| revision != &c.revision)
                        })
                        .collect(),
                    next: AtomicUsize::new(0),
                    stopped: AtomicBool::new(false),
                    reads: &reads,
                    done: AtomicUsize::new(records.len()),
                    total: candidates.len(),
                    metrics,
                    live,
                    started_at,
                    progress,
                    method,
                };
                // Drain every tab even when one fails. Dropping a sibling's in-flight
                // command closes the shared browser transport.
                // The first tab shows the list's application; the others show whatever
                // their last read left, which a first read in each loads.
                futures_util::future::join_all(
                    std::iter::once((&self.page, true))
                        .chain(others.iter().map(|page| (page, pass > 0)))
                        .map(|(page, loaded)| routes.lane(page, loaded)),
                )
                .await
            };
            for lane in lanes {
                for (id, read) in lane? {
                    match read {
                        Some(record) => records.insert(id, record),
                        // A meal swipe landed after the list was read. The next pass
                        // re-reads this route at its new revision.
                        None => records.remove(&id),
                    };
                }
            }
            progress(85, format!("Checking source changes (pass {})", pass + 1)).await?;
            // A loaded list, not a move: Cortex's application keeps the list it has.
            let next = self.candidates(scope, metrics).await?;
            let ids: HashSet<_> = next.iter().map(|c| c.id.clone()).collect();
            ensure(known.is_subset(&ids), "cortex_membership_regressed", 502)?;
            if next.len() == records.len()
                && next.iter().all(|c| {
                    records
                        .get(&c.id)
                        .is_some_and(|(revision, _)| revision == &c.revision)
                })
            {
                let capture = Capture {
                    scope: scope.clone(),
                    started_at,
                    finished_at: now(),
                    itineraries: records.into_values().map(|(_, route)| route).collect(),
                };
                capture.validate(scope)?;
                progress(95, "Validating meal publication".into()).await?;
                return Ok(serde_json::to_value(capture)?);
            }
            if let Some(live) = live {
                live.cortex_drivers(drivers(&next)).await?;
            }
            candidates = next;
        }
        Err(Error::new("cortex_source_changed", 502))
    }
}

/// A page script's answer: its evidence, or its failure as the code the collection knows.
fn evidence(result: Value, metrics: &Recorder) -> Result<Value> {
    if let Some(error) = result["error"].as_str() {
        metrics.detail(s(&result, "reason"));
        let allowed = [
            crate::Code::CortexScopeMismatch,
            crate::Code::CortexContentIncomplete,
            crate::Code::CortexTimezoneMismatch,
            crate::Code::CortexSourceTooLarge,
            crate::Code::CortexInvalidMealEvidence,
            crate::Code::CortexInvalidIdentity,
            crate::Code::CortexSourceChanged,
            crate::Code::InvalidCortexScope,
        ];
        return Err(Error::new(
            if crate::Code::text_is_any(error, &allowed) {
                error
            } else {
                "cortex_content_incomplete"
            },
            502,
        ));
    }
    Ok(result)
}
/// The drivers a list names, as the live view shows them.
fn drivers(candidates: &[Candidate]) -> Value {
    json!(
        candidates
            .iter()
            .map(|c| json!({"id":c.transporter_id,"name":c.driver}))
            .collect::<Vec<_>>()
    )
}

/// One pass over the routes whose record is missing or out of date, shared by the tabs.
struct Routes<'a, F> {
    driver: &'a Driver,
    scope: &'a Scope,
    pending: Vec<&'a Candidate>,
    next: AtomicUsize,
    stopped: AtomicBool,
    reads: &'a AtomicUsize,
    done: AtomicUsize,
    total: usize,
    metrics: &'a Recorder,
    live: Option<&'a Writer>,
    started_at: i64,
    progress: &'a F,
    method: &'a MealMethod,
}
/// A route's record at the revision it was read, or `None` when it changed meanwhile.
type Read = (String, Option<(String, Itinerary)>);
impl<F, Fut> Routes<'_, F>
where
    F: Fn(i64, String) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    async fn lane(&self, page: &Page, loaded: bool) -> Result<Vec<Read>> {
        let result = self.read(page, loaded && self.method.hook).await;
        if result.is_err() {
            self.stopped.store(true, Ordering::SeqCst);
        }
        result
    }
    async fn read(&self, page: &Page, mut moving: bool) -> Result<Vec<Read>> {
        let mut reads = Vec::new();
        while !self.stopped.load(Ordering::SeqCst) {
            let Some(candidate) = self.pending.get(self.next.fetch_add(1, Ordering::SeqCst)) else {
                break;
            };
            let ordinal = self.reads.fetch_add(1, Ordering::SeqCst) + 1;
            self.metrics.page_start(ordinal, 1);
            self.metrics.page_stage(ordinal, "content");
            let done = self.done.load(Ordering::SeqCst);
            (self.progress)(
                10 + (70 * done / self.total.max(1)) as i64,
                format!("Reading meal evidence ({}/{})", done + 1, self.total),
            )
            .await?;
            let result = self
                .driver
                .read_route(
                    page,
                    self.scope,
                    candidate,
                    self.method,
                    &mut moving,
                    self.metrics,
                )
                .await;
            self.metrics
                .page_finish(ordinal, result.as_ref().err().map(|e| e.code.as_str()));
            match result {
                Ok(value) => {
                    let mut route: Itinerary =
                        serde_json::from_value(value["itinerary"].clone())
                            .map_err(|_| Error::new("cortex_content_incomplete", 502))?;
                    route.source_url = Some(format!(
                        "{}{}",
                        self.driver.origin,
                        self.scope.detail_path(&candidate.id)
                    ));
                    ensure(
                        route.id == candidate.id
                            && route.transporter_id == candidate.transporter_id,
                        "cortex_invalid_identity",
                        502,
                    )?;
                    if let Some(live) = self.live {
                        live.cortex(&Capture {
                            scope: self.scope.clone(),
                            started_at: self.started_at,
                            finished_at: now(),
                            itineraries: vec![route.clone()],
                        })
                        .await?;
                    }
                    self.done.fetch_add(1, Ordering::SeqCst);
                    reads.push((
                        candidate.id.clone(),
                        Some((candidate.revision.clone(), route)),
                    ));
                }
                Err(error) if error.is(crate::Code::CortexSourceChanged) => {
                    reads.push((candidate.id.clone(), None));
                }
                Err(error) => return Err(error),
            }
            // A rendered page keeps what it showed until collected; the hook's pages
            // render nothing.
            if !self.method.hook && reads.len().is_multiple_of(10) {
                page.collect_garbage().await?;
            }
        }
        Ok(reads)
    }
}
