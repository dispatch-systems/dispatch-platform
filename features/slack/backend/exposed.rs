//! What Slack exposes to every other feature and agent tool: its workspaces' members, as each
//! person's accounts there (`chat_accounts`, through `dispatch_slack_contract`).
use super::{people, storage::SlackStore};
use dispatch_core::{Result, db::Store, manifest::people::Source};
use dispatch_slack_contract::{ChatAccount, ChatAccounts};
use std::collections::HashMap;

/// Slack's workspaces' members, as `ChatAccounts` reads them.
pub(crate) struct Exposed;
pub(crate) static EXPOSED: Exposed = Exposed;

impl ChatAccounts for Exposed {
    fn source(&self) -> Source {
        people::SOURCE
    }
    fn accounts(&self, db: &Store, dsp: &str, ids: &[String]) -> Result<Vec<ChatAccount>> {
        // Each workspace the DSP has, by its name in Dispatch and whether its access broke: a
        // member of one it no longer has is nobody's account.
        let workspaces: HashMap<String, (String, bool)> = db
            .slack_workspaces(dsp)?
            .into_iter()
            .map(|workspace| {
                let name = workspace.label.unwrap_or(workspace.name);
                (workspace.team, (name, workspace.broken))
            })
            .collect();
        let mut found: HashMap<String, ChatAccount> = HashMap::new();
        for (team, member) in db.slack_members(dsp)? {
            let id = people::id(&team, &member);
            let Some((workspace, broken)) = workspaces.get(&team) else {
                continue;
            };
            if ids.contains(&id) {
                let account = ChatAccount {
                    id: id.clone(),
                    workspace: workspace.clone(),
                    user: member.user,
                    display_name: member.display_name,
                    active: member.active,
                    guest: member.guest,
                    broken: *broken,
                };
                found.insert(id, account);
            }
        }
        Ok(ids.iter().filter_map(|id| found.remove(id)).collect())
    }
}

#[cfg(test)]
#[path = "../tests/backend/exposed.rs"]
mod tests;
