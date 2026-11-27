//! Cortex authentication and performance transport from Amazon Logistics.
pub(crate) mod companies;
pub(crate) mod gate;
pub mod performance;
use self::companies::COMPANIES;
use crate::collections::meals::collect as collection;
use crate::discovery::{CollectionRequest, Scope, known::KnownScopes};
use dispatch_core::collection::browser::{
    Collected, Driver as Drives, Pending, Run,
    attempt::Attempts,
    browseros,
    http::{Http, Refusal, SignedIn},
    page::{Page, call},
};
use dispatch_core::{
    Code, Error, Result, collection::metrics::Recorder, db::s, ensure, manifest::registry,
};
use serde_json::{Value, json};
use std::{path::Path, sync::Arc, time::Duration};
use tokio::time::{Instant, sleep};
// The page is between documents or still loading its scripts; ask it again.
const PAGE_NOT_READY: &[dispatch_core::Code] = &[
    dispatch_core::Code::BrowserNavigationPending,
    dispatch_core::Code::BrowserScriptFailed,
    dispatch_core::Code::ManualVerificationRequired,
];
const ORIGIN: &str = "https://logistics.amazon.com";
const ORIGINS: &[&str] = &[ORIGIN, "https://www.amazon.com", "https://amazon.com"];
const LANDING: &str = "/dspconsolev2";
const AUTH: &str = include_str!("../scripts/auth.js");
/// As much as the company list a session check reads may hold.
const COMPANIES_LIMIT: usize = 2 * 1024 * 1024;
/// How long one request of the session check may take: a stalled answer signs in on the
/// page instead, well within the start's own time.
const CHECK_TIMEOUT: Duration = Duration::from_secs(10);

