//! Agent keys and the agent API: the platform owner's Agents page, `whoami` under `/api/v1/`,
//! and the MCP server at `/api/v1/mcp`. Nothing on the Agents page asks
//! for recent verification: a key only ever reads what the owner allows, and stops at once
//! when revoked.
use crate::{
    Result,
    db::Store,
    ensure,
    foundation::validate as v,
    mcp::{
        activity::{self, ActivityQuery, Outcomes},
        api::types::{AgentKeyRequest, AgentKeysRevoked},
        piece::{Agent, Kept},
        server,
    },
    server::http::{
        input::{Input, Reply, optional_text, query_number},
        route::{PlatformOwner, PlatformRoutine, Route, Served, User, agent_protocol, read, write},
    },
};
use axum::{extract::Request, http::Method};

pub fn routes() -> Vec<Route> {
    vec![
        read("/api/platform/agents", PlatformOwner, keys),
        write("/api/platform/agents/keys", PlatformRoutine, create),
        write("/api/platform/agents/keys/{id}", PlatformRoutine, update),
        write(
            "/api/platform/agents/keys/{id}/revoke",
            PlatformRoutine,
            revoke,
        ),
        write(
            "/api/platform/agents/revoke-all",
            PlatformRoutine,
            revoke_all,
        ),
        read(
            "/api/platform/agents/activity",
            PlatformRoutine,
            |db, _, input| Reply::of(&db.agent_activity(&activity_query(&input.query)?)?),
        ),
        read("/api/v1/whoami", Agent::READ, |db, agent, _| {
            Reply::of(&db.agent_whoami(agent)?)
        }),
        agent_protocol(Method::POST, "/api/v1/mcp", Agent::READ, mcp),
        agent_protocol(Method::GET, "/api/v1/mcp", Agent::READ, mcp),
    ]
}

fn keys(db: &Store, owner: &User, _: &Input) -> Result<Reply> {
    Reply::of(&db.agent_keys(&Kept::of(&owner.state).usage.last())?)
}
fn create(db: &Store, owner: &User, input: &Input) -> Result<Reply> {
    let request = AgentKeyRequest::parse(&input.body)?;
    Reply::of(&db.create_agent_key(owner.actor(), &request)?)
}
fn update(db: &Store, owner: &User, input: &Input) -> Result<Reply> {
    let request = AgentKeyRequest::parse(&input.body)?;
    Reply::of(&db.update_agent_key(owner.actor(), input.param("id"), &request)?)
}
fn revoke(db: &Store, owner: &User, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &[])?;
    Reply::of(&db.revoke_agent_key(owner.actor(), input.param("id"))?)
}
fn revoke_all(db: &Store, owner: &User, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &[])?;
    Reply::of(&AgentKeysRevoked {
        revoked: db.revoke_agent_keys(Some(owner.actor()), None)?,
    })
}
/// What `GET /api/platform/agents/activity` asks for: `key`, `outcome` (`ok` or `refused`),
/// `before` (a page's `next`) and `limit`, 1 to 200.
fn activity_query(q: &serde_json::Value) -> Result<ActivityQuery> {
    v::fields(q, &["key", "outcome", "before", "limit"])?;
    let key = optional_text(q, "key", 100)?;
    let outcomes = match optional_text(q, "outcome", 10)? {
        "" => Outcomes::All,
        "ok" => Outcomes::Ok,
        "refused" => Outcomes::Refused,
        _ => return Err(crate::Error::new("invalid_input", 400)),
    };
    let before = optional_text(q, "before", 40)?;
    let cursor = activity::cursor(before);
    ensure(before.is_empty() || cursor.is_some(), "invalid_input", 400)?;
    Ok(ActivityQuery {
        key: (!key.is_empty()).then(|| key.to_owned()),
        outcomes,
        before: cursor,
        limit: query_number(q, "limit", 50, 1, 200)?,
    })
}
fn mcp(request: Request) -> Served {
    Box::pin(server::serve(request))
}
