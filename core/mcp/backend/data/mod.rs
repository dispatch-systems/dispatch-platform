//! What agents read: the DSP's drivers and their days, assembled from every collection
//! and named the way a person asks for them. Each answer says what it understood (the DSP,
//! the days, the driver) and which days each source has, so missing data never reads as
//! zero. Requests that leave something unclear are refused with the choices, never guessed.
pub mod access;
pub mod catalog;
pub mod facts;
pub mod schema;
pub mod scope;
mod selected;
pub mod shape;
mod views;

pub use access::switched_on;
pub use shape::BUDGET;
pub use views::*;

use crate::{
    Error, State,
    db::Store,
    mcp::{Caller, api::types::AgentDsp},
};
use access::{Access, Read};
use serde_json::{Value, json};

/// A request an agent can fix: what was unclear, in words it can repeat, and what it could
/// have meant.
#[derive(Debug)]
pub struct Refusal {
    status: u16,
    pub code: &'static str,
    message: String,
    pub choices: Vec<String>,
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
    let endpoint = catalog::select(endpoint, query)?;
    let read = match endpoint.area {
        Some(area) => Access::of(db, caller, query)?.check(area)?,
        None => Read::On,
    };
    let mut answer = (endpoint.answer)(db, state, caller, named, query)?;
    if catalog::has_variants(endpoint.id)
        && let Some(area) = endpoint.area
    {
        answer["source"] = json!(area.source().as_str());
    }
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
