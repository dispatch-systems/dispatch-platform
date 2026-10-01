//! Agent keys and the agent API: the platform owner's Agents page, everything an agent
//! reads under `/api/v1/`, and the MCP server at `/api/v1/mcp`. Making or widening a key asks for
//! recent verification; revoking never does.
use crate::{
    Result,
    agents::{data, mcp, skill},
    contracts::{AgentKeyRequest, AgentKeysRevoked},
    db::Store,
    http::{
        input::{Input, Reply},
        route::{
            Agent, PlatformOwner, PlatformRoutine, Route, Served, User, agent_protocol, read, write,
        },
    },
    validate as v,
};
use axum::{extract::Request, http::Method};

pub fn routes() -> Vec<Route> {
    let mut routes = vec![
        read("/api/platform/agents", PlatformOwner, keys),
        write("/api/platform/agents/keys", PlatformOwner, create),
        write("/api/platform/agents/keys/{id}", PlatformOwner, update),
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
    routes.extend(data::catalog::ENDPOINTS.iter().map(|endpoint| {
        read(endpoint.path, Agent::READ, move |db, agent, input| {
            let named = endpoint
                .path_params
                .first()
                .map(|param| decoded(input.param(param.name)))
                .unwrap_or_default();
            reply(data::ask(
                endpoint,
                db,
                agent.state,
                agent,
                &named,
                &input.query,
            ))
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
/// What an agent reads: the answer, or a refusal that says what to fix.
fn reply(answer: data::Answer) -> Result<Reply> {
    let (status, body) = data::settle(answer)?;
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
    Box::pin(mcp::serve(request))
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
mod tests {
    #[test]
    fn path_segments_are_decoded() {
        assert_eq!(super::decoded("Daniel%20Ortiz"), "Daniel Ortiz");
        assert_eq!(super::decoded("Jos%C3%A9"), "José");
        assert_eq!(super::decoded("100%"), "100%");
        assert_eq!(super::decoded("K4M7QZ"), "K4M7QZ");
    }
}
