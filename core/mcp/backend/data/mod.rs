//! What agents read: the DSP's drivers and their days, assembled from every collection
//! and named the way a person asks for them. Each answer says what it understood (the DSP,
//! the days, the driver) and which days each source has, so missing data never reads as
//! zero. Requests that leave something unclear are refused with the choices, never guessed.
#[path = "access.rs"]
mod access;
#[path = "catalog.rs"]
pub mod catalog;
#[path = "facts.rs"]
mod facts;
#[path = "scope.rs"]
mod scope;
#[path = "../../../../features/scorecard/mcp/scorecard.rs"]
mod scorecard;
#[path = "shape.rs"]
mod shape;
#[path = "views.rs"]
mod views;

pub use access::switched_on;
pub use scorecard::{feedback, returns, safety, weekly};
pub use shape::BUDGET;
pub use views::*;

use crate::{Error, State, agents::Caller, contracts::AgentDsp, db::Store};
use access::{Access, Read};
use serde_json::{Value, json};

/// A request an agent can fix: what was unclear, in words it can repeat, and what it could
/// have meant.
#[derive(Debug)]
pub struct Refusal {
    status: u16,
    code: &'static str,
    message: String,
    choices: Vec<String>,
}
impl Refusal {
    pub fn new(status: u16, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            choices: vec![],
        }
    }
    pub fn choices(mut self, choices: Vec<String>) -> Self {
        self.choices = choices;
        self
    }
    pub fn body(self) -> (u16, Value) {
        let mut body = json!({"error":self.code,"message":self.message});
        if !self.choices.is_empty() {
            body["choices"] = json!(self.choices);
        }
        if let Err(refusal) = shape::check_budget(&body) {
            return (
                refusal.status,
                json!({"error":refusal.code,"message":refusal.message}),
            );
        }
        (self.status, body)
    }
}

/// Why an answer could not be given: something the agent can fix, or a failure of ours.
#[derive(Debug)]
pub enum Failure {
    Refused(Refusal),
    Failed(Error),
}
impl From<Refusal> for Failure {
    fn from(refusal: Refusal) -> Self {
        Self::Refused(refusal)
    }
}
impl From<Error> for Failure {
    fn from(error: Error) -> Self {
        Self::Failed(error)
    }
}
pub type Answer = std::result::Result<Value, Failure>;

/// The status and body to answer with: the answer, or the refusal as the agent should
/// read it. A failure of ours stays an error.
pub fn settle(answer: Answer) -> crate::Result<(u16, Value)> {
    match answer {
        Ok(value) => Ok((200, value)),
        Err(Failure::Refused(refusal)) => Ok(refusal.body()),
        Err(Failure::Failed(error)) => Err(error),
    }
}

/// The DSP a request is about, as its answer reads it: the one named with `dsp`, or the key's
/// only one. None for an endpoint about no one DSP, or a request that leaves it unclear.
pub fn about(endpoint: &catalog::Endpoint, caller: &Caller, query: &Value) -> Option<AgentDsp> {
    if !endpoint.params.iter().any(|param| param.name == "dsp") {
        return None;
    }
    let dsp = scope::pick_dsp(caller, scope::param(query, "dsp")).ok()?;
    Some(AgentDsp {
        id: dsp.id.clone(),
        name: dsp.name.clone(),
    })
}

/// Answers one endpoint of the catalog, as its REST route and its MCP tool both ask it.
/// `named` fills the endpoint's path parameter, when it has one.
pub fn ask(
    endpoint: &catalog::Endpoint,
    db: &Store,
    state: &State,
    caller: &Caller,
    named: &str,
    query: &Value,
) -> Answer {
    // A tool that reads one kind of data answers only where the key or app is allowed it and
    // the DSP has its feature on, or it bypasses features: the one place every such tool is
    // gated, so none can forget. Those that read several gate each in their answer.
    let read = match endpoint.area {
        Some(area) => Access::of(db, caller, query)?.check(area)?,
        None => Read::On,
    };
    let answer: Answer = match endpoint.id {
        "whoami" => {
            catalog::check("whoami", query)?;
            Ok(json!(db.agent_whoami(caller)?))
        }
        "status" => status(db, caller, query),
        "metrics" => metrics(query),
        "drivers" => drivers(db, state, caller, query),
        "driver" => driver(db, state, caller, named, query),
        "team" => team(db, state, caller, query),
        "routes" => routes(db, state, caller, query),
        "route" => route(db, state, caller, named, query),
        "package" => package(db, state, caller, named, query),
        "packages" => packages(db, state, caller, query),
        "feedback" => scorecard::feedback(db, state, caller, query),
        "safety" => scorecard::safety(db, state, caller, query),
        "returns" => scorecard::returns(db, state, caller, query),
        "scorecard" => scorecard::weekly(db, state, caller, query),
        "timecards" => timecards(db, state, caller, query),
        "meal_breaks" => meal_breaks(db, state, caller, query),
        "dvic" => dvic(db, state, caller, query),
        other => Err(Error::new(format!("unanswered_endpoint_{other}"), 500).into()),
    };
    let mut answer = answer?;
    if let (Some(area), Read::Bypassed) = (endpoint.area, read) {
        access::bypassed(&mut answer, area.source());
    }
    shape::check_budget(&answer)?;
    Ok(answer)
}

/// Whether an answer read a feature the DSP has switched off, by bypassing features: it names
/// them under `bypassed`, and the Activity log marks the call.
pub fn bypassed(answer: &Value) -> bool {
    answer.get("bypassed").is_some()
}
