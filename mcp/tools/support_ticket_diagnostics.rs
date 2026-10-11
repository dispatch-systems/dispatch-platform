//! An authorized ticket's bounded operational health, without employee records or secrets.
use crate::toolbox::{Answer, Cx, Reply, Scope, Tool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Input {
    /// The AI Support ticket ID; the ticket must belong to this call's DSP.
    pub ticket_id: String,
}

#[derive(Serialize, JsonSchema)]
pub struct Diagnostics {
    pub ticket_id: String,
    /// Ticket-derived runtime and bounded provider/job health; excludes reports and employee records.
    pub diagnostics: Value,
}

pub struct SupportTicketDiagnostics;
impl Tool for SupportTicketDiagnostics {
    const NAME: &'static str = "support_ticket_diagnostics";
    const TITLE: &'static str = "AI Support ticket diagnostics";
    const DESCRIPTION: &'static str =
        "Read an authorized AI Support ticket's runtime and recent provider/job health at its DSP.";
    const SCOPE: Scope = Scope::Dsp;
    type Input = Input;
    type Output = Diagnostics;
    async fn call(cx: Cx, input: Input) -> Answer<Reply<Diagnostics>> {
        let dsp = cx.dsp().id.clone();
        let actor = cx.caller().user.clone();
        cx.read(move |db| {
            let diagnostics =
                dispatch_core::support::diagnostic_dsp(db, &input.ticket_id, &dsp, &actor)?;
            Ok(Diagnostics {
                ticket_id: input.ticket_id,
                diagnostics,
            }
            .into())
        })
        .await
    }
}

#[cfg(test)]
#[path = "../tests/backend/tools/support_ticket_diagnostics.rs"]
mod tests;
