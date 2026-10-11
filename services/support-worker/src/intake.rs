//! Typed routing decisions separate conversation from source and browser authority.
use crate::{Result, require};
use dispatch_core::support::api::types::SupportProposal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Route {
    Acknowledge,
    Answer,
    Source,
    Browser,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Decision {
    pub route: Route,
    pub reason: String,
    pub reply: String,
}

impl Decision {
    pub fn validate(&self) -> Result<()> {
        require(
            !self.reason.trim().is_empty(),
            "Intake routing needs a reason",
        )?;
        if matches!(self.route, Route::Acknowledge | Route::Answer) {
            require(!self.reply.trim().is_empty(), "Intake reply is missing")?;
        }
        Ok(())
    }

    pub fn completion(&self) -> Option<Value> {
        let outcome = match self.route {
            Route::Acknowledge => "acknowledged",
            Route::Answer => "answered",
            Route::Source | Route::Browser => return None,
        };
        Some(
            json!({"outcome":outcome,"reply":self.reply,"reason":self.reason,
            "findings":[],"questions":[],"investigationAttempted":false}),
        )
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NextStep {
    Finish,
    Browser,
    Isolated,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Acknowledged,
    Answered,
    UnclearBug,
    UnclearFeature,
    Proposal,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Analysis {
    pub next_step: NextStep,
    pub outcome: Option<Outcome>,
    pub reason: String,
    pub reply: String,
    pub findings: Vec<String>,
    pub questions: Vec<String>,
    pub proposal: Option<SupportProposal>,
}

impl Analysis {
    pub fn validate(&self, ticket_kind: &str, attempted: bool, usable: bool) -> Result<()> {
        require(
            !self.reason.trim().is_empty(),
            "Analysis needs a routing reason",
        )?;
        if self.next_step != NextStep::Finish {
            require(
                self.outcome.is_none() && self.proposal.is_none(),
                "A continuation cannot also publish a final outcome",
            )?;
            return Ok(());
        }
        let outcome = self.outcome.ok_or("Analysis has no final outcome")?;
        require(!self.reply.trim().is_empty(), "Analysis reply is missing")?;
        if matches!(outcome, Outcome::UnclearBug | Outcome::UnclearFeature) {
            require(
                attempted
                    && usable
                    && !self.findings.is_empty()
                    && !self.questions.is_empty()
                    && self.findings.iter().all(|value| !value.trim().is_empty())
                    && self.questions.iter().all(|value| !value.trim().is_empty())
                    && matches!(
                        (outcome, ticket_kind),
                        (Outcome::UnclearBug, "bug") | (Outcome::UnclearFeature, "feature")
                    ),
                "Unclear intake requires a matching report, usable investigation and useful questions",
            )?;
        }
        require(
            (outcome == Outcome::Proposal) == self.proposal.is_some(),
            "Only a proposal outcome can contain a proposal",
        )?;
        if outcome == Outcome::Proposal {
            require(
                attempted && usable,
                "A proposal requires usable source or browser investigation",
            )?;
        }
        Ok(())
    }

    pub fn completion(&self, attempted: bool) -> Value {
        let mut reply = self.reply.clone();
        if matches!(
            self.outcome,
            Some(Outcome::UnclearBug | Outcome::UnclearFeature)
        ) {
            for question in &self.questions {
                let question = question.trim();
                if !reply.contains(question) {
                    reply.push_str("\n\n");
                    reply.push_str(question);
                }
            }
        }
        json!({"outcome":self.outcome,"reply":reply,"reason":self.reason,
            "findings":self.findings,"questions":self.questions,"investigationAttempted":attempted})
    }
}

pub const ROUTING: &str = "You are the tool-free AI Support intake stage. Decide from the complete report, latest conversation, existing proposals and prior evidence which next action is useful. Return ONLY {route:'acknowledge'|'answer'|'source'|'browser',reason:string,reply:string}. Greetings, thanks and genuine test submissions without an issue receive a brief acknowledgement; never invent a proposal for them. A report saying 'I tested routes and it crashed' is a real bug, not a test submission. One-off answers that are already supported by supplied context use answer. Source/code/how-it-works questions use source and need no browser by default. UI behavior, screenshots and vague real bugs/features use browser when observing the UI could clarify intent; use source when code is the useful evidence. Real but vague requests must get a best-effort investigation, never be dismissed by keywords. For owner question jobs, answer the identified question from prior evidence when possible; grant source/browser tools only if new evidence is needed. Do not return unclear here. You have no tools, source directory, browser grant, command permission or isolated preview. Public replies must not contain owner-private messages, evidence, provider details or secrets.";

pub const ANALYSIS: &str = "Use only the evidence and tools granted in this stage. Return ONLY {nextStep:'finish'|'browser'|'isolated',outcome:null|'acknowledged'|'answered'|'unclear_bug'|'unclear_feature'|'proposal',reason:string,reply:string,findings:string[],questions:string[],proposal:null|<full proposal contract>}. Source inspection and prior evidence can answer questions or support proposals without a browser. Request browser only when new live UI observations are useful. Request isolated only when a specific mutation/reproduction is necessary to understand the problem or plan; name that operation and why readonly source/live UI are insufficient. Do not request a baseline merely because a report exists. A continuation has outcome=null and proposal=null. Finish with answered for a one-off answer, or proposal when the intended issue/outcome is understood, even if a bug cannot be reproduced. Use unclear_bug/unclear_feature ONLY when the intended issue/outcome remains unidentified after actual best-effort source/browser investigation, with specific findings and clarification questions. Every question must be reporter-safe and concern their reported behavior or intended outcome; never include owner-private discussion, infrastructure/provider details or secrets. Include those questions in the public reply; findings and reasoning remain owner-only. Unavailable infrastructure, denied access and failure to reproduce an otherwise identified issue are uncertainties, not unclear outcomes or passes. Never invent browser observations. For owner question jobs, reply answers the private identified question; add a revised proposal only when it materially changes the plan/criteria and no published PR exists. Once a PR exists, answer only and describe extra scope as a follow-up. Keep public replies free of owner-private discussion and evidence.";
