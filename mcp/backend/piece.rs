//! How the MCP plugs into core, as the agents' piece (`manifest::Agents`): its routes, tables
//! and upkeep, what it keeps in memory, how an agent's request is signed in, and what it does
//! when core tells it a request was answered, a password was reset or a backup restored. Core
//! reaches it through these and nothing else.
use super::{
    Caller, activity,
    api::{
        routes::{agent_api, oauth as oauth_routes},
        types::{AgentAccess, AgentActivityKey, AgentKeyKind},
    },
    client_label, migrations, oauth,
    tools::{self, AnyTool},
    usage::Usage,
};
use crate::{ActivityStore, KeyStore, OAuthStore};
use dispatch_core::{
    Result, State,
    db::{Store, iso, now},
    ensure,
    foundation::{
        config::{Config, Site},
        observability::{self, RequestTrace},
    },
    manifest::{Agents, Audit, Maintenance, Upkeep, registry},
    server::http::{Answered, Input, route::Grant},
    tenancy::api::audit::AuditArea,
};
use serde_json::json;
use std::{
    any::Any,
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    },
    time::Duration,
};

/// The MCP, as the app installs it.
pub const AGENTS: Agents = piece(&Own { tools: &[] });

/// The MCP with `own`'s tools beside core's, as the app installs it or a test installs it
/// with tools of its own.
pub const fn piece(own: &'static Own) -> Agents {
    Agents {
        routes,
        migrations: migrations::MIGRATIONS,
        tables: migrations::TABLES,
        audit: Audit {
            areas: &[("agent.", AuditArea::Access)],
            ..Audit::NONE
        },
        maintenance: MAINTENANCE,
        state,
        tick,
        authorize,
        challenge,
        called,
        answered,
        password_reset,
        restored,
        check,
        own,
    }
}

/// Each minute's upkeep.
const MAINTENANCE: &[Maintenance] = &[Maintenance {
    every: Duration::from_secs(60),
    run: upkeep,
}];

/// What only the MCP reads of its own declaration: the tools it offers beside core's.
pub struct Own {
    pub tools: &'static [&'static dyn AnyTool],
}
/// The installed MCP's own declaration.
pub fn own() -> &'static Own {
    registry()
        .agents
        .and_then(|agents| agents.own.downcast_ref::<Own>())
        .expect("the MCP is the installed agents' piece")
}

/// What the MCP keeps in memory for as long as the server runs.
pub struct Kept {
    /// How much each key is used, until the scheduler writes it down.
    pub usage: Usage,
    /// The calls agents made, until the scheduler writes them down.
    pub activity: activity::Activity,
    /// The known apps' client documents, as last fetched.
    pub documents: oauth::Documents,
    /// Public OAuth requests admitted before they can consume database capacity.
    pub limits: oauth::limits::Limits,
    /// When calls were last written down for the Activity log.
    written: AtomicI64,
}
impl Kept {
    pub fn of(state: &State) -> &Self {
        state
            .agents
            .as_ref()
            .and_then(|held| held.downcast_ref::<Self>())
            .expect("the MCP made what it holds as the server started")
    }
}

fn routes() -> Vec<dispatch_core::server::http::Route> {
    let mut routes = agent_api::routes();
    routes.extend(oauth_routes::routes());
    routes
}

fn state(store: &Store) -> Result<Box<dyn Any + Send + Sync>> {
    Ok(Box::new(Kept {
        usage: Usage::default(),
        // Each key's calls recorded today, so a restart keeps its daily cap.
        activity: activity::Activity::seeded(store)?,
        documents: oauth::Documents::default(),
        limits: oauth::limits::Limits::default(),
        written: AtomicI64::new(0),
    }))
}

/// Each minute: when keys were last used, the OAuth records past use, and agents' calls past
/// 90 days, a step a minute, apart so a long backlog never holds the lock for the rest.
fn upkeep(state: Arc<State>, _due: bool) -> Upkeep {
    Box::pin(async move {
        let used = Kept::of(&state).usage.take();
        let keys: Vec<String> = used.iter().map(|(key, _)| key.clone()).collect();
        let result = state
            .run_bookkeeping(move |db| {
                if !used.is_empty() {
                    db.record_agent_use(&used)?;
                }
                db.prune_oauth()
            })
            .await;
        if let Err(error) = result {
            // Nothing was committed, so the keys' last use is written next time.
            Kept::of(&state).usage.unsaved(&keys);
            failed("agent_upkeep_failed", &error);
        }
        if let Err(error) = state.run_bookkeeping(|db| db.prune_agent_activity()).await {
            failed("agent_activity_prune_failed", &error);
        }
    })
}

