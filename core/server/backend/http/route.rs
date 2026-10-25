//! How an endpoint is registered. Every helper takes the access the route
//! requires, so a route cannot exist without one, and hands the handler only
//! what that access produced.
use super::{
    input::{Input, Reply},
    middleware,
    upload::{UPLOAD_LIMIT, Upload},
};
use crate::{
    Error, Result, State,
    accounts::{Auth, Context},
    db::Store,
    ensure,
    foundation::{config::Site, crypto, observability::RequestTrace},
    manifest::registry,
};
use axum::{
    body::{Body, HttpBody},
    extract::Request,
    http::{Extensions, Method},
    response::{IntoResponse, Response},
};
use std::{future::Future, pin::Pin, sync::Arc};

/// Who may call a route, as listed in the route inventory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    /// Anyone, signed in or not.
    Public,
    /// Any signed-in user.
    Session,
    /// A signed-in platform owner.
    PlatformOwner,
    /// A member looking at a DSP through a role holding one of the `|`-separated permissions.
    Dsp(&'static str),
    /// An outside agent signed in with its own credentials, never a browser session, as the
    /// agents' piece (`manifest::Agents`) signs it in: what it is allowed, in that piece's
    /// words.
    Agent(&'static str),
}

/// The access a route is registered with. Authorizing yields what the handler works with.
pub trait Grant: Copy + Send + Sync + 'static {
    type Who: Send + 'static;
    fn access(self) -> Access;
    /// Runs inside the same database closure as the handler's own work, so the
    /// check and what it guards are one step under the platform lock.
    fn authorize(self, db: &Store, input: &Input) -> Result<Self::Who>;
    /// Then counts the call against what the caller may do in memory, such as an agent
    /// key's calls a minute. Most callers have nothing to count.
    fn admit(self, _state: &State, _who: &Self::Who) -> Result<()> {
        Ok(())
    }
    /// Refuses a request from what it alone says, before any database turn: a credential not
    /// shaped like one, or an address whose credentials keep being refused. Most callers
    /// refuse nothing here.
    fn gate(self, _state: &State, _input: &Input) -> Result<()> {
        Ok(())
    }
    /// Told that `authorize` refused the request with `error`, for a caller that counts such
    /// refusals.
    fn refused(self, _state: &State, _input: &Input, _error: &Error) {}
    /// The one DSP whose data the caller acts on, for a write's cache: its write evicts only
    /// that DSP's cached reads and those every DSP shares. With none, the write may change
    /// anything, so it advances the global revision.
    fn tenant(self, _who: &Self::Who) -> Option<&str> {
        None
    }
}
#[derive(Clone, Copy)]
pub struct Public;
#[derive(Clone, Copy)]
pub struct Session;
/// A platform owner. The few writes that ask them to have verified who they are recently,
/// such as removing a DSP, ask for it themselves.
#[derive(Clone, Copy)]
pub struct PlatformOwner;
#[derive(Clone, Copy)]
pub struct Dsp(pub &'static str);

impl Grant for Public {
    type Who = ();
    fn access(self) -> Access {
        Access::Public
    }
    fn authorize(self, _: &Store, _: &Input) -> Result<()> {
        Ok(())
    }
}
impl Grant for Session {
    type Who = Auth;
    fn access(self) -> Access {
        Access::Session
    }
    fn authorize(self, db: &Store, input: &Input) -> Result<Auth> {
        let support = input.support_read_token(db.config.development);
        let raw = if let Some(raw) = input.observation_token(&db.config)? {
            crate::accounts::browser::at_origin(&db.config, input)?;
            raw
        } else if support.is_empty() {
            let session = input.session_token(db.config.development);
            ensure(
                !session.starts_with(crate::accounts::browser::PREFIX),
                "sign_in_required",
                401,
            )?;
            session
        } else {
            ensure(
                support.starts_with(crate::support::browser::PREFIX),
                "sign_in_required",
                401,
            )?;
            support
        };
        let auth = db.authenticate(raw, &input.site)?;
        if crate::accounts::browser::read_only(&auth) {
            ensure(
                input.method == Method::GET
                    || (input.method == Method::POST && input.path == "/api/session/dsp"),
                if auth.raw.starts_with(crate::accounts::browser::PREFIX) {
                    "observation_read_only"
                } else {
                    "support_read_only"
                },
                403,
            )?;
        }
        {
            let mut trace = input
                .trace
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            trace.actor = Some(auth.user.id.clone());
            trace.account = Some(crypto::sign(
                &db.key,
                &format!("account:{}", auth.user.email.to_lowercase()),
            ));
        }
        if input.method == Method::POST {
            let sent = input.header("x-csrf-token");
            ensure(crypto::equal(&auth.csrf, sent), "csrf_required", 403)?;
        }
        if input.path != "/api/session"
            && input.path != "/api/auth/logout"
            && !input.path.starts_with("/api/auth/security/")
        {
            db.ensure_mfa(&auth)?;
        }
        Ok(auth)
    }
}
impl Grant for PlatformOwner {
    type Who = Auth;
    fn access(self) -> Access {
        Access::PlatformOwner
    }
    fn authorize(self, db: &Store, input: &Input) -> Result<Auth> {
        let auth = Session.authorize(db, input)?;
        ensure(auth.user.platform_owner, "platform_owner_required", 403)?;
        Ok(auth)
    }
}
impl Grant for Dsp {
    type Who = Context;
    fn access(self) -> Access {
        Access::Dsp(self.0)
    }
    fn authorize(self, db: &Store, input: &Input) -> Result<Context> {
        let auth = Session.authorize(db, input)?;
        let context = db.from_view(&auth, input.header("x-dispatch-view"), self.0)?;
        input
            .trace
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .tenant = Some(context.dsp.id.clone());
        Ok(context)
    }
    fn tenant(self, who: &Context) -> Option<&str> {
        Some(&who.dsp.id)
    }
}
impl Dsp {
    /// Checks again, after work done outside the database, that the member may still do this.
    pub fn revalidate(self, db: &Store, context: &Context) -> Result<Context> {
        db.revalidate(context, self.0)
    }
}

/// What a database handler works with: who is calling, and the shared state.
pub struct Ctx<'a, W> {
    pub state: &'a State,
    who: W,
}
impl<W> std::ops::Deref for Ctx<'_, W> {
    type Target = W;
    fn deref(&self) -> &W {
        &self.who
    }
}
impl Ctx<'_, Auth> {
    /// The signed-in user's id, for the audit log.
    pub fn actor(&self) -> &str {
        self.who.user.id.as_str()
    }
}
impl Ctx<'_, Context> {
    pub fn actor(&self) -> &str {
        self.who.auth.user.id.as_str()
    }
    pub fn dsp_id(&self) -> &str {
        self.who.dsp.id.as_str()
    }
}
pub type Anyone<'a> = Ctx<'a, ()>;
pub type User<'a> = Ctx<'a, Auth>;
pub type Member<'a> = Ctx<'a, Context>;

