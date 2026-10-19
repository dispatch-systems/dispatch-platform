//! What Slack exposes to every other feature and agent tool: its workspaces' members, as each
//! person's accounts there (`chat_accounts`, through `dispatch_slack_contract`).
use super::{people, storage::SlackStore};
use dispatch_core::{Result, db::Store, ensure, manifest::people::Source};
use dispatch_slack_contract::{ChatAccount, ChatAccounts};
use std::collections::HashMap;

/// The most IDs one question about accounts names.
const MOST_ASKED: usize = 32;

/// Slack's workspaces' members, as `ChatAccounts` reads them.
pub(crate) struct Exposed;
pub(crate) static EXPOSED: Exposed = Exposed;

impl ChatAccounts for Exposed {
    fn source(&self) -> Source {
        people::SOURCE
    }
    fn accounts(&self, db: &Store, dsp: &str, ids: &[String]) -> Result<Vec<ChatAccount>> {
        ensure(
            ids.len() <= MOST_ASKED
                && ids
                    .iter()
                    .all(|id| !id.is_empty() && id.chars().count() <= 128),
            "invalid_input",
            400,
        )?;
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        // Each workspace by its name in Dispatch and whether its access broke. A workspace's
        // members go with it, so every member kept is in one.
        let workspaces: HashMap<String, (String, bool)> = db
            .slack_workspaces(dsp)?
            .into_iter()
            .map(|workspace| {
                let name = workspace.label.unwrap_or(workspace.name);
                (workspace.team, (name, workspace.broken))
            })
            .collect();
        let mut found = HashMap::new();
        for (team, member) in db.slack_members(dsp)? {
            let id = people::id(&team, &member);
            if !ids.contains(&id) {
                continue;
            }
            let Some((workspace, broken)) = workspaces.get(&team) else {
                continue;
            };
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
        Ok(ids.iter().filter_map(|id| found.remove(id)).collect())
    }
}

#[cfg(test)]
#[path = "../tests/backend/exposed.rs"]
mod tests;
