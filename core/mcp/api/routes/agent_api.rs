//! Agent keys and the agent API: the platform owner's Agents page, everything an agent
//! reads under `/api/v1/`, and the MCP server at `/api/v1/mcp`. Nothing on the Agents page asks
//! for recent verification: a key only ever reads what the owner allows, and stops at once
//! when revoked.
use crate::{
    Result,
    db::Store,
    ensure,
    foundation::{observability::RequestTrace, validate as v},
    mcp::{
        activity::{self, ActivityQuery, Outcomes},
        api::types::{AgentDsp, AgentKeyRequest, AgentKeysRevoked},
        data, server, skill,
    },
    server::http::{
        input::{Input, Reply, optional_text, query_number},
        route::{
            Agent, PlatformOwner, PlatformRoutine, Route, Served, User, agent_protocol, read, write,
        },
    },
};
use axum::{extract::Request, http::Method};

pub fn routes() -> Vec<Route> {
    let mut routes = vec![
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
        read("/api/platform/agents/skill", PlatformOwner, |db, _, _| {
            Ok(skill_file(db))
        }),
        read(
            "/api/platform/agents/openapi.json",
            PlatformOwner,
            |db, _, _| Ok(openapi(db)),
        ),
        read("/api/v1/openapi.json", Agent::READ, |db, _, _| {
            Ok(openapi(db))
        }),
        read("/api/v1/skill", Agent::READ, |db, _, _| Ok(skill_file(db))),
        agent_protocol(Method::POST, "/api/v1/mcp", Agent::READ, mcp),
        agent_protocol(Method::GET, "/api/v1/mcp", Agent::READ, mcp),
    ];
    // Every endpoint of the catalog, answered exactly as its MCP tool answers.
    let canonical = data::catalog::ENDPOINTS
        .iter()
        .map(|endpoint| (endpoint.path, *endpoint));
    let legacy = crate::manifest::registry()
        .features
        .iter()
        .flat_map(|feature| feature.mcp.legacy_paths)
        .map(|(path, id)| (*path, data::catalog::endpoint(id)));
    routes.extend(canonical.chain(legacy).map(|(path, endpoint)| {
        read(path, Agent::READ, move |db, agent, input| {
            let named = endpoint
                .path_params
                .first()
                .map(|param| decoded(input.param(param.name)))
                .unwrap_or_default();
            let dsp = data::about(endpoint, agent, &input.query);
            reply(
                &input.trace,
                dsp,
                data::ask(endpoint, db, agent.state, agent, &named, &input.query),
            )
        })
    }));
    routes
}

fn keys(db: &Store, owner: &User, _: &Input) -> Result<Reply> {
    Reply::of(&db.agent_keys(&owner.state.agents.last())?)
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
/// What an agent reads: the answer, or a refusal that says what to fix. The Activity log
/// notes the DSP it was about, how it ended, and whether it bypassed features.
fn reply(trace: &RequestTrace, dsp: Option<AgentDsp>, answer: data::Answer) -> Result<Reply> {
    let settled = data::settle(answer);
    let (outcome, bypassed) = match &settled {
        Ok((200, body)) => ("ok", data::bypassed(body)),
        Ok((_, body)) => (body["error"].as_str().unwrap_or("refused"), false),
        Err(error) => (error.code.as_str(), false),
    };
    activity::note(trace, dsp, outcome, bypassed);
    let (status, body) = settled?;
    Ok(Reply::status(body, status))
}
fn openapi(db: &Store) -> Reply {
    Reply::json(data::catalog::openapi(&db.config.origin))
}
fn skill_file(db: &Store) -> Reply {
    Reply::file(
        "text/markdown; charset=utf-8",
        "SKILL.md",
        skill::skill(&db.config.origin),
    )
}
fn mcp(request: Request) -> Served {
    Box::pin(server::serve(request))
}
/// A path segment as the agent meant it: percent-escapes decoded, as a name with a space
/// arrives as `Daniel%20Ortiz`.
fn decoded(segment: &str) -> String {
    let bytes = segment.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let hex = |at: usize| bytes.get(at).and_then(|b| (*b as char).to_digit(16));
        match (bytes[index], hex(index + 1), hex(index + 2)) {
            (b'%', Some(high), Some(low)) => {
                out.push((high * 16 + low) as u8);
                index += 3;
            }
            (byte, ..) => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
#[path = "../../tests/backend/api/routes/agent_api.rs"]
mod tests;