/// Which database access a route's work runs under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Work {
    /// One closure under the shared lock: `State::read`.
    Read,
    /// One closure under the exclusive platform lock, which says what it changes once its
    /// caller is authorized: `State::run_declaring`.
    Write,
    /// Reviewed leases and checkpoints, under the exclusive lock without evicting data.
    Bookkeeping,
    /// An async handler that makes its own `State::read` and `State::run` calls.
    Async,
    /// An async handler, as `Async`, given a file as it arrives rather than a JSON body.
    Upload,
    /// Answered from memory, before the query and body are even parsed.
    Memory,
}

/// A route's handler, told whether its work is a write whose changes it says once it knows its
/// caller, with `State::changed`.
type Blocking = Arc<dyn Fn(&Store, &State, &Input, bool) -> Result<Reply> + Send + Sync>;
/// A route's gate: its access's `Grant::gate`, run before the database is asked for.
type Gate = Arc<dyn Fn(&State, &Input) -> Result<()> + Send + Sync>;
fn gate_of<A: Grant>(access: A) -> Gate {
    Arc::new(move |state, input| access.gate(state, input))
}
fn open() -> Gate {
    Arc::new(|_, _| Ok(()))
}
type Pending = Pin<Box<dyn Future<Output = Result<Reply>> + Send>>;
pub type Served = Pin<Box<dyn Future<Output = Response> + Send>>;
type Uploaded = Box<dyn Fn(Arc<State>, Input, Upload) -> Pending + Send + Sync>;
/// Signs in the caller of a protocol route under the shared lock, counts the call, and puts
/// who it is in the request's extensions.
type Signer = Arc<
    dyn Fn(&Store, &State, &Input) -> Result<Box<dyn FnOnce(&mut Extensions) + Send>> + Send + Sync,