/// Every second: agents' calls written down for the Activity log every five seconds, or
/// sooner once a batch is waiting, one write for many calls; and all of them as the server
/// stops, so none is lost to a restart.
fn tick(state: Arc<State>, stopping: bool) -> Upkeep {
    Box::pin(async move {
        let held = Kept::of(&state);
        let waiting = held.activity.pending();
        let recent = now() - held.written.load(Ordering::Relaxed) < activity::EVERY_MS;
        if waiting == 0 || (!stopping && waiting < activity::BATCH && recent) {
            return;
        }
        held.written.store(now(), Ordering::Relaxed);
        if let Err(error) = activity::flush(&state).await {
            failed("agent_activity_failed", &error);
        }
    })
}

fn failed(event: &str, error: &dispatch_core::Error) {
    observability::event("warn", event, json!({"error": error.code}));
}

/// An outside agent's key or a connected app's access token, never a browser session.
/// `Agent::READ` is any; a route that acts would ask for `AgentAccess::Operator`.
#[derive(Clone, Copy)]
pub struct Agent(AgentAccess);
impl Agent {
    pub const READ: Agent = Agent(AgentAccess::Read);
}
impl Grant for Agent {
    type Who = Caller;
    fn access(self) -> dispatch_core::server::http::Access {
        dispatch_core::server::http::Access::Agent(self.0.as_str())
    }
    fn authorize(self, db: &Store, input: &Input) -> Result<Caller> {
        // Agents are the platform owner's, and reach Dispatch only at the admin's address.
        ensure(input.site == Site::Admin, "not_found", 404)?;
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
        let client = client_label(input.header("user-agent"));
        // A connected app signs in with its OAuth access token, and only ever as a bearer.
        let app = bearer && token.starts_with("dsa_");
        let caller = if app {
            db.authenticate_app(token, &client)?
        } else {
            db.authenticate_agent(token, &client)?
        };
        {
            let mut trace = input
                .trace
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            trace.actor = Some(format!("agent:{}", caller.key));
            activity::noted(&mut trace).key = Some(AgentActivityKey {
                id: caller.key.clone(),
                name: caller.name.clone(),
                kind: if app {
                    AgentKeyKind::App
                } else {
                    AgentKeyKind::Key
                },
            });
        }
        ensure(
            self.0 == AgentAccess::Read || caller.access == AgentAccess::Operator,
            "agent_read_only",
            403,
        )?;
        Ok(caller)
    }
    fn admit(self, state: &State, who: &Caller) -> Result<()> {
        Kept::of(state).usage.admit(&who.key, &who.client)
    }
}

fn authorize(db: &Store, input: &Input) -> Result<()> {
    Agent::READ.authorize(db, input).map(|_| ())
}

/// Where and how to sign in. An MCP client finds Dispatch's authorization server from the
/// challenge's resource metadata. One that sent a token hears it was refused, and refreshes
/// it; one that sent none is not told of an error (RFC 6750 §3.1).
fn challenge(config: &Config, code: Option<&str>) -> String {
    let refused = code.is_some_and(|code| code != "agent_key_required");
    format!(
        "Bearer realm=\"Dispatch\", resource_metadata=\"{}\", scope=\"{}\"{}",
        oauth::resource_metadata(config),
        oauth::SCOPE,
        if refused {
            ", error=\"invalid_token\""
        } else {
            ""
        }
    )
}

/// The tool an MCP message calls, if any, for the Activity log, noted before the key is even
/// counted: a call refused for its rate is still that tool's.
fn called(trace: &RequestTrace, body: &[u8]) {
    if let Some(tool) = activity::tool_call(body) {
        let mut trace = trace.lock().unwrap_or_else(|poison| poison.into_inner());
        activity::noted(&mut trace).surface = Some(tool);
    }
}

fn answered(state: &State, noted: Option<Box<dyn Any + Send>>, answered: Answered<'_>) {
    let noted = noted
        .and_then(|noted| noted.downcast::<activity::Noted>().ok())
        .map(|noted| *noted)
        .unwrap_or_default();
    Kept::of(state).activity.finish(noted, answered);
}

/// A reset can follow a stolen password, so the keys and apps of its owner end with it.
fn password_reset(db: &Store, user: &str) -> Result<()> {
    db.revoke_agent_keys_within(Some(user), Some(user))
        .map(|_| ())
}

/// Agent keys end like sessions do, and an unspent OAuth code, a short-lived bearer
/// capability that can mint a new key, with them. A backup older than either has neither.
fn restored(db: &rusqlite::Connection) -> Result<()> {
    let exists = |table: &str| -> Result<bool> {
        Ok(db.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?)",
            [table],
            |row| row.get::<_, bool>(0),
        )?)
    };
    if exists("agent_keys")? {
        db.execute(
            "UPDATE agent_keys SET revoked_at=?1 WHERE revoked_at IS NULL",
            [iso()],
        )?;
    }
    if exists("oauth_codes")? {
        db.execute("DELETE FROM oauth_codes WHERE used_at IS NULL", [])?;
    }
    Ok(())
}

fn check() {
    tools::check(own().tools);
}
