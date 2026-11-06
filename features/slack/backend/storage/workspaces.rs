//! Workspace ownership across the platform registry, DSP rows and encrypted token files.
use crate::backend::{integrations, slack::Tokens};
use dispatch_core::{
    Result,
    db::{self, Db, FromRow, Row, Store, iso},
    ensure,
    foundation::crypto,
};
use rusqlite::params;
use serde_json::json;

/// A Slack workspace a DSP added Dispatch to, as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Workspace {
    pub team: String,
    pub name: String,
    pub broken: bool,
    /// What its admin let the bot do.
    pub scopes: Vec<String>,
    pub bot_user: String,
    pub app: String,
    /// The member who connected it; none when it came by the install link.
    pub connected_by: Option<String>,
    pub connected_at: String,
    /// Its name in Dispatch, if it was given one; else it's known by its name in Slack.
    pub label: Option<String>,
}
impl FromRow for Workspace {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            team: row.get("team_id")?,
            name: row.get("name")?,
            broken: row.get::<String>("status")? == "broken",
            scopes: row
                .get::<String>("scopes")?
                .split(' ')
                .filter(|scope| !scope.is_empty())
                .map(str::to_owned)
                .collect(),
            bot_user: row.get("bot_user_id")?,
            app: row.get("app_id")?,
            connected_by: row.get("connected_by")?,
            connected_at: row.get("connected_at")?,
            label: row.get("label")?,
        })
    }
}

/// Whether the DSP still has the workspace `team`: one disconnected while its members were
/// being read keeps none of them.
pub(super) fn kept_workspace(db: &Db, team: &str) -> Result<bool> {
    Ok(db
        .one("SELECT 1 FROM slack_workspaces WHERE team_id=?", [team])?
        .is_some())
}

/// Frees each workspace of a DSP being deleted for any DSP: the rest of what Slack keeps of them
/// is in the DSP's folder, which goes with it.
pub fn forget_dsp(db: &Store, dsp: &str) -> Result<()> {
    db.platform
        .exec("DELETE FROM slack_teams WHERE dsp_id=?", [dsp])?;
    Ok(())
}

/// The tables Slack declares in each DSP's database, which removing a workspace clears of it.
fn workspace_tables() -> impl Iterator<Item = &'static str> {
    crate::FEATURE
        .tables
        .iter()
        .filter(|(database, _)| *database == "dsp")
        .flat_map(|(_, tables)| tables.iter().copied())
}

pub trait WorkspaceStore {
    /// The DSP's workspaces, by name.
    fn slack_workspaces(&self, dsp: &str) -> Result<Vec<Workspace>>;
    fn slack_workspace(&self, dsp: &str, team: &str) -> Result<Option<Workspace>>;
    /// The DSP a workspace belongs to, if any does.
    fn slack_team_dsp(&self, team: &str) -> Result<Option<String>>;
    /// Keeps the workspace, which becomes the DSP's, and its tokens. One that another DSP has
    /// is refused, and nothing changes.
    fn save_slack_workspace(&self, dsp: &str, workspace: &Workspace, tokens: &Tokens)
    -> Result<()>;
    fn slack_tokens(&self, dsp: &str, team: &str) -> Result<Option<Tokens>>;
    /// Keeps the workspace's renewed tokens.
    fn save_slack_tokens(&self, dsp: &str, team: &str, tokens: &Tokens) -> Result<()>;
    /// Keeps the workspace's name, as Slack now has it.
    fn rename_slack_workspace(&self, dsp: &str, team: &str, name: &str) -> Result<()>;
    /// Gives the workspace its name in Dispatch, or with none, has it known by its name in
    /// Slack: the workspace, if the DSP has it.
    fn label_slack_workspace(
        &self,
        dsp: &str,
        team: &str,
        label: Option<&str>,
    ) -> Result<Option<Workspace>>;
    /// Marks the workspace broken, once: whether it was working until now.
    fn break_slack_workspace(&self, dsp: &str, team: &str) -> Result<bool>;
    /// Forgets the workspace, its members and its tokens, and frees it for any DSP: what was
    /// forgotten.
    fn remove_slack_workspace(&self, dsp: &str, team: &str) -> Result<Option<Workspace>>;
}