>;
enum Handler {
    Blocking(Blocking),
    Async(Box<dyn Fn(Arc<State>, Input) -> Pending + Send + Sync>),
    /// Its most bytes, and the handler.
    Upload(u64, Uploaded),
    Memory(fn(&State) -> Reply),
    Protocol(Signer, fn(Request) -> Served),
    Open(fn(Arc<State>, Request) -> Served),
    Page(fn(Arc<State>, Request) -> Served),
}

/// The paths a logged route covers, and the route the request log names them by.
pub(super) struct LogLabel {
    prefix: &'static str,
    /// Every path that begins with `prefix`, rather than only `prefix` itself.
    beneath: bool,
    label: String,
}
impl LogLabel {
    pub(super) fn names(&self, path: &str) -> Option<&str> {
        let covered = if self.beneath {
            path.starts_with(self.prefix)
        } else {
            path == self.prefix
        };
        covered.then_some(self.label.as_str())
    }
}
pub struct Route {
    pub method: Method,
    pub path: &'static str,
    pub access: Access,
    pub work: Work,
    pub invalidates_schedules: bool,
    logged: bool,
    gate: Gate,
    handler: Handler,
}

fn blocking<A: Grant>(
    path: &'static str,
    access: A,
    work: Work,
    method: Method,
    handler: impl Fn(&Store, &Ctx<'_, A::Who>, &Input) -> Result<Reply> + Send + Sync + 'static,
) -> Route {
    let handler = move |db: &Store, state: &State, input: &Input, writes: bool| {
        let who = access
            .authorize(db, input)
            .inspect_err(|error| access.refused(state, input, error))?;
        access.admit(state, &who)?;
        // A write on a DSP's route changes that DSP's data, and any other changes anything.
        if writes {
            state.changed(access.tenant(&who));
        }
        handler(db, &Ctx { state, who }, input)
    };
    Route {
        method,
        path,
        access: access.access(),
        work,
        invalidates_schedules: false,
        logged: false,
        gate: gate_of(access),
        handler: Handler::Blocking(Arc::new(handler)),
    }
}
/// `GET path`: authorizes, then runs the handler, in one shared-lock database closure.
pub fn read<A: Grant>(
    path: &'static str,
    access: A,
    handler: impl Fn(&Store, &Ctx<'_, A::Who>, &Input) -> Result<Reply> + Send + Sync + 'static,
) -> Route {
    blocking(path, access, Work::Read, Method::GET, handler)
}
/// An expensive cached DSP read. Waiters hold no database slot; each checks its
/// credentials before waiting and again alongside the handler's read afterward.
pub fn coalesced_read(
    path: &'static str,
    access: Dsp,
    handler: impl Fn(&Store, &Member<'_>, &Input) -> Result<Reply> + Send + Sync + 'static,
) -> Route {
    coalesced_read_by(
        path,
        access,
        |_, _, input| Ok(serde_json::to_string(&(&input.path, &input.query))?),
        handler,
    )
}
/// Like `coalesced_read`, keyed by the underlying cached read rather than its page or
/// date alias. The key reads only cheap metadata after authorization. Every waiter
/// still authorizes and runs its own handler with the latest data.
pub fn coalesced_read_by(
    path: &'static str,
    access: Dsp,
    key: impl Fn(&Store, &Member<'_>, &Input) -> Result<String> + Send + Sync + 'static,
    handler: impl Fn(&Store, &Member<'_>, &Input) -> Result<Reply> + Send + Sync + 'static,
) -> Route {
    let key = Arc::new(key);
    let mut route = read(path, access, handler);
    let Handler::Blocking(handler) = route.handler else {
        unreachable!()
    };
    route.handler = Handler::Async(Box::new(move |state, input| {
        let handler = handler.clone();
        let key = key.clone();
        Box::pin(async move {
            let checking = input.clone();
            let shared = state.clone();
            let key = state
                .read(move |db| {
                    let who = access.authorize(db, &checking)?;
                    let context = Ctx {
                        state: &shared,
                        who,
                    };
                    Ok(serde_json::to_string(&(
                        context.dsp_id(),
                        path,
                        key(db, &context, &checking)?,
                    ))?)
                })
                .await?;
            let turn = state.read_turns.enter(key).await?;
            let shared = state.clone();
            state
                .read(move |db| {
                    let _turn = turn;
                    handler(db, &shared, &input, false)
                })
                .await
        })
    }));
    route
}
/// `POST path`: authorizes, then runs the handler, in one closure under the platform write lock.
pub fn write<A: Grant>(
    path: &'static str,
    access: A,
    handler: impl Fn(&Store, &Ctx<'_, A::Who>, &Input) -> Result<Reply> + Send + Sync + 'static,
) -> Route {
    blocking(path, access, Work::Write, Method::POST, handler)
}
fn asynchronous<A: Grant, F>(
    method: Method,
    path: &'static str,
    access: A,
    handler: impl Fn(Arc<State>, Input, A) -> F + Send + Sync + 'static,
) -> Route
where
    F: Future<Output = Result<Reply>> + Send + 'static,
{
    let handler = move |state, input| Box::pin(handler(state, input, access)) as Pending;
    Route {
        method,
        path,
        access: access.access(),
        work: Work::Async,
        invalidates_schedules: false,
        logged: false,
        gate: gate_of(access),
        handler: Handler::Async(Box::new(handler)),
    }
}
/// `GET path` for work that waits outside the database. The handler receives the
/// registered access and must call `access.authorize(db, &input)` inside its own
/// `state.read`/`state.run` closure; leaving `access` unused fails the build's lints.
pub fn async_get<A: Grant, F>(
    path: &'static str,
    access: A,
    handler: impl Fn(Arc<State>, Input, A) -> F + Send + Sync + 'static,
) -> Route
where
    F: Future<Output = Result<Reply>> + Send + 'static,
{
    asynchronous(Method::GET, path, access, handler)
}
/// `POST path` for work that waits outside the database. See [`async_get`].
pub fn async_post<A: Grant, F>(
    path: &'static str,
    access: A,
    handler: impl Fn(Arc<State>, Input, A) -> F + Send + Sync + 'static,
) -> Route
where
    F: Future<Output = Result<Reply>> + Send + 'static,
{
    asynchronous(Method::POST, path, access, handler)
}
/// `POST path` taking a file as it arrives, of at most `limit` bytes, which may not exceed
/// [`UPLOAD_LIMIT`]: `application/octet-stream` of a stated length, its name and the like in
/// the query. As with [`async_get`], the handler calls `access.authorize(db, &input)` itself,
/// before it reads the upload.
pub fn upload<A: Grant, F>(
    path: &'static str,
    access: A,
    limit: u64,
    handler: impl Fn(Arc<State>, Input, A, Upload) -> F + Send + Sync + 'static,
) -> Route
where
    F: Future<Output = Result<Reply>> + Send + 'static,
{
    assert!(limit <= UPLOAD_LIMIT, "{path}: an upload of at most 100 MB");
    let handler =
        move |state, input, upload| Box::pin(handler(state, input, access, upload)) as Pending;
    Route {
        method: Method::POST,
        path,
        access: access.access(),
        work: Work::Upload,
        invalidates_schedules: false,
        logged: false,
        gate: gate_of(access),
        handler: Handler::Upload(limit, Box::new(handler)),
    }
}
/// `method path` for a protocol an agent speaks whose requests the handler reads itself,
/// such as MCP. The caller is signed in and counted first, under the shared lock; the
/// handler then finds who it is (`A::Who`) and the server's `Arc<State>` in the request's
/// extensions.
pub fn agent_protocol<A: Grant>(
    method: Method,
    path: &'static str,
    access: A,
    handler: fn(Request) -> Served,
) -> Route
where
    A::Who: Clone + Sync,
{
    let signer: Signer = Arc::new(move |db: &Store, state: &State, input: &Input| {
        let who = access
            .authorize(db, input)
            .inspect_err(|error| access.refused(state, input, error))?;
        access.admit(state, &who)?;
        Ok(Box::new(move |extensions: &mut Extensions| {
            extensions.insert(who);
        }))
    });
    Route {
        method,
        path,
        access: access.access(),
        work: Work::Async,
        invalidates_schedules: false,
        logged: false,
        gate: gate_of(access),
        handler: Handler::Protocol(signer, handler),
    }
}
/// How an agent's request ended, as its route saw it, for the agents' piece to record.
pub struct Answered<'a> {
    /// The route's registered path.
    pub path: &'static str,
    pub at: i64,
    pub ms: u128,
    pub bytes: u64,
    pub status: u16,
    /// The code the route failed with, when it failed rather than answered.
    pub failed: Option<&'a str>,
}
/// A public `method path` whose requests the handler reads and answers itself, as OAuth's
/// form posts and redirects need. Such a path sits outside `/api/`, so the JSON-only rule
/// and the Origin check that guard the dashboard's API never apply to it.
pub fn protocol(
    method: Method,
    path: &'static str,
    handler: fn(Arc<State>, Request) -> Served,
) -> Route {
    Route {
        method,
        path,
        access: Access::Public,
        work: Work::Async,
        invalidates_schedules: false,
        logged: false,
        gate: open(),
        handler: Handler::Open(handler),
    }
}
/// A public `GET` page of Dispatch's own that the handler answers itself, at every address:
/// one that sets its own rules, as for an outside service's scripts.
pub fn page(path: &'static str, handler: fn(Arc<State>, Request) -> Served) -> Route {
    Route {
        handler: Handler::Page(handler),
        ..protocol(Method::GET, path, handler)
    }
}
/// A public `GET` answered from memory: no session, query, body or database.
pub fn probe(path: &'static str, handler: fn(&State) -> Reply) -> Route {
    Route {
        method: Method::GET,
        path,
        access: Access::Public,
        work: Work::Memory,
        invalidates_schedules: false,
        logged: false,
        gate: open(),
        handler: Handler::Memory(handler),
    }
}

impl Route {
    /// The owner's reviewed write changes only bookkeeping, never data a cached read
    /// uses. It keeps the write lock and all of the route's authorization checks.
    pub fn bookkeeping(mut self) -> Self {
        assert_eq!(self.work, Work::Write);
        self.work = Work::Bookkeeping;
        self
    }
    /// Wakes the scheduler after the handler succeeds, because the route
    /// changes which DSPs or schedules exist.
    pub fn invalidates_schedules(mut self) -> Self {
        self.invalidates_schedules = true;
        self
    }
    /// Names its requests in the request log by its pattern even when one is refused before
    /// the route reads it, each `{name}` spelled `:name`. A pattern's parameter must end it,
    /// and names every path beneath the part before it.
    pub fn logged(mut self) -> Self {
        self.logged = true;
        self
    }
    /// How the request log names the paths it covers, when it is logged.
    pub(super) fn log_label(&self) -> Option<LogLabel> {
        if !self.logged {
            return None;
        }
        let Some(start) = self.path.find('{') else {
            return Some(LogLabel {
                prefix: self.path,
                beneath: false,
                label: self.path.to_owned(),
            });
        };
        let name = self.path[start..]
            .strip_prefix('{')
            .and_then(|rest| rest.strip_suffix('}'))
            .filter(|name| !name.contains(['/', '{', '}']))
            .unwrap_or_else(|| panic!("{}: a logged pattern's parameter ends it", self.path));
        let prefix = &self.path[..start];
        Some(LogLabel {
            prefix,
            beneath: true,
            label: format!("{prefix}:{name}"),
        })
    }
    /// Answers `GET` although it was registered with `write`, for a read that
    /// still has to record something.
    pub fn get(mut self) -> Self {
        self.method = Method::GET;
        self
    }
    pub(super) async fn serve(&self, state: Arc<State>, request: Request) -> Response {
        // An agent's call is noted as the request goes, and handed to the agents' piece once
        // answered, which records it in memory, so recording it never waits for the database.
        let call = matches!(self.access, Access::Agent(_)).then(|| {
            let trace = request.extensions().get::<RequestTrace>().cloned();
            (crate::db::now(), std::time::Instant::now(), trace)
        });
        let (response, failed) = match self.answer(state.clone(), request).await {
            Ok(response) => (response, None),
            Err(error) => {
                let code = error.code.clone();
                (middleware::failure(error), Some(code))
            }
        };
        if let (Some((at, started, Some(trace))), Some(agents)) = (call, registry().agents) {
            let noted = trace
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .agent
                .take();
            let size = response.body().size_hint();
            (agents.answered)(
                &state,
                noted,
                Answered {
                    path: self.path,
                    at,
                    ms: started.elapsed().as_millis(),
                    bytes: size.exact().unwrap_or(size.lower()),
                    status: response.status().as_u16(),
                    failed: failed.as_deref(),
                },
            );
        }
        response
    }
    async fn answer(&self, state: Arc<State>, request: Request) -> Result<Response> {
        // Even public routes and GET routes registered as writes cannot mutate through the
        // support browser. The one POST opens a signed view and skips its visit audit below.
        let support_cookie = if state.config.development {
            "dispatch_support_read="
        } else {
            "__Host-dispatch_support_read="
        };
        let support = request
            .headers()
            .get_all("cookie")
            .iter()
            .filter_map(|h| h.to_str().ok())
            .flat_map(|h| h.split(';'))
            .any(|part| part.trim().starts_with(support_cookie));
        let observation = crate::accounts::browser::restricted_request(request.headers());
        if support || observation {
            let view = self.path == "/api/session/dsp" && request.method() == Method::POST;
            let exchange = observation
                && !support
                && self.path == crate::accounts::browser::EXCHANGE
                && request.method() == Method::POST;
            ensure(
                view || exchange
                    || (request.method() == Method::GET
                        && matches!(self.work, Work::Read | Work::Memory)),
                if observation {
                    "observation_read_only"
                } else {
                    "support_read_only"
                },
                403,
            )?;
        }
        let handler = match &self.handler {
            Handler::Memory(handler) => return Ok(handler(&state).into_response()),
            Handler::Async(handler) => {
                let input = middleware::input(&state, request, self.path).await?;
                (self.gate)(&state, &input)?;
                return Ok(handler(state, input).await?.into_response());
            }
            Handler::Protocol(signer, handler) => {
                let request = self.signed(signer.clone(), state, request).await?;
                return Ok(handler(request).await);
            }
            // Agents' sign-in, and the pages outside services send the browser back to, are
            // the admin's alone.
            Handler::Open(handler) => {
                let admin = request.extensions().get::<Site>() == Some(&Site::Admin);
                ensure(admin, "not_found", 404)?;
                return Ok(handler(state, request).await);
            }
            Handler::Page(handler) => return Ok(handler(state, request).await),
            Handler::Upload(limit, handler) => {
                let (parts, body) = request.into_parts();
                let input = middleware::head(&state, &parts, self.path)?;
                (self.gate)(&state, &input)?;
                let upload = Upload::begin(&parts, body, *limit)?;
                return Ok(handler(state, input, upload).await?.into_response());
            }
            Handler::Blocking(handler) => handler.clone(),
        };
        let input = middleware::input(&state, request, self.path).await?;
        (self.gate)(&state, &input)?;
        let shared = state.clone();
        let writes = self.work == Work::Write;
        let work = move |db: &Store| handler(db, &shared, &input, writes);
        let reply = match self.work {
            Work::Read => state.read(work).await?,
            Work::Bookkeeping => state.run_bookkeeping(work).await?,
            _ => state.run_declaring(work).await?,
        };
        if self.invalidates_schedules {
            state
                .schedule_revision
                .fetch_add(1, std::sync::atomic::Ordering::Release);
        }
        Ok(reply.into_response())
    }
    /// The request with its agent signed in and counted, carrying who it is and the server's
    /// state for the protocol's handler. The body keeps the usual limits.
    async fn signed(&self, signer: Signer, state: Arc<State>, request: Request) -> Result<Request> {
        let (mut parts, body) = request.into_parts();
        let input = middleware::head(&state, &parts, self.path)?;
        (self.gate)(&state, &input)?;
        middleware::json_only(&parts)?;
        let body = middleware::bytes(body).await?;
        // What the message calls, if anything, before the caller is even counted: a call
        // refused for its rate is still noted as what it called.
        if let Some(agents) = registry().agents {
            (agents.called)(&input.trace, &body);
        }
        let shared = state.clone();
        let who = state.read(move |db| signer(db, &shared, &input)).await?;
        who(&mut parts.extensions);
        parts.extensions.insert(state);
        Ok(Request::from_parts(parts, Body::from(body)))
    }
}
