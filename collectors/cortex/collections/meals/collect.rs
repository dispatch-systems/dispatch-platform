use super::{
    capture::{Capture, Coverage, Itinerary},
    live::Writer,
};
use crate::{codes, collections::routes::collect::MOVE, connection::Driver, discovery::Scope};
use dispatch_core::{
    Error, Result,
    collection::{
        browser::page::{Page, call},
        metrics::Recorder,
    },
    db::{now, s},
    ensure,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashSet},
    future::Future,
    sync::{
        LazyLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::time::{Instant, sleep};
// The route's content has not settled yet; read it again.
pub(crate) const CONTENT_NOT_READY: &[dispatch_core::Code] = &[
    codes::CORTEX_CONTENT_INCOMPLETE,
    dispatch_core::Code::BrowserNavigationPending,
    dispatch_core::Code::BrowserScriptFailed,
    codes::CORTEX_SCOPE_MISMATCH,
    dispatch_core::Code::VerificationRequired,
];
/// How long the application may take to answer a move before the route's page is loaded.
const MOVE_DEADLINE: Duration = Duration::from_secs(20);
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
pub(crate) struct Candidate {
    id: String,
    transporter_id: String,
    driver: String,
    route: String,
    route_complete: bool,
    meals: Vec<Punch>,
    /// The rule the route's punches broke, when they can't be read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    unreadable: Option<String>,
    revision: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Punch {
    id: String,
    start: i64,
    end: Option<i64>,
}
/// How a day's meals are read. The default was measured against the alternatives on real
/// days of 36 to 51 routes (`measure_meal_method`): one tab of the hook waiting for twelve
/// routes at once, its list read from the response the application fetched, read a
/// finished 51-route day in 8.8 s instead of 26 to 42 s for three tabs waiting for one
/// route each, with half the CPU and memory and the same records. More routes at once, or
/// more tabs, read no faster.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct MealMethod {
    /// Tabs reading routes at once.
    pub tabs: usize,
    /// Routes each tab waits for at once. Cortex's application fetches each route it is
    /// moved to, even while an earlier one is still loading, and the hook reads every
    /// response for its own route, so the tab waits on Cortex for all of them together.
    /// Without the hook a tab reads one route at a time.
    pub in_flight: usize,
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
            tabs: 1,
            in_flight: 12,
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
            in_flight: 1,
            windows: true,
            hook: false,
        }
    }
}
/// What a read of the page asks `meal.js` for.
#[derive(Clone, Copy)]
pub(crate) enum Ask<'a> {
    /// The list's routes, once three reads agree.
    List,
    /// Only that the page shows the scope's station, day and provider, read once.
    Scope,
    /// A route's evidence, once three reads agree.
    Detail(&'a Candidate),
}
impl Ask<'_> {
    fn kind(&self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Scope => "scope",
            Self::Detail(_) => "detail",
        }
    }
    fn candidate(&self) -> Option<&Candidate> {
        match self {
            Self::Detail(candidate) => Some(candidate),
            _ => None,
        }
    }
}
/// Whether the scope's day has ended where its station is.
fn day_over(scope: &Scope) -> bool {
    scope.timezone.parse::<chrono_tz::Tz>().is_ok_and(|tz| {
        scope.date
            < chrono::Utc::now()
                .with_timezone(&tz)
                .date_naive()
                .to_string()
    })
}
/// What `kept` holds of `c`'s route when the route was finished then and the list still
/// shows it finished, with the same driver and meals: a record nothing can change any more.
fn finished<'a>(kept: &'a [Itinerary], c: &Candidate) -> Option<&'a Itinerary> {
    let route = kept.iter().find(|k| k.id == c.id)?;
    let mut was: Vec<_> = route
        .meals
        .iter()
        .map(|m| (m.id.as_str(), m.start, m.end))
        .collect();
    let mut is: Vec<_> = c
        .meals
        .iter()
        .map(|m| (m.id.as_str(), m.start, m.end))
        .collect();
    was.sort();
    is.sort();
    (route.route_complete
        && c.route_complete
        && route.delivery_coverage == Coverage::Complete
        && route.transporter_id == c.transporter_id
        && route.driver == c.driver
        && route.route == c.route
        && was == is)
        .then_some(route)
}
impl Driver {
    async fn meal_read(
        &self,
        page: &Page,
        scope: &Scope,
        ask: Ask<'_>,
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
        let input = json!({"kind":ask.kind(),"scope":scope,"candidate":ask.candidate(),"origin":self.origin});
        let result = self
            .browser
            .evaluate(&page.id, &call(&EXTRACT, &input))
            .await?;
        evidence(result, metrics)
    }
    /// Loads the page `ask` reads and reads it once its document is the new one: a list or
    /// a route once three reads running agree, the scope at the first read that shows it.
    pub(crate) async fn meal_page(
        &self,
        page: &Page,
        scope: &Scope,
        ask: Ask<'_>,
        metrics: &Recorder,
    ) -> Result<Value> {
        let url = self.meal_url(scope, ask.candidate());
        let mut previous = page.start_navigation(&url).await?;
        let mut loaded = false;
        let agree = if matches!(ask, Ask::Scope) { 0 } else { 2 };
        let deadline = Instant::now() + Duration::from_secs(30);
        // Cortex occasionally settles on another route's details and never
        // corrects itself. One reload recovers it without hiding a real mismatch.
        let mut reload = Some(Instant::now() + Duration::from_secs(10));
        let mut last = None;
        let mut stable = 0;
        let mut last_error = "cortex_content_incomplete".to_owned();
        while Instant::now() < deadline {
            // The page it leaves would answer for the one it loads.
            loaded = loaded || !page.navigation(&previous).await?.is_null();
            if !loaded {
                sleep(Duration::from_millis(50)).await;
                continue;
            }
            match self.meal_read(page, scope, ask, metrics).await {
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
                    if stable >= agree {
                        return Ok(value);
                    }
                }
                Err(error) if error.is_any(CONTENT_NOT_READY) => {
                    last = None;
                    stable = 0;
                    if error.is(codes::CORTEX_SCOPE_MISMATCH)
                        && reload.is_some_and(|at| Instant::now() >= at)
                    {
                        reload = None;
                        previous = page.start_navigation(&url).await?;
                        loaded = false;
                    }
                    last_error = error.code;
                }
                Err(error) => return Err(error),
            }
            sleep(Duration::from_millis(if agree > 0 { 300 } else { 50 })).await;
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
            return self
                .meal_page(page, scope, Ask::Detail(candidate), metrics)
                .await;
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
                    .evaluate(&call(MOVE, &json!(url)))
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
        let deadline = Instant::now() + MOVE_DEADLINE;
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
        let result = self
            .meal_page(&page, scope, Ask::Detail(candidate), metrics)
            .await;
        let _ = page.close().await;
        result
    }
    /// The day's routes as the list shows them. With the hook they are read from the list
    /// the application fetched, once the page shows the scope; without it, from the
    /// page's props once three reads agree.
    pub(crate) async fn candidates(
        &self,
        scope: &Scope,
        method: &MealMethod,
        metrics: &Recorder,
    ) -> Result<Vec<Candidate>> {
        let value = if method.hook {
            self.meal_page(&self.page, scope, Ask::Scope, metrics)
                .await?;
            self.hooked_list(scope, metrics).await?
        } else {
            self.meal_page(&self.page, scope, Ask::List, metrics)
                .await?
        };
        let rows: Vec<Candidate> = serde_json::from_value(value["candidates"].clone())
            .map_err(|_| Error::new("cortex_content_incomplete", 502))?;
        ensure(rows.len() <= 1000, "cortex_source_too_large", 502)?;
        Ok(rows)
    }
    /// The routes of the list the hook kept, once the application has it: a page that
    /// shows the scope has already received it.
    async fn hooked_list(&self, scope: &Scope, metrics: &Recorder) -> Result<Value> {
        let list = format!(
            "window.__dispatchMeals ? window.__dispatchMeals.list({}) : null",
            json!({"scope": scope, "origin": self.origin})
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match self.browser.evaluate(&self.page.id, &list).await {
                Ok(value) if value.is_object() => return evidence(value, metrics),
                Ok(_) => (),
                Err(error) if error.is_any(CONTENT_NOT_READY) => (),
                Err(error) => return Err(error),
            }
            ensure(Instant::now() < deadline, "cortex_content_incomplete", 502)?;
            sleep(Duration::from_millis(50)).await;
        }
    }
    /// Every route of the scope's day, read as `method` says until one pass finds each
    /// route's record at its latest revision. `kept` is what Timecard already holds for
    /// the scope: a finished route it holds unchanged is not read again.
    pub async fn collect<F, Fut>(
        &mut self,
        scope: &Scope,
        kept: &[Itinerary],
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
        ensure(
            (1..=16).contains(&method.in_flight),
            "invalid_cortex_scope",
            400,
        )?;
        let installed = if method.hook {
            let added = self
                .page
                .command(
                    "Page.addScriptToEvaluateOnNewDocument",
                    json!({"source": *HOOK}),
                )
                .await?;
            Some(added["identifier"].clone())
        } else {
            None
        };
        let result = self
            .read_day(scope, kept, metrics, live, &progress, method)
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
        kept: &[Itinerary],
        metrics: &Recorder,
        live: Option<&Writer>,
        progress: &F,
        method: &MealMethod,
    ) -> Result<Value>
    where
        F: Fn(i64, String) -> Fut,
        Fut: Future<Output = Result<()>>,
    {
        let started_at = now();
        let candidates = self.candidates(scope, method, metrics).await?;
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
                page.command(
                    "Page.addScriptToEvaluateOnNewDocument",
                    json!({"source": *HOOK}),
                )
                .await?;
            }
            others.push(page);
        }
        let result = self
            .passes(
                scope, kept, metrics, live, progress, candidates, &others, started_at, method,
            )
            .await;
        // Close the extra windows, so they hold no memory while the capture is published.
        for page in &others {
            let _ = page.close().await;
        }
        result
    }
    /// A route the list shows without meals, or whose punches it can't read: there are no
    /// deliveries to bound, so its itinerary is not read and its deliveries are not counted.
    fn without_meals(&self, scope: &Scope, c: &Candidate) -> Itinerary {
        Itinerary {
            id: c.id.clone(),
            transporter_id: c.transporter_id.clone(),
            driver: c.driver.clone(),
            route: c.route.clone(),
            observed_at: now(),
            route_complete: c.route_complete,
            delivery_coverage: Coverage::Unavailable,
            meals: Vec::new(),
            source_url: Some(format!("{}{}", self.origin, scope.detail_path(&c.id))),
            unreadable: c.unreadable.clone(),
        }
    }
    /// Reads until one pass finds every listed route's record at its latest revision.
    #[allow(clippy::too_many_arguments)]
    async fn passes<F, Fut>(
        &self,
        scope: &Scope,
        kept: &[Itinerary],
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
            // Routes the list alone answers: one without meals or whose punches can't be
            // read, and a finished one Timecard holds unchanged.
            for c in &candidates {
                if records
                    .get(&c.id)
                    .is_some_and(|(revision, _)| revision == &c.revision)
                {
                    continue;
                }
                let route = if c.meals.is_empty() {
                    metrics.detail(if c.unreadable.is_some() {
                        "meal_unreadable"
                    } else {
                        "meal_list_only"
                    });
                    self.without_meals(scope, c)
                } else if let Some(route) = finished(kept, c) {
                    metrics.detail("meal_kept");
                    Itinerary {
                        observed_at: now(),
                        source_url: Some(format!("{}{}", self.origin, scope.detail_path(&c.id))),
                        ..route.clone()
                    }
                } else {
                    continue;
                };
                if let Some(live) = live {
                    live.cortex(&Capture {
                        scope: scope.clone(),
                        started_at,
                        finished_at: now(),
                        itineraries: vec![route.clone()],
                    })
                    .await?;
                }
                records.insert(c.id.clone(), (c.revision.clone(), route));
            }
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
            let current = |candidates: &[Candidate]| {
                candidates.len() == records.len()
                    && candidates.iter().all(|c| {
                        records
                            .get(&c.id)
                            .is_some_and(|(revision, _)| revision == &c.revision)
                    })
            };
            // A finished day changes no more: when every route was complete and read
            // as listed, its list is not read again to confirm it.
            let settled = day_over(scope)
                && candidates.iter().all(|c| c.route_complete)
                && current(&candidates);
            if !settled {
                progress(85, format!("Checking source changes (pass {})", pass + 1)).await?;
                // A loaded list, not a move: Cortex's application keeps the list it has.
                let next = self.candidates(scope, method, metrics).await?;
                let ids: HashSet<_> = next.iter().map(|c| c.id.clone()).collect();
                ensure(known.is_subset(&ids), "cortex_membership_regressed", 502)?;
                if !current(&next) {
                    if let Some(live) = live {
                        live.cortex_drivers(drivers(&next)).await?;
                    }
                    candidates = next;
                    continue;
                }
            }
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
        Err(Error::new("cortex_source_changed", 502))
    }
}

