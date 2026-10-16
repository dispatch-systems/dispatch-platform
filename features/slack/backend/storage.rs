//! Slack's storage: in each DSP's own database, its workspaces, the installs started for them
//! and its install link; each workspace's tokens, encrypted in the DSP's secrets; and in the
//! platform's database, which DSP each workspace belongs to. Only this crate writes SQL against
//! its tables.
use super::slack::Tokens;
use dispatch_core::{
    Error, Result,
    db::{self, FromRow, Row, Store, iso, now},
    ensure,
    foundation::crypto,
};
use rusqlite::params;
use serde_json::json;

/// How long an install started at Slack may take to come back.
const INSTALL_MS: i64 = 10 * 60 * 1000;
/// How long an install link lasts.
pub const LINK_MS: i64 = 7 * 24 * 60 * 60 * 1000;

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

/// Who an install was started by: a member, or the install link, by its hash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Starter {
    Member(String),
    Link(String),
}

/// An install someone started: who started it, the workspace it named to Slack, to reconnect
/// or update, none for a new one, and the name the workspace it adds is to have.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Install {
    pub starter: Starter,
    pub team: Option<String>,
    pub label: Option<String>,
}
/// An install as stored until Slack sends it back, with when it expires.
struct Requested {
    install: Option<Install>,
    expires_at: i64,
}
impl FromRow for Requested {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        let user: Option<String> = row.get("user_id")?;
        let link: Option<String> = row.get("link_hash")?;
        let starter = match (user, link) {
            (Some(user), None) => Some(Starter::Member(user)),
            (None, Some(link)) => Some(Starter::Link(link)),
            _ => None,
        };
        Ok(Self {
            install: match starter {
                Some(starter) => Some(Install {
                    starter,
                    team: row.get("team_id")?,
                    label: row.get("label")?,
                }),
                None => None,
            },
            expires_at: row.get("expires_at")?,
        })
    }
}

/// The DSP's install link, as stored: never the link itself, only its hash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub created_by: String,
    pub created_at: String,
    pub expires_at: i64,
    /// The name the workspace it adds is to have.
    pub label: Option<String>,
}

