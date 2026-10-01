//! Agent keys: the platform owner's Agents page, and what an agent asks about itself.
//! Making or widening a key asks for recent verification; revoking never does.
use crate::{
    Result,
    contracts::{AgentKeyRequest, AgentKeysRevoked},
    db::Store,
    http::{
        input::{Input, Reply},
        route::{Agent, AgentCall, PlatformOwner, PlatformRoutine, Route, User, read, write},
    },
    validate as v,
};

pub fn routes() -> Vec<Route> {
    vec![
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
        read("/api/v1/whoami", Agent::READ, whoami),
    ]
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
fn whoami(db: &Store, agent: &AgentCall, _: &Input) -> Result<Reply> {
    Reply::of(&db.agent_whoami(agent)?)
}
