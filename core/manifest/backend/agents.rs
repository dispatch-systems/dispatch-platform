//! The agents piece: what lets outside agents use Dispatch beside its people. One piece at
//! most, outside the features, declares it; core reaches it through the registry alone.
use crate::{
    Result, State,
    db::{Migrations, Store},
    foundation::{config::Config, observability::RequestTrace},
    manifest::{Audit, Maintenance, Registry, Tables, Upkeep},
    server::http::{Answered, Input, Route},
};
use std::{any::Any, sync::Arc};

/// What lets outside agents use Dispatch beside its people: their credentials, how they
/// sign in and what they call. One piece at most, outside the features, declares it; core
/// reaches it through these and nothing else. Its routes take `Access::Agent` under
/// `/api/v1/`, the agent API.
pub struct Agents {
    pub routes: fn() -> Vec<Route>,
    /// What it adds to the databases others number, numbered with theirs.
    pub migrations: &'static [Migrations],
    /// The tables it keeps.
    pub tables: Tables,
    /// How the activity log lists its actions.
    pub audit: Audit,
    /// Upkeep it needs the scheduler to run every minute.
    pub maintenance: &'static [Maintenance],
    /// What it keeps in memory for as long as the server runs, made as it starts.
    pub state: fn(&Store) -> Result<Box<dyn Any + Send + Sync>>,
    /// Runs every second, and once more as the server stops (`true`).
    pub tick: fn(Arc<State>, bool) -> Upkeep,
    /// Refuses a request to the agent API before any database turn, from what the request
    /// alone says, as each of its routes does.
    pub gate: fn(&State, &Input) -> Result<()>,
    /// Signs in a request to the agent API that no route serves, so it is refused as any of
    /// its routes would refuse it before it is told there is nothing there.
    pub authorize: fn(&Store, &Input) -> Result<()>,
    /// Told that `authorize` refused a request with this error.
    pub refused: fn(&State, &Input, &crate::Error),
    /// The `WWW-Authenticate` challenge an answer of the agent API refused for its
    /// credentials carries, given the code it was refused with.
    pub challenge: fn(&Config, Option<&str>) -> String,
    /// Notes in the request's trace what a message to one of its protocol routes calls, read
    /// from the body before the caller is signed in.
    pub called: fn(&RequestTrace, &[u8]),
    /// What an agent's request noted in its trace (`RequestContext::agent`), once its route
    /// answered.
    pub answered: fn(&State, Option<Box<dyn Any + Send>>, Answered<'_>),
    /// Ends what a person's credentials made, inside the transaction of a reset of their
    /// password, as their sessions end.
    pub password_reset: fn(&Store, &str) -> Result<()>,
    /// Ends, in a restored backup's platform database, what its credentials made, as its
    /// sessions end.
    pub restored: fn(&rusqlite::Connection) -> Result<()>,
    /// Panics unless what it declares of itself holds, as the registry is installed.
    pub check: fn(),
    /// What only the piece itself reads of its declaration.
    pub own: &'static (dyn Any + Send + Sync),
}

/// Panics unless the agents' piece's own declarations are as it asks.
pub(crate) fn check(registry: &Registry) {
    if let Some(agents) = registry.agents {
        (agents.check)();
    }
}
