use super::{
    capture::{Capture, Coverage, Itinerary},
    live::Writer,
};
use crate::{
    HOSTS, codes,
    collections::routes::{Check, Failure, Native, page::MOVE},
    connection::{Driver, gate},
    discovery::Scope,
};
use dispatch_core::{
    Error, Result,
    collection::{
        browser::{
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
    collections::{BTreeMap, HashMap, HashSet},
    future::Future,
    sync::{
        LazyLock, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::time::{Instant, sleep};

mod routes;
mod rules;

use routes::*;
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
/// `meal_ask.js`, which asks the application's own API action for one itinerary.
const ASK: &str = include_str!("../../scripts/meal_ask.js");
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
/// How a day's meals are read. The default asks the application's API action in one
/// tab without rendering, falling back to in-app moves. Compare the alternatives with
/// the `measure_meal_method` operator probe.
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
    /// With the hook, each route is asked of the application's own API action, as its
    /// pages ask for what they show, rather than moved to: the request is the
    /// application's own, and nothing renders. A tab whose application declines, having
    /// no store or an action that doesn't wait on a request, moves to its routes as
    /// before, and a route the browser couldn't ask is moved to.
    #[serde(default = "asks")]
    pub ask: bool,
    /// The day is read first with no page, with the browser's session: its list and the
    /// itineraries of the routes it needs, signed and downloaded as the routes collection's
    /// are, and read by the meal rules here. A day that can't be read so is read as the rest
    /// of the method says.
    #[serde(default = "asks")]
    pub native: bool,
}
fn asks() -> bool {
    true
}
impl Default for MealMethod {
    fn default() -> Self {
        Self {
            tabs: 1,
            in_flight: 16,
            windows: false,
            hook: true,
            ask: true,
            native: true,
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
            ask: false,
            native: false,
        }
    }
}
/// What a collection is told as it reads and where it tells of it: the job's metrics, the
/// live view of what it finds, and the job's authority, asked again after a wait with no page.
#[derive(Clone, Copy)]
pub(crate) struct Context<'a> {
    pub metrics: &'a Recorder,
    pub live: Option<&'a Writer>,
    pub check: &'a Check,
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
    scope
        .timezone
        .parse::<chrono_tz::Tz>()
        .is_ok_and(|tz| scope.date < dispatch_core::foundation::clock::today(tz).to_string())
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
/// A day's meals read from responses already kept, as live route data keeps a day's: its
/// list as `list`, and each itinerary `itinerary` gives by its id, read by the meal rules as
/// a collection's are, each route as of `at`, when the responses were read. None when the day's
/// meals can't be read from them whole: a route's itinerary is missing, a response is not the
/// route the list names, or a meal changed between the list and it. `kept` is what Timecard
/// already holds for the scope: a finished route it holds unchanged needs no itinerary. Each
/// response is asked for as it is needed and read away from the threads serving requests.
pub(crate) async fn from_responses<F, Fut>(
    scope: &Scope,
    list: String,
    kept: &[Itinerary],
    at: i64,
    itinerary: F,
) -> Result<Option<Capture>>
where
    F: Fn(String) -> Fut,
    Fut: Future<Output = Result<Option<String>>>,
{
    let of = scope.clone();
    let listed = tokio::task::spawn_blocking(move || {
        let summaries = serde_json::from_str::<Value>(&list).ok()?;
        rules::candidates(&summaries, &of).ok()
    })
    .await;
    let Ok(Some(candidates)) = listed else {
        return Ok(None);
    };
    let mut itineraries = Vec::with_capacity(candidates.len());
    for c in candidates {
        let route = if c.meals.is_empty() {
            Itinerary {
                id: c.id.clone(),
                transporter_id: c.transporter_id.clone(),
                driver: c.driver.clone(),
                route: c.route.clone(),
                observed_at: at,
                route_complete: c.route_complete,
                delivery_coverage: Coverage::Unavailable,
                meals: Vec::new(),
                source_url: None,
                unreadable: c.unreadable.clone(),
            }
        } else if let Some(route) = finished(kept, &c) {
            route.clone()
        } else {
            let Some(text) = itinerary(c.id.clone()).await? else {
                return Ok(None);
            };
            let of = scope.clone();
            let read = tokio::task::spawn_blocking(move || rules::detail(&text, &c, &of)).await;
            let Ok(Ok(route)) = read else {
                return Ok(None);
            };
            route
        };
        itineraries.push(Itinerary {
            observed_at: at,
            source_url: None,
            ..route
        });
    }
    Ok(Some(Capture {
        scope: scope.clone(),
        started_at: at,
        finished_at: at,
        itineraries,
    }))
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
            if moved && let Some(result) = self.hook_result(page, scope, candidate, metrics).await?
            {
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
        match self.hook_result(page, scope, candidate, metrics).await? {
            Some(result) => evidence(result, metrics),
            None => self.rendered(scope, candidate, metrics).await,
        }
    }
    /// What the hook read for `candidate`, or none when the application's request for it
    /// failed or no request came within 20 s. A request Cortex throttled ends the collection
    /// (`cortex_throttled`), as no other way of reading the route asks Cortex for less.
    async fn hook_result(
        &self,
        page: &Page,
        scope: &Scope,
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
                // A route's own page throttled: the account cools, and the collection's next
                // attempt reads it.
                Ok(value) if value["throttled"] == true => {
                    metrics.detail("meal_throttled");
                    gate::of(&self.origin, scope.provider.as_str()).throttled(gate::PAUSE);
                    return Err(Error::new(codes::CORTEX_THROTTLED.as_str(), 502));
                }
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
        context: &Context<'_>,
        progress: F,
        method: &MealMethod,
    ) -> Result<Value>
    where
        F: Fn(i64, String) -> Fut,
        Fut: Future<Output = Result<()>>,
    {
        let Context {
            metrics,
            live,
            check,
        } = *context;
        scope.validate()?;
        ensure((1..=4).contains(&method.tabs), "invalid_cortex_scope", 400)?;
        ensure(
            (1..=16).contains(&method.in_flight),
            "invalid_cortex_scope",
            400,
        )?;
        if method.native {
            match self
                .natively(scope, kept, metrics, live, &progress, check)
                .await?
            {
                Ok(day) => {
                    metrics.method("meals_native");
                    return Ok(day);
                }
                Err(why) => metrics.fallback(why),
            }
        }
        self.ensure_page().await?;
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
    /// A route the list alone answers: one without meals or whose punches can't be read, and
    /// a finished one Timecard holds unchanged.
    fn answered_by_list(
        &self,
        scope: &Scope,
        kept: &[Itinerary],
        c: &Candidate,
        metrics: &Recorder,
    ) -> Option<Itinerary> {
        if c.meals.is_empty() {
            metrics.detail(if c.unreadable.is_some() {
                "meal_unreadable"
            } else {
                "meal_list_only"
            });
            return Some(self.without_meals(scope, c));
        }
        let route = finished(kept, c)?;
        metrics.detail("meal_kept");
        Some(Itinerary {
            observed_at: now(),
            source_url: Some(format!("{}{}", self.origin, scope.detail_path(&c.id))),
            ..route.clone()
        })
    }
    /// The day read with no page, with the browser's session, or why it was not: what
    /// `Failure::label` calls it. Each pass reads the day's list and the itineraries of the
    /// routes whose record is missing or out of date, a few at a time at the account's gate,
    /// and the meal rules read each response here, keeping only its meal evidence. It ends
    /// with a list that shows every record at its latest revision, as the passes with a page
    /// do. Throttled past what a read waits, or its authority gone, the collection ends.
    async fn natively<F, Fut>(
        &self,
        scope: &Scope,
        kept: &[Itinerary],
        metrics: &Recorder,
        live: Option<&Writer>,
        progress: &F,
        check: &Check,
    ) -> Result<std::result::Result<Value, &'static str>>
    where
        F: Fn(i64, String) -> Fut,
        Fut: Future<Output = Result<()>>,
    {
        let told = |fault: rules::Fault| {
            metrics.detail(&fault.reason);
            Error::new(fault.code, 502)
        };
        let session = SignedIn::of(&self.browser, &self.origin, &HOSTS).await?;
        let mut native = Native::new(&session)?;
        let started_at = now();
        let mut records: BTreeMap<String, (String, Itinerary)> = BTreeMap::new();
        let mut known = HashSet::new();
        // Six passes read routes, as with a page, and a seventh only confirms the sixth's:
        // each pass's list is what shows the one before it left every record current.
        for pass in 0..=6 {
            let message = match pass {
                0 => "Reading meal evidence".to_owned(),
                _ => format!("Checking source changes (pass {pass})"),
            };
            progress(if pass == 0 { 10 } else { 85 }, message).await?;
            // The list as this pass finds it, as the rules read it, and of its routes those
            // whose itinerary is asked for.
            let listed = Mutex::new(None);
            let have: HashMap<&str, &str> = (records.iter())
                .map(|(id, (revision, _))| (id.as_str(), revision.as_str()))
                .collect();
            let wanted = |summaries: &Value| {
                let read = rules::candidates(summaries, scope);
                let ask = (read.iter().flatten())
                    .filter(|c| {
                        pass < 6
                            && have.get(c.id.as_str()) != Some(&c.revision.as_str())
                            && !c.meals.is_empty()
                            && finished(kept, c).is_none()
                    })
                    .map(|c| (c.id.clone(), c.transporter_id.clone()))
                    .collect();
                *listed.lock().expect("the day's list") = Some(read);
                Ok(ask)
            };
            let itineraries = match native.read_some(scope, check, &wanted).await {
                Ok(itineraries) => itineraries,
                Err(Failure::Throttled(error) | Failure::Stopped(error)) => return Err(error),
                Err(failure) => return Ok(Err(failure.label())),
            };
            let candidates = (listed.into_inner().expect("the day's list"))
                .ok_or_else(|| Error::new("cortex_content_incomplete", 502))?
                .map_err(told)?;
            let ids: HashSet<String> = candidates.iter().map(|c| c.id.clone()).collect();
            ensure(known.is_subset(&ids), "cortex_membership_regressed", 502)?;
            known = ids;
            let current = |records: &BTreeMap<String, (String, Itinerary)>, c: &Candidate| {
                (records.get(&c.id)).is_some_and(|(revision, _)| revision == &c.revision)
            };
            let stale = candidates.iter().any(|c| !current(&records, c));
            if let Some(live) = live {
                if pass == 0 {
                    live.start_cortex(scope, drivers(&candidates)).await?;
                } else if stale {
                    live.cortex_drivers(drivers(&candidates)).await?;
                }
            }
            let mut read = Vec::new();
            for c in candidates.iter().filter(|c| !current(&records, c)) {
                if let Some(route) = self.answered_by_list(scope, kept, c, metrics) {
                    read.push((c.clone(), Ok(route)));
                }
            }
            // Each response is text only while the rules read it, away from the threads
            // serving requests; nothing of it but its meal evidence is kept.
            let asked: Vec<_> = (itineraries.into_iter())
                .filter_map(|i| {
                    let c = candidates.iter().find(|c| c.id == i.id)?;
                    Some((c.clone(), i.detail))
                })
                .collect();
            let of = scope.clone();
            read.extend(
                tokio::task::spawn_blocking(move || {
                    (asked.into_iter())
                        .map(|(c, detail)| {
                            let evidence = match detail.text() {
                                Ok(text) => rules::detail(&text, &c, &of),
                                Err(_) => Err(rules::Fault::unread()),
                            };
                            (c, evidence)
                        })
                        .collect::<Vec<_>>()
                })
                .await
                .map_err(|_| Error::new("cortex_content_incomplete", 502))?,
            );
            for (c, evidence) in read {
                match evidence {
                    Ok(mut route) => {
                        route.source_url =
                            Some(format!("{}{}", self.origin, scope.detail_path(&c.id)));
                        if let Some(live) = live {
                            live.cortex(&Capture {
                                scope: scope.clone(),
                                started_at,
                                finished_at: now(),
                                itineraries: vec![route.clone()],
                            })
                            .await?;
                        }
                        records.insert(c.id, (c.revision, route));
                    }
                    // A meal swipe landed after the list was read. The next pass reads this
                    // route at its new revision.
                    Err(fault) if fault.code == codes::CORTEX_SOURCE_CHANGED.as_str() => {
                        records.remove(&c.id);
                    }
                    Err(fault) => return Err(told(fault)),
                }
            }
            // A finished day changes no more; any other is done once a list read after its
            // records shows them all as they are.
            let whole = candidates.len() == records.len()
                && candidates.iter().all(|c| current(&records, c));
            let settled = day_over(scope) && candidates.iter().all(|c| c.route_complete);
            if whole && (settled || !stale) {
                let capture = Capture {
                    scope: scope.clone(),
                    started_at,
                    finished_at: now(),
                    itineraries: records.into_values().map(|(_, route)| route).collect(),
                };
                capture.validate(scope)?;
                progress(95, "Validating meal publication".into()).await?;
                return Ok(Ok(serde_json::to_value(capture)?));
            }
        }
        Err(Error::new("cortex_source_changed", 502))
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
        let throttled = AtomicBool::new(false);
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
                let Some(route) = self.answered_by_list(scope, kept, c, metrics) else {
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
                    throttled: &throttled,
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
            // A day read whole that met no throttle counts toward widening the account's
            // gate again, as a native read of a day does.
            if method.hook && !throttled.load(Ordering::SeqCst) {
                gate::of(&self.origin, &scope.provider).read_clean();
            }
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
