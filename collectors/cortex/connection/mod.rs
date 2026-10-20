//! Cortex authentication and performance transport from Amazon Logistics.
pub mod performance;
use crate::collections::associates::collect::COMPANIES;
use crate::collections::meals::collect as collection;
use crate::discovery::{Scope, known::KnownScopes};
use dispatch_core::collection::browser::{
    Collected, Driver as Drives, Pending, Run,
    attempt::Attempts,
    browseros,
    http::{Http, Refusal},
    page::{Page, call},
};
use dispatch_core::{Error, Result, db::s, ensure, manifest::registry};
use serde_json::{Value, json};
use std::{path::Path, time::Duration};
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
fn console(html: &str) -> bool {
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
pub struct Driver {
    pub browser: browseros::Session,
    pub(crate) page: Page,
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
        let origin = fixture.unwrap_or(ORIGIN).trim_end_matches('/').to_owned();
        let origins = if fixture.is_some() {
            vec![origin.clone()]
        } else {
            ORIGINS.iter().map(|v| (*v).to_owned()).collect()
        };
        Ok(Self {
            page: Page::empty(browser.clone(), origin.clone()),
            browser,
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
    async fn open(&mut self) -> Result<()> {
        self.page = Page::open(self.browser.clone(), self.origin.clone()).await?;
        self.page
            .allow_origins(&self.origins.iter().map(String::as_str).collect::<Vec<_>>());
        self.page.size_window().await?;
        self.page.front_alone().await
    }
    /// The application's tab, opened the first time a collection needs one: a session
    /// that started from the saved one has none until then.
    pub(crate) async fn ensure_page(&mut self) -> Result<()> {
        if self.page.id.is_empty() {
            self.open().await?;
        }
        Ok(())
    }
    /// Whether the session the profile kept still signs this browser in: it kept
    /// Amazon's cookies, Amazon serves the console page itself rather than sending it to
    /// a sign-in or a verification, and the company list, which Amazon answers only a
    /// signed-in account, comes back with the account's companies. Anything else signs
    /// in on the page as before, where a verification reaches the owner. Amazon answered
    /// both in under half a second when tried. Answers the company list when the session
    /// stands.
    async fn saved_session(&self) -> Result<Option<Value>> {
        let http = Http::signed_in(&self.browser, &self.origin, &crate::HOSTS).await?;
        if !http.carries_cookies() {
            return Ok(None);
        }
        let landing = format!("{}{LANDING}", self.origin);
        let home = format!("{}/", self.origin);
        let companies = format!("{}{COMPANIES}", self.origin);
        let workforce = format!("{}/workforce", self.origin);
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
        Ok(futures_util::future::try_join(console, companies)
            .await
            .ok()
            .map(|(_, companies)| companies))
    }
    async fn script(&self, mut input: Value) -> Result<Value> {
        input["origins"] = json!(self.origins);
        input["applicationOrigin"] = json!(self.origin);
        self.page.evaluate(&call(AUTH, &input)).await
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
                    let frame = self.page.frame().await?;
                    ensure(
                        s(&frame, "url") == "about:blank" || self.page.trusted(s(&frame, "url")),
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
    pub async fn request(&mut self, command: Value) -> Result<Value> {
        let result = async {
            // A session that started from the saved one has no tab to verify on.
            if ["complete_assistance", "verify", "screenshot", "assist"]
                .contains(&s(&command, "action"))
            {
                ensure(!self.page.id.is_empty(), "verification_expired", 409)?;
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
                        && self.page.id.is_empty()
                        && let Some(companies) = self.saved_session().await?
                    {
                        self.attempts.succeeded()?;
                        self.credentials = Value::Null;
                        self.companies = Some(companies);
                        return Ok(json!({"type":"ready"}));
                    }
                    self.ensure_page().await?;
                    self.page.navigate(LANDING).await?;
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
                "screenshot" => self.page.screenshot().await,
                "assist" => self.page.assisted(&command["input"]).await,
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
                    let kept = kept_meals(run, &scope).await.unwrap_or_else(|_| {
                        run.metrics.detail("meal_kept_unavailable");
                        Vec::new()
                    });
                    let data = Driver::collect(
                        self,
                        &scope,
                        &kept,
                        run.metrics,
                        Some(&crate::meals::Writer::new(
                            run.state.clone(),
                            run.job,
                            run.owner,
                        )),
                        |progress, message| {
                            run.progress(
                                10 + (index as i64 * 100 + progress) * 85 / (count as i64 * 100),
                                format!("{} · {} · {message}", request.date, scope.station),
                            )
                        },
                        &collection::MealMethod::default(),
                    )
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
            // What Timecard holds only saves reads: without it, every route is read.
            let kept = kept_meals(run, &scope).await.unwrap_or_else(|_| {
                run.metrics.detail("meal_kept_unavailable");
                Vec::new()
            });
            let data = Driver::collect(
                self,
                &scope,
                &kept,
                run.metrics,
                Some(&crate::meals::Writer::new(
                    run.state.clone(),
                    run.job,
                    run.owner,
                )),
                |progress, message| run.progress(progress, message),
                &collection::MealMethod::default(),
            )
            .await?;
            Ok(Collected {
                data,
                scope: Some(serde_json::to_value(scope)?),
            })
        })
    }
    fn browser(&self) -> Option<&browseros::Session> {
        Some(&self.browser)
    }
}
