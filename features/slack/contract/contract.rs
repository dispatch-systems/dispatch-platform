//! The DSP's chat accounts, as any feature or agent tool reads them without depending on Slack:
//! each person's accounts in the DSP's Slack workspaces, with the workspace each is in, the name
//! they go by there and whether it's active. What the DSP's people already carry of them, as
//! their names, email addresses and phone numbers, isn't repeated. Slack exposes it as
//! `chat_accounts`.
use dispatch_core::{Result, db::Store, manifest::people::Source};

/// Reads a DSP's chat accounts, as Slack keeps its workspaces' members.
pub trait ChatAccounts: Send + Sync {
    /// The source whose IDs name the accounts, to find them among the DSP's people.
    fn source(&self) -> Source;
    /// The accounts `ids` name, in their order. An ID of no account the DSP keeps, as one of a
    /// workspace it no longer has, is left out.
    fn accounts(&self, db: &Store, dsp: &str, ids: &[String]) -> Result<Vec<ChatAccount>>;
}

/// One account in one of the DSP's workspaces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatAccount {
    /// Its ID in `source`.
    pub id: String,
    /// The workspace it's in, by its name in Dispatch, else its name in Slack.
    pub workspace: String,
    /// The member's own ID in the workspace, `U…`, as a mention (`<@U…>`) names them.
    pub user: String,
    /// The name they go by in Slack, when it isn't the name on their profile.
    pub display_name: Option<String>,
    /// Whether the account is active: one Slack deactivated is kept, inactive.
    pub active: bool,
    /// Whether they're a guest in the workspace.
    pub guest: bool,
    /// Whether the workspace's access broke: the rest is as Slack last told it, until it's
    /// reconnected.
    pub broken: bool,
}

/// What reads the DSP's chat accounts: only while Slack is on there.
pub fn for_dsp(db: &Store, dsp: &str) -> Result<Option<&'static dyn ChatAccounts>> {
    dispatch_core::manifest::exposes::for_dsp::<dyn ChatAccounts>(db, dsp)
}
