//! Agent keys and the agent API: the platform owner's Agents page, and everything an
//! agent reads under `/api/v1/`. Making or widening a key asks for recent verification;
//! revoking never does.
use crate::{
    Result,
    agents::data,
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
        read("/api/v1/openapi.json", Agent::READ, openapi),
        read("/api/v1/status", Agent::READ, |db, a, i| {
            reply(data::status(db, a, &i.query))
        }),
        read("/api/v1/metrics", Agent::READ, |_, _, i| {
            reply(data::metrics(&i.query))
        }),
        read("/api/v1/drivers", Agent::READ, |db, a, i| {
            reply(data::drivers(db, a.state, a, &i.query))
        }),
        read("/api/v1/drivers/{driver}", Agent::READ, |db, a, i| {
            let driver = decoded(i.param("driver"));
            reply(data::driver(db, a.state, a, &driver, &i.query))
        }),
        read("/api/v1/team", Agent::READ, |db, a, i| {
            reply(data::team(db, a.state, a, &i.query))
        }),
        read("/api/v1/routes", Agent::READ, |db, a, i| {
            reply(data::routes(db, a.state, a, &i.query))
        }),
        read("/api/v1/routes/{itinerary}", Agent::READ, |db, a, i| {
            let itinerary = decoded(i.param("itinerary"));
            reply(data::route(db, a.state, a, &itinerary, &i.query))
        }),
        read("/api/v1/packages/{tracking}", Agent::READ, |db, a, i| {
            let tracking = decoded(i.param("tracking"));
            reply(data::package(db, a.state, a, &tracking, &i.query))
        }),
        read("/api/v1/timecards", Agent::READ, |db, a, i| {
            reply(data::timecards(db, a.state, a, &i.query))
        }),
        read("/api/v1/meal-breaks", Agent::READ, |db, a, i| {
            reply(data::meal_breaks(db, a.state, a, &i.query))
        }),
        read("/api/v1/dvic", Agent::READ, |db, a, i| {
            reply(data::dvic(db, a.state, a, &i.query))
        }),
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
/// What an agent reads: the answer, or a refusal that says what to fix.
fn reply(answer: data::Answer) -> Result<Reply> {
    let (status, body) = data::settle(answer)?;
    Ok(Reply::status(body, status))
}
fn openapi(db: &Store, _: &AgentCall, _: &Input) -> Result<Reply> {
    Ok(Reply::json(data::catalog::openapi(&db.config.origin)))
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
