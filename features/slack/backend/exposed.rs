//! What Slack exposes to every other feature and agent tool: `chat_accounts`, through its
//! contract (`dispatch_slack_contract`).
use dispatch_core::{Result, db::Store};
use dispatch_slack_contract::ChatAccounts;

/// Slack's Chat Accounts, as its contract says.
pub(crate) struct Exposed;
pub(crate) static EXPOSED: Exposed = Exposed;

impl ChatAccounts for Exposed {
    fn ready(&self, _: &Store, _: &str) -> Result<bool> {
        // TODO(scaffold): answer from what it keeps.
        Ok(true)
    }
}

#[cfg(test)]
#[path = "../tests/backend/exposed.rs"]
mod tests;
