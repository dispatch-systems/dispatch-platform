//! How an endpoint is registered. Every helper takes the access the route
//! requires, so a route cannot exist without one, and hands the handler only
//! what that access produced.
use super::{
    input::{Input, Reply},
    middleware,
};
use crate::{
    Result, State,
    accounts::{Auth, Context},
    agents::{self, Caller},
    contracts::AgentAccess,
    crypto,
    db::Store,
    ensure,
};
use axum::{
    body::Body,
    extract::Request,
    http::Method,
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
    /// An outside agent signed in with a key, or a connected app with its access token:
    /// `read` for any, `operator` for a key that may also run collections and test connections.
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
}
#[derive(Clone, Copy)]
pub struct Public;
#[derive(Clone, Copy)]
pub struct Session;
/// A platform owner who verified their identity recently, for writes that create,
/// suspend or remove a DSP and the like.
#[derive(Clone, Copy)]
pub struct PlatformOwner;
/// A platform owner doing a routine, reversible change that asks for no fresh
/// verification: switching a DSP's features.
#[derive(Clone, Copy)]
pub struct PlatformRoutine;
#[derive(Clone, Copy)]
pub struct Dsp(pub &'static str);
/// An outside agent's key or a connected app's access token, never a browser session.
/// `Agent::READ` is any; a route that acts would ask for `AgentAccess::Operator`.
#[derive(Clone, Copy)]
pub struct Agent(AgentAccess);
impl Agent {
    pub const READ: Agent = Agent(AgentAccess::Read);
}

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
        let auth = db.authenticate(input.session_token(db.config.development))?;
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
        let owner = auth.user.platform_owner;
        ensure(owner, "platform_owner_required", 403)?;
        if input.method == Method::POST {
            db.ensure_recent(&auth)?;
        }
        Ok(auth)
    }
}
impl Grant for PlatformRoutine {
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
        // Editing a role is routine and reversible, so it asks for no fresh verification.
        if input.method == Method::POST
            && ["connections.manage", "members.manage", "members.invite"].contains(&self.0)
        {
            db.ensure_recent(&auth)?;
        }
        let context = db.from_view(&auth, input.header("x-dispatch-view"), self.0)?;
        input
            .trace
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .tenant = Some(context.dsp.id.clone());
        Ok(context)
    }
}
impl Grant for Agent {
    type Who = Caller;
    fn access(self) -> Access {
        Access::Agent(self.0.as_str())
    }
    fn authorize(self, db: &Store, input: &Input) -> Result<Caller> {
        let header = input.header("authorization");
        let (token, bearer) = match header.split_once(' ') {
            Some((scheme, token)) if scheme.eq_ignore_ascii_case("bearer") => (token.trim(), true),
            _ if header.is_empty() => (input.header("x-api-key").trim(), false),
            _ => ("", false),
        };
        ensure(!token.is_empty(), "agent_key_required", 401)?;
        // A key and a session never travel together, so a browser can never lend its
        // session to a request an agent sends, or the other way round.
        ensure(
            input.session_token(db.config.development).is_empty(),
            "session_and_key",
            400,
        )?;
        let client = agents::client_label(input.header("user-agent"));
        // A connected app signs in with its OAuth access token, and only ever as a bearer.
        let caller = if bearer && token.starts_with("dsa_") {
            db.authenticate_app(token, &client)?
        } else {
            db.authenticate_agent(token, &client)?
        };
        input
            .trace
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .actor = Some(format!("agent:{}", caller.key));
        ensure(
            self.0 == AgentAccess::Read || caller.access == AgentAccess::Operator,
            "agent_read_only",
            403,
        )?;
        Ok(caller)
    }
    fn admit(self, state: &State, who: &Caller) -> Result<()> {
        state.agents.admit(&who.key, &who.client)
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
    /// One closure under the exclusive platform lock: `State::run`.
    Write,
    /// An async handler that makes its own `State::read` and `State::run` calls.
    Async,
    /// Answered from memory, before the query and body are even parsed.
    Memory,
}

type Blocking = Arc<dyn Fn(&Store, &State, &Input) -> Result<Reply> + Send + Sync>;
type Pending = Pin<Box<dyn Future<Output = Result<Reply>> + Send>>;
pub type Served = Pin<Box<dyn Future<Output = Response> + Send>>;
enum Handler {
    Blocking(Blocking),
    Async(Box<dyn Fn(Arc<State>, Input) -> Pending + Send + Sync>),
    Memory(fn(&State) -> Reply),
    Protocol(Agent, fn(Request) -> Served),
    Open(fn(Arc<State>, Request) -> Served),
}

pub struct Route {
    pub method: Method,
    pub path: &'static str,
    pub access: Access,
    pub work: Work,
    pub invalidates_schedules: bool,
    handler: Handler,
}

fn blocking<A: Grant>(
    path: &'static str,
    access: A,
    work: Work,
    method: Method,
    handler: impl Fn(&Store, &Ctx<'_, A::Who>, &Input) -> Result<Reply> + Send + Sync + 'static,
) -> Route {
    let handler = move |db: &Store, state: &State, input: &Input| {
        let who = access.authorize(db, input)?;
        access.admit(state, &who)?;
        handler(db, &Ctx { state, who }, input)
    };
    Route {
        method,
        path,
        access: access.access(),
        work,
        invalidates_schedules: false,
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
/// `method path` for a protocol an agent speaks whose requests the handler reads itself,
/// such as MCP. The key is checked and counted first, under the shared lock; the handler
/// then finds the `Caller` and the server's `Arc<State>` in the request's extensions.
pub fn agent_protocol(
    method: Method,
    path: &'static str,
    access: Agent,
    handler: fn(Request) -> Served,
) -> Route {
    Route {
        method,
        path,
        access: access.access(),
        work: Work::Async,
        invalidates_schedules: false,
        handler: Handler::Protocol(access, handler),
    }
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
        handler: Handler::Open(handler),
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
        handler: Handler::Memory(handler),
    }
}

impl Route {
    /// Wakes the scheduler after the handler succeeds, because the route
    /// changes which DSPs or schedules exist.
    pub fn invalidates_schedules(mut self) -> Self {
        self.invalidates_schedules = true;
        self
    }
    /// Answers `GET` although it was registered with `write`, for a read that
    /// still has to record something.
    pub fn get(mut self) -> Self {
        self.method = Method::GET;
        self
    }
    pub(super) async fn serve(&self, state: Arc<State>, request: Request) -> Response {
        match self.answer(state, request).await {
            Ok(response) => response,
            Err(error) => middleware::failure(error),
        }
    }
    async fn answer(&self, state: Arc<State>, request: Request) -> Result<Response> {
        let handler = match &self.handler {
            Handler::Memory(handler) => return Ok(handler(&state).into_response()),
            Handler::Async(handler) => {
                let input = middleware::input(&state, request, self.path).await?;
                return Ok(handler(state, input).await?.into_response());
            }
            Handler::Protocol(access, handler) => {
                let request = self.signed(*access, state, request).await?;
                return Ok(handler(request).await);
            }
            Handler::Open(handler) => return Ok(handler(state, request).await),
            Handler::Blocking(handler) => handler.clone(),
        };
        let input = middleware::input(&state, request, self.path).await?;
        let shared = state.clone();
        let work = move |db: &Store| handler(db, &shared, &input);
        let reply = if self.work == Work::Read {
            state.read(work).await?
        } else {
            state.run(work).await?
        };
        if self.invalidates_schedules {
            state
                .schedule_revision
                .fetch_add(1, std::sync::atomic::Ordering::Release);
        }
        Ok(reply.into_response())
    }
    /// The request with its agent's key checked and counted, carrying the caller and the
    /// server's state for the protocol's handler. The body keeps the usual limits.
    async fn signed(&self, access: Agent, state: Arc<State>, request: Request) -> Result<Request> {
        let (mut parts, body) = request.into_parts();
        let input = middleware::head(&state, &parts, self.path)?;
        let body = middleware::bytes(body).await?;
        let shared = state.clone();
        let caller = state
            .read(move |db| {
                let caller = access.authorize(db, &input)?;
                access.admit(&shared, &caller)?;
                Ok(caller)
            })
            .await?;
        parts.extensions.insert(caller);
        parts.extensions.insert(state);
        Ok(Request::from_parts(parts, Body::from(body)))
    }
}