/// A page script's answer: its evidence, or its failure as the code the collection knows.
fn evidence(result: Value, metrics: &Recorder) -> Result<Value> {
    if let Some(error) = result["error"].as_str() {
        metrics.detail(s(&result, "reason"));
        let allowed = [
            codes::CORTEX_SCOPE_MISMATCH,
            codes::CORTEX_CONTENT_INCOMPLETE,
            codes::CORTEX_TIMEZONE_MISMATCH,
            codes::CORTEX_SOURCE_TOO_LARGE,
            codes::CORTEX_INVALID_MEAL_EVIDENCE,
            codes::CORTEX_INVALID_IDENTITY,
            codes::CORTEX_SOURCE_CHANGED,
            codes::INVALID_CORTEX_SCOPE,
        ];
        return Err(Error::new(
            if dispatch_core::Code::text_is_any(error, &allowed) {
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
impl<'a, F, Fut> Routes<'a, F>
where
    F: Fn(i64, String) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    async fn lane(&self, page: &Page, loaded: bool) -> Result<Vec<Read>> {
        let result = if self.method.hook {
            self.moves(page, loaded).await
        } else {
            self.read(page).await
        };
        if result.is_err() {
            self.stopped.store(true, Ordering::SeqCst);
        }
        result
    }
    /// The next route still to read, its read begun in the metrics, with its ordinal.
    async fn begin(&self) -> Result<Option<(&'a Candidate, usize)>> {
        if self.stopped.load(Ordering::SeqCst) {
            return Ok(None);
        }
        let Some(&candidate) = self.pending.get(self.next.fetch_add(1, Ordering::SeqCst)) else {
            return Ok(None);
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
        Ok(Some((candidate, ordinal)))
    }
    /// A route's read, kept as the pass keeps it, or the error that ends the collection.
    async fn end(
        &self,
        candidate: &Candidate,
        ordinal: usize,
        result: Result<Value>,
    ) -> Result<Read> {
        self.metrics
            .page_finish(ordinal, result.as_ref().err().map(|e| e.code.as_str()));
        match result {
            Ok(value) => {
                let mut route: Itinerary = serde_json::from_value(value["itinerary"].clone())
                    .map_err(|_| Error::new("cortex_content_incomplete", 502))?;
                route.source_url = Some(format!(
                    "{}{}",
                    self.driver.origin,
                    self.scope.detail_path(&candidate.id)
                ));
                ensure(
                    route.id == candidate.id && route.transporter_id == candidate.transporter_id,
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
                Ok((
                    candidate.id.clone(),
                    Some((candidate.revision.clone(), route)),
                ))
            }
            // A meal swipe landed after the list was read. The next pass re-reads this
            // route at its new revision.
            Err(error) if error.is(codes::CORTEX_SOURCE_CHANGED) => {
                Ok((candidate.id.clone(), None))
            }
            Err(error) => Err(error),
        }
    }
    /// Reads routes one at a time from their rendered pages.
    async fn read(&self, page: &Page) -> Result<Vec<Read>> {
        let mut reads = Vec::new();
        let mut moving = false;
        while let Some((candidate, ordinal)) = self.begin().await? {
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
            reads.push(self.end(candidate, ordinal, result).await?);
            // A rendered page keeps what it showed until collected.
            if reads.len().is_multiple_of(10) {
                page.collect_garbage().await?;
            }
        }
        Ok(reads)
    }
    /// Reads routes through the hook, `in_flight` at once: each is named to the hook and
    /// the application moved to it without waiting for the one before, and what the hook
    /// has read is taken every 50 ms. A route the application didn't answer, or not within
    /// 20 s, is read afterwards from its own loaded page. `loaded` is whether the tab
    /// shows the application already; one that doesn't loads its first route's page.
    async fn moves(&self, page: &Page, loaded: bool) -> Result<Vec<Read>> {
        let driver = self.driver;
        let mut reads = Vec::new();
        let mut moving = loaded;
        let mut waiting: Vec<(&'a Candidate, usize, Instant)> = Vec::new();
        let mut unanswered: Vec<(&'a Candidate, usize)> = Vec::new();
        loop {
            if !moving {
                let Some((candidate, ordinal)) = self.begin().await? else {
                    break;
                };
                let result = driver
                    .read_route(
                        page,
                        self.scope,
                        candidate,
                        self.method,
                        &mut moving,
                        self.metrics,
                    )
                    .await;
                reads.push(self.end(candidate, ordinal, result).await?);
                continue;
            }
            let mut drained = false;
            while waiting.len() < self.method.in_flight {
                let Some((candidate, ordinal)) = self.begin().await? else {
                    drained = true;
                    break;
                };
                let input =
                    json!({"candidate": candidate, "scope": self.scope, "origin": driver.origin});
                let named = driver
                    .browser
                    .evaluate(
                        &page.id,
                        &format!(
                            "!!window.__dispatchMeals && window.__dispatchMeals.want({input})"
                        ),
                    )
                    .await;
                let url = driver.meal_url(self.scope, Some(candidate));
                let moved = named.is_ok_and(|v| v == true)
                    && page
                        .evaluate(&call(MOVE, &json!(url)))
                        .await
                        .is_ok_and(|v| v == true);
                if moved {
                    waiting.push((candidate, ordinal, Instant::now()));
                } else {
                    unanswered.push((candidate, ordinal));
                }
            }
            if waiting.is_empty() {
                if drained {
                    break;
                }
                continue;
            }
            sleep(Duration::from_millis(50)).await;
            match driver
                .browser
                .evaluate(
                    &page.id,
                    "window.__dispatchMeals ? window.__dispatchMeals.taken() : null",
                )
                .await
            {
                Ok(Value::Object(taken)) => {
                    for (id, value) in taken {
                        let Some(index) = waiting.iter().position(|(c, ..)| c.id == id) else {
                            continue;
                        };
                        let (candidate, ordinal, _) = waiting.swap_remove(index);
                        if value["unanswered"] == true {
                            self.metrics.detail("meal_hook_unanswered");
                            unanswered.push((candidate, ordinal));
                        } else {
                            let result = evidence(value, self.metrics);
                            reads.push(self.end(candidate, ordinal, result).await?);
                        }
                    }
                }
                Ok(_) => (),
                Err(error) if error.is_any(CONTENT_NOT_READY) => (),
                Err(error) => return Err(error),
            }
            let mut index = 0;
            while index < waiting.len() {
                if waiting[index].2.elapsed() >= MOVE_DEADLINE {
                    let (candidate, ordinal, _) = waiting.swap_remove(index);
                    unanswered.push((candidate, ordinal));
                } else {
                    index += 1;
                }
            }
        }
        // The application didn't answer these: each is read from its own page.
        for (candidate, ordinal) in unanswered {
            if self.stopped.load(Ordering::SeqCst) {
                break;
            }
            self.metrics.detail("meal_move_reloaded");
            let mut reload = false;
            let result = driver
                .read_route(
                    page,
                    self.scope,
                    candidate,
                    self.method,
                    &mut reload,
                    self.metrics,
                )
                .await;
            reads.push(self.end(candidate, ordinal, result).await?);
        }
        Ok(reads)
    }
}