/// Whether `html` is the console's own page, by its title: a sign-in, a verification or a
/// robot check served in its place has another.
pub(crate) fn console(html: &str) -> bool {
    let lower = html.to_ascii_lowercase();
    lower
        .split("<title>")
        .nth(1)
        .and_then(|rest| rest.split("</title>").next())
        .is_some_and(|title| title.trim() == "dsp console")
}
/// One request of the saved session's check, asked once more when Amazon was briefly
/// unavailable. A refusal, or no answer in time, is none.
async fn checked<T, F, Fut>(ask: F) -> Option<T>
where
    F: Fn() -> Fut,
    Fut: Future<Output = std::result::Result<T, Refusal>>,
{
    for attempt in 0..2 {
        match tokio::time::timeout(CHECK_TIMEOUT, ask()).await {
            Ok(Ok(answer)) => return Some(answer),
            Ok(Err(Refusal::Unavailable)) if attempt == 0 => {
                sleep(Duration::from_millis(500)).await;
            }
            _ => return None,
        }
    }
    None
}
/// Whether the session `http` carries still signs the account in: it carries Amazon's
/// cookies, Amazon serves the console page itself rather than sending it to a sign-in or a
/// verification, and the company list, which Amazon answers only a signed-in account, comes
/// back with the account's companies. Answers the company list when the session stands.
async fn session_stands(http: &Http, origin: &str) -> Option<Value> {
    if !http.carries_cookies() {
        return None;
    }
    let landing = format!("{origin}{LANDING}");
    let home = format!("{origin}/");
    let companies = format!("{origin}{COMPANIES}");
    let workforce = format!("{origin}/workforce");
    // Both at once; the first refused ends the check without waiting on the other.
    let console = async {
        checked(|| http.page(&landing, &home))
            .await
            .filter(|html| console(html))
            .ok_or(())
    };
    let companies = async {
        checked(|| http.json(&companies, &workforce, COMPANIES_LIMIT))
            .await
            .filter(|reply| {
                reply["records"]
                    .as_array()
                    .is_some_and(|records| !records.is_empty())
            })
            .ok_or(())
    };
    futures_util::future::try_join(console, companies)
        .await
        .ok()
        .map(|(_, companies)| companies)
}
/// What a driver with no browser fails where a collection needs a page: the job's browser
/// starts then, and the collection runs again in it.
fn browser_required() -> Error {
    Error::new(Code::BrowserRequired, 409)
}
/// Where the application is, and the origins its tab may open: Amazon's, or only the
/// stand-in a test points the browser at.
pub(crate) fn application(fixture: Option<&str>) -> (String, Vec<String>) {
    let origin = fixture.unwrap_or(ORIGIN).trim_end_matches('/').to_owned();
    let origins = if fixture.is_some() {
        vec![origin.clone()]
    } else {
        ORIGINS.iter().map(|v| (*v).to_owned()).collect()
    };
    (origin, origins)
}
/// The application in a browser, as a collection reads it: the browser, the tab it reads
/// in and the origins that tab may open. A job's driver lends its own (`Driver::tab`); a
/// browser a feature opened outside a job holds one of its own (`routes::Browser`).
pub(crate) struct Tab<'a> {
    pub browser: &'a browseros::Session,
    pub page: &'a mut Page,
    pub origin: &'a str,
    pub origins: &'a [String],
}
impl Tab<'_> {
    /// The tab, opened the first time a collection needs one: a session that started from
    /// the saved one has none until then.
    pub(crate) async fn ensure_page(&mut self) -> Result<()> {
        if self.page.id.is_empty() {
            *self.page = Page::open(self.browser.clone(), self.origin.to_owned()).await?;
            self.page
                .allow_origins(&self.origins.iter().map(String::as_str).collect::<Vec<_>>());
            self.page.size_window().await?;
            self.page.front_alone().await?;
        }
        Ok(())
    }
}
/// A job's browser, and the tab its collections read in.
struct Application {
    browser: browseros::Session,
    page: Page,
}
pub struct Driver {
    /// None for a driver reading with the session a sign-in saved (`resumed`), which fails
    /// `browser_required` where a collection needs a page.
    application: Option<Application>,
    /// The session such a driver reads with: one client for the job, which keeps what its
    /// replies set for the next job.
    saved: Option<Arc<Http>>,
    pub(crate) origin: String,
    pub(crate) origins: Vec<String>,
    attempts: Attempts,
    /// What discovery found for the DSP's stations, for the next collections.
    pub(crate) known: KnownScopes,
    /// The company list the start's session check read, which a collection needing it reads
    /// no second time: none after a sign-in on the page.
    pub(crate) companies: Option<Value>,
    credentials: Value,
    username_submitted: bool,
    password_submitted: bool,
}
impl Driver {
    pub async fn new(
        browser: browseros::Session,
        profile: &Path,
        fixture: Option<&str>,
    ) -> Result<Self> {
        let (origin, origins) = application(fixture);
        Ok(Self {
            application: Some(Application {
                page: Page::empty(browser.clone(), origin.clone()),
                browser,
            }),
            saved: None,
            ..Self::without_browser(profile, origin, origins)?
        })
    }
    /// A driver reading with `session`, the one a sign-in saved, and no browser.
    pub fn resumed(session: &SignedIn, profile: &Path, fixture: Option<&str>) -> Result<Self> {
        let (origin, origins) = application(fixture);
        Ok(Self {
            saved: Some(Arc::new(Http::of_session(session, &crate::HOSTS)?)),
            ..Self::without_browser(profile, origin, origins)?
        })
    }
    fn without_browser(profile: &Path, origin: String, origins: Vec<String>) -> Result<Self> {
        Ok(Self {
            application: None,
            saved: None,
            origin,
            origins,
            attempts: Attempts::beside(profile, "cortex")?,
            known: KnownScopes::beside(profile)?,
            companies: None,
            credentials: Value::Null,
            username_submitted: false,
            password_submitted: false,
        })
    }
    /// Its browser: none while it reads with the session a sign-in saved.
    pub(crate) fn browser(&self) -> Result<&browseros::Session> {
        self.application
            .as_ref()
            .map(|application| &application.browser)
            .ok_or_else(browser_required)
    }
    /// The tab its collections read in, which `ensure_page` opens.
    pub(crate) fn page(&self) -> Result<&Page> {
        self.application
            .as_ref()
            .map(|application| &application.page)
            .ok_or_else(browser_required)
    }
    /// Its application, for a collection to read in.
    pub(crate) fn tab(&mut self) -> Result<Tab<'_>> {
        let application = self.application.as_mut().ok_or_else(browser_required)?;
        Ok(Tab {
            browser: &application.browser,
            page: &mut application.page,
            origin: &self.origin,
            origins: &self.origins,
        })
    }
    pub(crate) async fn ensure_page(&mut self) -> Result<()> {
        self.tab()?.ensure_page().await
    }
    pub async fn resolve_scope(
        &mut self,
        request: &CollectionRequest,
        metrics: &Recorder,
    ) -> Result<Scope> {
        self.tab()?.resolve_scope(request, metrics).await
    }
    /// The account's session over plain HTTP: the browser's as it is now, or the one a
    /// sign-in saved.
    pub(crate) async fn http(&self) -> Result<Arc<Http>> {
        match (&self.saved, &self.application) {
            (Some(http), _) => Ok(http.clone()),
            (None, Some(application)) => Ok(Arc::new(
                Http::signed_in(&application.browser, &self.origin, &crate::HOSTS).await?,
            )),
            (None, None) => Err(browser_required()),
        }
    }
    /// Closes its browser once a collection needs no page: one reading with the saved session
    /// has none.
    pub(crate) async fn close_browser(&self) {
        if let Some(application) = &self.application {
            application.browser.close().await;
        }
    }
    /// Asks its browser to close without waiting for it to exit, so that what reads next is
    /// not held up: one reading with the saved session has none.
    pub(crate) fn stop_browser(&self) {
        if let Some(application) = &self.application {
            application.browser.stop();
        }
    }
    async fn script(&self, mut input: Value) -> Result<Value> {
        input["origins"] = json!(self.origins);
        input["applicationOrigin"] = json!(self.origin);
        self.page()?.evaluate(&call(AUTH, &input)).await
    }
    async fn observe(&self, previous: &str, seconds: u64) -> Result<Value> {
        let deadline = Instant::now() + Duration::from_secs(seconds);
        while Instant::now() < deadline {
            match self.script(json!({"action":"observe"})).await {
                Ok(value) if s(&value, "state") != "pending" && s(&value, "state") != previous => {
                    return Ok(value);
                }
                Ok(_) => (),
                Err(error) if error.is_any(PAGE_NOT_READY) => {
                    let page = self.page()?;
                    let frame = page.frame().await?;
                    ensure(
                        s(&frame, "url") == "about:blank" || page.trusted(s(&frame, "url")),
                        "navigation_policy_violation",
                        409,
                    )?;
                }
                Err(error) => return Err(error),
            }
            sleep(Duration::from_millis(200)).await;
        }
        Ok(json!({"state":"challenge"}))
    }
    async fn advance(&mut self, mut value: Value) -> Result<Value> {
        for _ in 0..4 {
            let state = s(&value, "state");
            match state {
                "authenticated" => {
                    self.attempts.succeeded()?;
                    self.credentials = Value::Null;
                    self.companies = None;
                    return Ok(json!({"type":"ready"}));
                }
                "invalid_credentials" | "account_locked" | "provider_unavailable" => {
                    return Err(Error::new(state, 409));
                }
                "username" | "password"
                    if !self.credentials.is_null()
                        && (state == "username" && !self.username_submitted
                            || state == "password" && !self.password_submitted) =>
                {
                    let previous = state.to_owned();
                    if state == "username" {
                        self.username_submitted = true;
                    } else {
                        self.password_submitted = true;
                    }
                    self.attempts.submitted()?;
                    let submitted = self
                        .script(json!({"action":"login","credentials":self.credentials}))
                        .await?;
                    if s(&submitted, "state") != "submitted" {
                        break;
                    }
                    value = self.observe(&previous, 30).await?;
                }
                _ => break,
            }
        }
        self.attempts.failed("manual_verification_required")?;
        Ok(json!({"type":"challenge"}))
    }
    /// The start of a driver reading with the session a sign-in saved: ready when the session
    /// still stands, as a browser's own is checked, and `browser_required` otherwise.
    async fn resume(&mut self, command: &Value) -> Result<Value> {
        ensure(s(command, "action") == "start", "verification_expired", 409)?;
        let http = self.http().await?;
        let companies = session_stands(&http, &self.origin)
            .await
            .ok_or_else(browser_required)?;
        self.attempts.succeeded()?;
        self.companies = Some(companies);
        Ok(json!({"type":"ready"}))
    }
    pub async fn request(&mut self, command: Value) -> Result<Value> {
        if self.application.is_none() {
            return self.resume(&command).await;
        }
        let result = async {
            // A session that started from the saved one has no tab to verify on.
            if ["complete_assistance", "verify", "screenshot", "assist"]
                .contains(&s(&command, "action"))
            {
                ensure(!self.page()?.id.is_empty(), "verification_expired", 409)?;
            }
            match s(&command, "action") {
                "start" | "check" => {
                    let observe_only = self
                        .attempts
                        .check(s(&command, "action") == "check" || command["ownerRetry"] == true)?;
                    self.credentials = if observe_only {
                        Value::Null
                    } else {
                        command["credentials"].clone()
                    };
                    self.username_submitted = false;
                    self.password_submitted = false;
                    // A job's new browser starts from the session its profile kept, when
                    // Amazon still accepts it, and opens no page until it needs one. The
                    // owner's own sign-ins always go through the page.
                    if s(&command, "action") == "start"
                        && command["ownerRetry"] != true
                        && self.page()?.id.is_empty()
                        && let Some(companies) = session_stands(
                            &Http::signed_in(self.browser()?, &self.origin, &crate::HOSTS).await?,
                            &self.origin,
                        )
                        .await
                    {
                        self.attempts.succeeded()?;
                        self.credentials = Value::Null;
                        self.companies = Some(companies);
                        return Ok(json!({"type":"ready"}));
                    }
                    self.ensure_page().await?;
                    self.page()?.navigate(LANDING).await?;
                    let value = self.observe("", 30).await?;
                    self.advance(value).await
                }
                "complete_assistance" => {
                    let value = self.observe("", 2).await?;
                    self.advance(value).await
                }
                "verify" => {
                    let code = s(&command, "code");
                    ensure(
                        (4..=8).contains(&code.len()) && code.bytes().all(|v| v.is_ascii_digit()),
                        "invalid_verification_code",
                        409,
                    )?;
                    let value = self.script(json!({"action":"verify","code":code})).await?;
                    ensure(
                        s(&value, "state") == "submitted",
                        "invalid_verification_code",
                        409,
                    )?;
                    let value = self.observe("challenge", 15).await?;
                    self.advance(value).await
                }
                "screenshot" => self.page()?.screenshot().await,
                "assist" => self.page()?.assisted(&command["input"]).await,
                _ => Err(Error::new("verification_expired", 409)),
            }
        }
        .await;
        if let Err(error) = &result {
            self.attempts.failed(&error.code)?;
            if !error.is(dispatch_core::Code::InvalidVerificationCode) {
                self.credentials = Value::Null;
            }
        }
        // Deliberately no provider page text, URL, username, password or OTP in logs.
        result
    }
}
/// Whether the job is still this worker's to run: asked again before a read with no page
/// goes on after a wait, as each step with one asks through its progress.
fn authority(run: &Run<'_>) -> impl Fn() -> Pending<'static, ()> + Send + Sync + 'static {
    let (state, job, owner) = (run.state.clone(), run.job.to_owned(), run.owner.to_owned());
    move || {
        let (state, job, owner) = (state.clone(), job.clone(), owner.clone());
        Box::pin(async move {
            state
                .read(move |db| db.guard(&job, &owner).map(|_| ()))
                .await
        })
    }
}
/// What the keeper of meal breaks answers `question`, in a short turn of its own.
async fn asked_of_keeper(run: &Run<'_>, question: Value) -> Result<Value> {
    let job = run.job.to_owned();
    run.state
        .read(move |store| {
            let dsp = store.job_row(&job, None)?.dsp_id;
            registry()
                .keeper(crate::meals::JOB_KIND)
                .kept(store, &dsp, &question)
        })
        .await
}
/// The scope's day of meals read from the live route data the DSP holds of today, with no
/// request to Cortex: none when its keeper has no such read to give, or one the day's meals
/// can't be read from whole, and the day is collected. The keeper gives the read's list, then
/// each itinerary needed, a turn each, and the meal rules read them here; a read replaced
/// meanwhile gives no more, and is no day. Each route names the Cortex page it would be read
/// from, and the day is checked as a collected one is.
async fn live_meals(run: &Run<'_>, scope: &Scope, origin: &str) -> Option<Value> {
    let live = asked_of_keeper(run, json!({"live": scope})).await.ok()?;
    let (at, list) = (live["at"].as_i64()?, live["list"].as_str()?.to_owned());
    let kept = kept_meals(run, scope).await.unwrap_or_default();
    let itinerary = |id: String| async move {
        let question = json!({"liveItinerary": {"scope": scope, "at": at, "id": id}});
        let answer = asked_of_keeper(run, question).await?;
        Ok(answer.as_str().map(str::to_owned))
    };
    let mut day = collection::from_responses(scope, list, &kept, at, itinerary)
        .await
        .ok()??;
    // The read every response came from is the one the keeper still holds.
    let still = asked_of_keeper(run, json!({"live": scope})).await.ok()?;
    if still["at"].as_i64() != Some(at) {
        return None;
    }
    for route in &mut day.itineraries {
        route.source_url = Some(format!("{origin}{}", scope.detail_path(&route.id)));
    }
    day.validate(scope).ok()?;
    serde_json::to_value(day).ok()
}
/// The routes Timecard already holds for `scope`, which the collection need not read again
/// while they stay as they were.
async fn kept_meals(run: &Run<'_>, scope: &Scope) -> Result<Vec<crate::meals::Itinerary>> {
    let job = run.job.to_owned();
    let question = serde_json::to_value(scope)?;
    let kept = run
        .state
        .read(move |store| {
            let dsp = store.job_row(&job, None)?.dsp_id;
            registry()
                .keeper(crate::meals::JOB_KIND)
                .kept(store, &dsp, &question)
        })
        .await?;
    if kept.is_null() {
        return Ok(Vec::new());
    }
    Ok(serde_json::from_value(kept)?)
}
impl Drives for Driver {
    fn request(&mut self, command: Value) -> Pending<'_, Value> {
        Box::pin(Driver::request(self, command))
    }
    fn collect<'a>(&'a mut self, run: &'a Run<'a>) -> Pending<'a, Collected> {
        Box::pin(async move {
            if let Some(request) = crate::associates::Request::parse(run.request)? {
                let capture = self.collect_associates(&request, run).await?;
                return Ok(Collected {
                    data: serde_json::to_value(capture)?,
                    scope: None,
                });
            }
            if let Some(request) = crate::vans::Request::parse(run.request)? {
                let capture = self.collect_vans(&request, run).await?;
                return Ok(Collected {
                    data: serde_json::to_value(capture)?,
                    scope: None,
                });
            }
            if let Some(request) = crate::shifts::Request::parse(run.request)? {
                let capture = self.collect_shifts(&request, run).await?;
                return Ok(Collected {
                    data: serde_json::to_value(capture)?,
                    scope: None,
                });
            }
            if let Some(request) = crate::dvic::Request::parse(run.request)? {
                let (capture, scope) = self.collect_dvic(&request, run).await?;
                return Ok(Collected {
                    data: serde_json::to_value(capture)?,
                    scope: Some(serde_json::to_value(scope)?),
                });
            }
            if let Some(request) = crate::weekly_scorecard::Request::parse(run.request)? {
                let (capture, scope) = self.collect_weekly_scorecard(&request, run).await?;
                return Ok(Collected {
                    data: serde_json::to_value(capture)?,
                    scope: Some(serde_json::to_value(scope)?),
                });
            }
            if let Some(request) = crate::daily_performance::Request::parse(run.request)? {
                let (capture, scope) = self.collect_daily_performance(&request, run).await?;
                return Ok(Collected {
                    data: serde_json::to_value(capture)?,
                    scope: Some(serde_json::to_value(scope)?),
                });
            }
            if let Some(request) = crate::routes::Request::parse(run.request)? {
                let capture = self.collect_routes(&request, run).await?;
                let scope = serde_json::to_value(&capture.scope)?;
                return Ok(Collected {
                    data: serde_json::to_value(capture)?,
                    scope: Some(scope),
                });
            }
            if let Some(request) = crate::meals::DayRequest::parse(run.request)? {
                let count = request.scope_requests.len();
                let mut captures = Vec::with_capacity(count);
                for (index, scope_request) in request.scope_requests.iter().enumerate() {
                    // Every scope begins with a lease/connection/permission check.
                    run.progress(
                        10 + (index * 85 / count) as i64,
                        format!("{} · scope {} of {}", request.date, index + 1, count),
                    )
                    .await?;
                    let scope = self.resolve_scope(scope_request, run.metrics).await?;
                    let data = self
                        .day_of_meals(run, &scope, |progress, message| {
                            run.progress(
                                10 + (index as i64 * 100 + progress) * 85 / (count as i64 * 100),
                                format!("{} · {} · {message}", request.date, scope.station),
                            )
                        })
                        .await?;
                    captures.push(serde_json::from_value(data)?);
                }
                let data = crate::meals::DayCapture {
                    date: request.date.clone(),
                    captures,
                };
                data.validate(&request)?;
                run.progress(95, "Publishing meal breaks".into()).await?;
                return Ok(Collected {
                    data: serde_json::to_value(data)?,
                    scope: None,
                });
            }
            let scope = self
                .resolve_scope(&serde_json::from_value(run.request.clone())?, run.metrics)
                .await?;
            let data = self
                .day_of_meals(run, &scope, |progress, message| {
                    run.progress(progress, message)
                })
                .await?;
            Ok(Collected {
                data,
                scope: Some(serde_json::to_value(scope)?),
            })
        })
    }
    fn browser(&self) -> Option<&browseros::Session> {
        Driver::browser(self).ok()
    }
    /// The session it read with, as its replies left it, when it had no browser; else the
    /// browser's, while it is open.
    fn session(&self) -> Pending<'_, Option<SignedIn>> {
        Box::pin(async move {
            let session = match (&self.saved, &self.application) {
                (Some(http), _) => http.session(),
                (None, Some(application)) => {
                    SignedIn::of(&application.browser, &self.origin, &crate::HOSTS)
                        .await
                        .ok()
                }
                (None, None) => None,
            };
            Ok(session.filter(|session| !session.cookies.is_empty()))
        })
    }
}
impl Driver {
    /// A scope's day of meals: read from the live route data the DSP holds of it when its
    /// keeper can (`live_meals`), and collected from Cortex otherwise, reading again only what
    /// Timecard does not already hold.
    async fn day_of_meals<F, Fut>(
        &mut self,
        run: &Run<'_>,
        scope: &Scope,
        progress: F,
    ) -> Result<Value>
    where
        F: Fn(i64, String) -> Fut,
        Fut: Future<Output = Result<()>>,
    {
        if let Some(day) = live_meals(run, scope, &self.origin).await {
            run.metrics.method("meals_live_route_data");
            // The job is still this worker's to publish it, as a collected day's last step asks.
            progress(95, "Validating meal publication".into()).await?;
            return Ok(day);
        }
        // What Timecard holds only saves reads: without it, every route is read.
        let kept = kept_meals(run, scope).await.unwrap_or_else(|_| {
            run.metrics.detail("meal_kept_unavailable");
            Vec::new()
        });
        Driver::collect(
            self,
            scope,
            &kept,
            &collection::Context {
                metrics: run.metrics,
                live: Some(&crate::meals::Writer::new(
                    run.state.clone(),
                    run.job,
                    run.owner,
                )),
                check: &authority(run),
            },
            progress,
            &collection::MealMethod::default(),
        )
        .await
    }
}
