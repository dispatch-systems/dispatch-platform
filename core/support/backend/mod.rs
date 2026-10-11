//! Global AI Support. This module alone owns its SQL, authorization and durable transitions.
#[path = "../api/mod.rs"]
pub mod api;
pub mod approvals;
pub mod attachments;
pub mod browser;
pub mod conversations;
pub mod diagnostics;
pub mod github_events;
mod intake;
pub mod jobs;
pub mod notifications;
pub mod proposals;
pub mod tickets;
pub mod verification;

pub use diagnostics::diagnostic_dsp;

use crate::{Error, Result, accounts::Auth, db::Store, ensure};
use serde::de::DeserializeOwned;

pub(crate) fn decode<T: DeserializeOwned>(text: &str) -> Result<T> {
    Ok(serde_json::from_str(text)?)
}

/// Directory and user together identify a reporter. A DSP id supplied by a client grants nothing.
pub(crate) fn authorize(db: &Store, auth: &Auth, id: &str) -> Result<()> {
    let row = tickets::record(db, id)?;
    ensure(
        auth.user.platform_owner
            || (row["reporter_id"] == auth.user.id
                && row["reporter_scope"] == auth.scope.as_deref().unwrap_or("")),
        "support_ticket_not_found",
        404,
    )
}

pub(crate) fn found<T>(value: Option<T>) -> Result<T> {
    value.ok_or_else(|| Error::new("support_ticket_not_found", 404))
}

#[cfg(test)]
#[path = "../tests/backend/workflow.rs"]
mod tests;