/// What Slack reads and writes.
pub trait SlackStore {
    /// The DSP's workspaces, by name.
    fn slack_workspaces(&self, dsp: &str) -> Result<Vec<Workspace>>;
    fn slack_workspace(&self, dsp: &str, team: &str) -> Result<Option<Workspace>>;
    /// The DSP a workspace belongs to, if any does.
    fn slack_team_dsp(&self, team: &str) -> Result<Option<String>>;
    /// Records an install someone started, by the hash of the state Slack carries back.
    fn start_slack_install(&self, dsp: &str, state: &str, install: &Install) -> Result<()>;
    /// The install `state` names, used up: each comes back once, before it expires.
    fn finish_slack_install(&self, dsp: &str, state: &str) -> Result<Install>;
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
    /// Forgets the workspace and its tokens, and frees it for any DSP: what was forgotten.
    fn remove_slack_workspace(&self, dsp: &str, team: &str) -> Result<Option<Workspace>>;
    fn slack_link(&self, dsp: &str) -> Result<Option<Link>>;
    /// Keeps a new install link for the DSP, by its hash, in place of any it had, with the
    /// name the workspace it adds is to have.
    fn save_slack_link(
        &self,
        dsp: &str,
        hash: &str,
        user: &str,
        label: Option<&str>,
    ) -> Result<Link>;
    /// Whether the link with this hash is the DSP's, and still works.
    fn slack_link_works(&self, dsp: &str, hash: &str) -> Result<bool>;
    /// Ends the DSP's install link, or the one with `hash` only: whether there was one.
    fn end_slack_link(&self, dsp: &str, hash: Option<&str>) -> Result<bool>;
}
impl SlackStore for Store {
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
    fn start_slack_install(&self, dsp: &str, state: &str, install: &Install) -> Result<()> {
        let db = self.dsp(dsp)?;
        let at = now();
        let (user, link) = match &install.starter {
            Starter::Member(user) => (Some(user.as_str()), None),
            Starter::Link(hash) => (None, Some(hash.as_str())),
        };
        db.transaction(|| {
            db.exec(
                "DELETE FROM slack_connect_requests WHERE expires_at<=?",
                [at],
            )?;
            db.exec(
                "INSERT INTO slack_connect_requests \
                 (state_hash,user_id,link_hash,team_id,label,expires_at) VALUES (?,?,?,?,?,?)",
                params![
                    crypto::sha(state),
                    user,
                    link,
                    install.team,
                    install.label,
                    at + INSTALL_MS
                ],
            )?;
            Ok(())
        })
    }
    fn finish_slack_install(&self, dsp: &str, state: &str) -> Result<Install> {
        let db = self.dsp(dsp)?;
        let hash = crypto::sha(state);
        db.transaction(|| {
            let found: Option<Requested> = db.one_as(
                "SELECT * FROM slack_connect_requests WHERE state_hash=?",
                [&hash],
            )?;
            db.exec(
                "DELETE FROM slack_connect_requests WHERE state_hash=?",
                [&hash],
            )?;
            found
                .filter(|found| found.expires_at > now())
                .and_then(|found| found.install)
                .ok_or_else(|| Error::new("slack_connect_expired", 409))
        })
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
                self.dsp(dsp)?.exec(
                    "INSERT OR REPLACE INTO slack_workspaces (team_id,name,status,scopes,\
                     bot_user_id,app_id,connected_by,connected_at,broken_at,label) \
                     VALUES (?,?,'connected',?,?,?,?,?,NULL,?)",
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
        let path = self.area(dsp, "secrets")?.join(secret(team)?);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        self.dsp(dsp)?
            .exec("DELETE FROM slack_workspaces WHERE team_id=?", [team])?;
        self.platform.exec(
            "DELETE FROM slack_teams WHERE team_id=? AND dsp_id=?",
            [team, dsp],
        )?;
        Ok(found)
    }
    fn slack_link(&self, dsp: &str) -> Result<Option<Link>> {
        let found: Option<(String, String, i64, Option<String>)> = self.dsp(dsp)?.one_as(
            "SELECT created_by,created_at,expires_at,label FROM slack_install_links \
             WHERE id=1 AND expires_at>?",
            [now()],
        )?;
        Ok(
            found.map(|(created_by, created_at, expires_at, label)| Link {
                created_by,
                created_at,
                expires_at,
                label,
            }),
        )
    }
    fn save_slack_link(
        &self,
        dsp: &str,
        hash: &str,
        user: &str,
        label: Option<&str>,
    ) -> Result<Link> {
        let link = Link {
            created_by: user.to_owned(),
            created_at: iso(),
            expires_at: now() + LINK_MS,
            label: label.map(str::to_owned),
        };
        self.dsp(dsp)?.exec(
            "INSERT OR REPLACE INTO slack_install_links (id,link_hash,created_by,created_at,\
             expires_at,label) VALUES (1,?,?,?,?,?)",
            params![
                hash,
                link.created_by,
                link.created_at,
                link.expires_at,
                link.label
            ],
        )?;
        Ok(link)
    }
    fn slack_link_works(&self, dsp: &str, hash: &str) -> Result<bool> {
        let found: Option<(i64,)> = self.dsp(dsp)?.one_as(
            "SELECT 1 FROM slack_install_links WHERE id=1 AND link_hash=? AND expires_at>?",
            params![hash, now()],
        )?;
        Ok(found.is_some())
    }
    fn end_slack_link(&self, dsp: &str, hash: Option<&str>) -> Result<bool> {
        let db = self.dsp(dsp)?;
        let ended = match hash {
            Some(hash) => db.exec("DELETE FROM slack_install_links WHERE link_hash=?", [hash])?,
            None => db.exec("DELETE FROM slack_install_links", [])?,
        };
        Ok(ended > 0)
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
