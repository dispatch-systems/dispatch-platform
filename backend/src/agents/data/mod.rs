//! What agents read: the DSP's drivers and their days, assembled from every collection
//! and named the way a person asks for them. Each answer says what it understood (the DSP,
//! the days, the driver) and which days each source has, so missing data never reads as
//! zero. Requests that leave something unclear are refused with the choices, never guessed.
pub mod catalog;
mod facts;
mod scope;
mod views;

pub use views::*;

use crate::Error;
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
    fn body(self) -> (u16, Value) {
        let mut body = json!({"error":self.code,"message":self.message});
        if !self.choices.is_empty() {
            body["choices"] = json!(self.choices);
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