impl WorkspaceStore for Store {
    fn slack_workspaces(&self, dsp: &str) -> Result<Vec<Workspace>> {
        self.dsp(dsp)?.query_as(
            "SELECT * FROM slack_workspaces ORDER BY name COLLATE NOCASE,team_id",
            [],
        )
    }
    fn slack_workspace(&self, dsp: &str, team: &str) -> Result<Option<Workspace>> {
        self.dsp(dsp)?
            .one_as("SELECT * FROM slack_workspaces WHERE team_id=?", [team])
    }
    fn slack_team_dsp(&self, team: &str) -> Result<Option<String>> {
        let found: Option<(String,)> = self
            .platform
            .one_as("SELECT dsp_id FROM slack_teams WHERE team_id=?", [team])?;
        Ok(found.map(|(dsp,)| dsp))
    }
    fn save_slack_workspace(
        &self,
        dsp: &str,
        workspace: &Workspace,
        tokens: &Tokens,
    ) -> Result<()> {
        ensure(team_id(&workspace.team), "slack_install_failed", 502)?;
        // The workspace becomes the DSP's first, so no two DSPs ever both hold it.
        let held = self.slack_team_dsp(&workspace.team)?.is_some();
        let claimed = self.platform.exec(
            "INSERT INTO slack_teams (team_id,dsp_id,connected_at) VALUES (?1,?2,?3) \
             ON CONFLICT(team_id) DO UPDATE SET connected_at=excluded.connected_at \
             WHERE slack_teams.dsp_id=excluded.dsp_id",
            params![workspace.team, dsp, workspace.connected_at],
        )?;
        ensure(claimed > 0, "slack_workspace_taken", 409)?;
        let kept = self
            .save_slack_tokens(dsp, &workspace.team, tokens)
            .and_then(|()| {
                // Reconnected, it keeps when the DSP got it.
                self.dsp(dsp)?.exec(
                    "INSERT INTO slack_workspaces (team_id,name,status,scopes,bot_user_id,\
                     app_id,connected_by,connected_at,broken_at,label,added_at) \
                     VALUES (?1,?2,'connected',?3,?4,?5,?6,?7,NULL,?8,?7) \
                     ON CONFLICT(team_id) DO UPDATE SET name=excluded.name,\
                     status=excluded.status,scopes=excluded.scopes,\
                     bot_user_id=excluded.bot_user_id,app_id=excluded.app_id,\
                     connected_by=excluded.connected_by,connected_at=excluded.connected_at,\
                     broken_at=NULL,label=excluded.label",
                    params![
                        workspace.team,
                        workspace.name,
                        workspace.scopes.join(" "),
                        workspace.bot_user,
                        workspace.app,
                        workspace.connected_by,
                        workspace.connected_at,
                        workspace.label,
                    ],
                )
            });
        // A claim this made goes with what failed after it, so no workspace is held by a DSP
        // that doesn't list it.
        if kept.is_err() && !held {
            self.platform.exec(
                "DELETE FROM slack_teams WHERE team_id=? AND dsp_id=?",
                [&workspace.team, dsp],
            )?;
        }
        kept.map(|_| ())
    }
    fn slack_tokens(&self, dsp: &str, team: &str) -> Result<Option<Tokens>> {
        let area = self.area(dsp, "secrets")?;
        let path = area.join(secret(team)?);
        if !path.exists() {
            return Ok(None);
        }
        db::private_file(&path, false)?;
        let key = db::key_file(&area.join("vault.key"))?;
        let value = crypto::decrypt(&key, &binding(dsp, team), &std::fs::read_to_string(path)?)?;
        let text = |field: &str| value[field].as_str().map(str::to_owned);
        Ok(text("access").map(|access| Tokens {
            access,
            refresh: text("refresh"),
            expires_at: value["expiresAt"].as_i64(),
        }))
    }
    fn save_slack_tokens(&self, dsp: &str, team: &str, tokens: &Tokens) -> Result<()> {
        let area = self.area(dsp, "secrets")?;
        let key = db::key_file(&area.join("vault.key"))?;
        let value = json!({
            "access": tokens.access,
            "refresh": tokens.refresh,
            "expiresAt": tokens.expires_at,
        });
        db::write_private(
            &area.join(secret(team)?),
            crypto::encrypt(&key, &binding(dsp, team), &value)?.as_bytes(),
        )
    }
    fn rename_slack_workspace(&self, dsp: &str, team: &str, name: &str) -> Result<()> {
        self.dsp(dsp)?.exec(
            "UPDATE slack_workspaces SET name=? WHERE team_id=? AND name<>?",
            params![name, team, name],
        )?;
        Ok(())
    }
    fn label_slack_workspace(
        &self,
        dsp: &str,
        team: &str,
        label: Option<&str>,
    ) -> Result<Option<Workspace>> {
        self.dsp(dsp)?.exec(
            "UPDATE slack_workspaces SET label=? WHERE team_id=?",
            params![label, team],
        )?;
        self.slack_workspace(dsp, team)
    }
    fn break_slack_workspace(&self, dsp: &str, team: &str) -> Result<bool> {
        let changed = self.dsp(dsp)?.exec(
            "UPDATE slack_workspaces SET status='broken',broken_at=? \
             WHERE team_id=? AND status='connected'",
            params![iso(), team],
        )?;
        Ok(changed > 0)
    }
    fn remove_slack_workspace(&self, dsp: &str, team: &str) -> Result<Option<Workspace>> {
        let found = self.slack_workspace(dsp, team)?;
        // What each integration keeps of it in tables of its own feature's, which it clears
        // first: should one fail, Slack keeps the workspace and its tokens, and disconnecting
        // again tries again.
        for (_, integration) in integrations::declared() {
            if let Some(forget) = integration.forget {
                forget(self, dsp, team)?;
            }
        }
        let path = self.area(dsp, "secrets")?.join(secret(team)?);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            // Every table Slack declares in the DSP's database that keeps rows by workspace,
            // its own and its integrations': a new one's go with the workspace too, as does a
            // reconnect someone started for it (`slack_connect_requests`), which finishing
            // would answer as expired. The workspace itself goes last.
            for table in workspace_tables() {
                let keyed = db.count(
                    "SELECT count(*) FROM pragma_table_info(?) WHERE name='team_id'",
                    [table],
                )? > 0;
                if keyed && table != "slack_workspaces" {
                    db.exec(&format!("DELETE FROM {table} WHERE team_id=?"), [team])?;
                }
            }
            db.exec("DELETE FROM slack_workspaces WHERE team_id=?", [team])
        })?;
        drop(db);
        self.platform.exec(
            "DELETE FROM slack_teams WHERE team_id=? AND dsp_id=?",
            [team, dsp],
        )?;
        Ok(found)
    }
}

/// The file a workspace's tokens are kept in, among the DSP's secrets. Slack's workspace IDs
/// are capital letters and digits.
fn secret(team: &str) -> Result<String> {
    ensure(team_id(team), "invalid_input", 400)?;
    Ok(format!("slack-{team}.enc"))
}
/// Whether `team` is a Slack workspace ID as Slack writes them.
pub fn team_id(team: &str) -> bool {
    (2..=32).contains(&team.len())
        && team.starts_with('T')
        && team
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
/// What the tokens' encryption is bound to: this DSP's tokens for this workspace, in their
/// first format.
fn binding(dsp: &str, team: &str) -> String {
    format!("{dsp}:slack:{team}:1")
}
