//! Slack's storage: in each DSP's own database, its workspaces, their members, the
//! integrations on in each, the installs started for them and its install link; each
//! workspace's tokens, encrypted in the DSP's secrets; and in the platform's database, which DSP
//! each workspace belongs to. Only this crate writes SQL against its tables.
use super::slack::Tokens;
use dispatch_core::{
    Error, Result,
    db::{self, Db, FromRow, Row, Store, iso, migrations::add_column, now},
    ensure,
    foundation::crypto,
};
use rusqlite::params;
use serde_json::json;

/// How long an install started at Slack may take to come back.
const INSTALL_MS: i64 = 10 * 60 * 1000;
/// How long an install link lasts.
pub const LINK_MS: i64 = 7 * 24 * 60 * 60 * 1000;

/// Slack's migration 2, for adding Slack from New on Settings' DSP Connections. Each workspace's
/// name in Dispatch, `label`, which whoever adds it may give it; without one it's known by its
/// name in Slack. An install a member started: the workspace it named to Slack, to reconnect or
/// update it, none for one started to add a workspace, which mustn't then be one the DSP already
/// has; and, like the install link, the name the workspace it adds is to have.
pub(crate) fn add_new_connections(db: &Db) -> Result<()> {
    add_column(db, "slack_workspaces", "label", "TEXT")?;
    add_column(db, "slack_connect_requests", "team_id", "TEXT")?;
    add_column(db, "slack_connect_requests", "label", "TEXT")?;
    add_column(db, "slack_install_links", "label", "TEXT")
}

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

/// A member of a workspace, as Slack describes them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    pub user: String,
    pub name: String,
    /// The name they go by in Slack, when it isn't `name`.
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    /// Whether their account is active: one deactivated in Slack is kept, inactive.
    pub active: bool,
    pub guest: bool,
}
impl FromRow for Member {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            user: row.get("user_id")?,
            name: row.get("name")?,
            display_name: row.get("display_name")?,
            email: row.get("email")?,
            phone: row.get("phone")?,
            active: row.get("active")?,
            guest: row.get("guest")?,
        })
    }
}
/// A member with the workspace they're in, as `slack_members` reads them.
struct InWorkspace(String, Member);
impl FromRow for InWorkspace {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self(row.get("team_id")?, Member::from_row(row)?))
    }
}
/// Whether the DSP still has the workspace `team`: one disconnected while its members were
/// being read keeps none of them.
fn kept_workspace(db: &Db, team: &str) -> Result<bool> {
    Ok(db
        .one("SELECT 1 FROM slack_workspaces WHERE team_id=?", [team])?
        .is_some())
}
/// Keeps one member, as Slack describes them now: whether they're new, a member Driver Match
/// has no person for yet. What it reads of the others it reads afresh each time.
fn keep_member(db: &Db, team: &str, member: &Member) -> Result<bool> {
    let kept: Vec<Member> = db.query_as(
        "SELECT * FROM slack_members WHERE team_id=? AND user_id=?",
        [team, &member.user],
    )?;
    if kept.first() == Some(member) {
        return Ok(false);
    }
    db.exec(
        "INSERT INTO slack_members(team_id,user_id,name,display_name,email,phone,active,guest) \
         VALUES (?,?,?,?,?,?,?,?) ON CONFLICT(team_id,user_id) DO UPDATE SET \
         name=excluded.name,display_name=excluded.display_name,email=excluded.email,\
         phone=excluded.phone,active=excluded.active,guest=excluded.guest",
        params![
            team,
            member.user,
            member.name,
            member.display_name,
            member.email,
            member.phone,
            member.active,
            member.guest
        ],
    )?;
    Ok(kept.is_empty())
}

/// Slack's migration 8: when the DSP got each workspace (`added_at`), which reconnecting or
/// updating it leaves as it was, unlike when it was last connected: for one it had already, when
/// it was connected.
pub(crate) fn add_workspaces_added(db: &Db) -> Result<()> {
    add_column(db, "slack_workspaces", "added_at", "TEXT")?;
    db.exec(
        "UPDATE slack_workspaces SET added_at=connected_at WHERE added_at IS NULL",
        [],
    )?;
    Ok(())
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
    /// Forgets the workspace, its members and its tokens, and frees it for any DSP: what was
    /// forgotten.
    fn remove_slack_workspace(&self, dsp: &str, team: &str) -> Result<Option<Workspace>>;
    /// Every member of the DSP's workspaces, with their workspace.
    fn slack_members(&self, dsp: &str) -> Result<Vec<(String, Member)>>;
    /// Keeps the workspace's members, all of them as Slack lists them now, while the DSP has
    /// the workspace: whether any is new.
    fn keep_slack_members(&self, dsp: &str, team: &str, members: &[Member]) -> Result<bool>;
    /// Keeps one member of the workspace, as Slack describes them now, while the DSP has the
    /// workspace: whether they're new.
    fn keep_slack_member(&self, dsp: &str, team: &str, member: &Member) -> Result<bool>;
    /// The IDs of the integrations on in the workspace `team`, in order: each switched on there,
    /// and each that starts on and wasn't switched off there. None for a workspace the DSP
    /// doesn't have.
    fn slack_integrations(&self, dsp: &str, team: &str) -> Result<Vec<String>>;
    /// When the integration `id` came on in the workspace `team`, in milliseconds: when someone
    /// switched it on there, or, for one on from the start, when the DSP got the workspace,
    /// however often it was reconnected since. None while it's off.
    fn slack_integration_on_since(&self, dsp: &str, team: &str, id: &str) -> Result<Option<i64>>;
    /// Switches the integration `id` on in the workspace `team`, as the member `by`: whether it
    /// was off.
    fn turn_on_slack_integration(&self, dsp: &str, team: &str, id: &str, by: &str) -> Result<bool>;
    /// Switches the integration `id` off in the workspace `team`, as the member `by`: whether it
    /// was on.
    fn turn_off_slack_integration(&self, dsp: &str, team: &str, id: &str, by: &str)
    -> Result<bool>;
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
/// Whether the integration `id` starts on in every workspace, as its feature declares it.
fn starts_on(id: &str) -> bool {
    super::integrations::declared()
        .any(|(_, integration)| integration.id == id && integration.starts_on)
}
/// Whether the integration `id` is on in the workspace `team`: switched on there, or starting
/// on and not switched off there.
fn switched_on(db: &Db, team: &str, id: &str) -> Result<bool> {
    let row = |table: &str| {
        db.one(
            &format!("SELECT 1 FROM {table} WHERE team_id=? AND integration=?"),
            [team, id],
        )
    };
    Ok(kept_workspace(db, team)?
        && (row("slack_integrations")?.is_some()
            || (starts_on(id) && row("slack_integrations_off")?.is_none())))
}
/// An ISO 8601 time `column` holds, in milliseconds, as SQL reads it.
fn ms(column: &str) -> String {
    format!("CAST((julianday({column}) - 2440587.5) * 86400000 AS INTEGER)")
}
/// The tables Slack declares in each DSP's database, which removing a workspace clears of it.
fn workspace_tables() -> impl Iterator<Item = &'static str> {
    crate::FEATURE
        .tables
        .iter()
        .filter(|(database, _)| *database == "dsp")
        .flat_map(|(_, tables)| tables.iter().copied())
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
        for (_, integration) in super::integrations::declared() {
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
    fn slack_members(&self, dsp: &str) -> Result<Vec<(String, Member)>> {
        let rows: Vec<InWorkspace> = self
            .dsp(dsp)?
            .query_as("SELECT * FROM slack_members ORDER BY team_id,user_id", [])?;
        Ok(rows
            .into_iter()
            .map(|InWorkspace(team, member)| (team, member))
            .collect())
    }
    fn keep_slack_members(&self, dsp: &str, team: &str, members: &[Member]) -> Result<bool> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            if !kept_workspace(&db, team)? {
                return Ok(false);
            }
            let mut new = false;
            for member in members {
                new |= keep_member(&db, team, member)?;
            }
            let listed = serde_json::to_string(
                &members
                    .iter()
                    .map(|member| &member.user)
                    .collect::<Vec<_>>(),
            )?;
            db.exec(
                "DELETE FROM slack_members WHERE team_id=? \
                 AND user_id NOT IN (SELECT value FROM json_each(?))",
                [team, &listed],
            )?;
            Ok(new)
        })
    }
    fn keep_slack_member(&self, dsp: &str, team: &str, member: &Member) -> Result<bool> {
        let db = self.dsp(dsp)?;
        db.transaction(|| Ok(kept_workspace(&db, team)? && keep_member(&db, team, member)?))
    }
    fn slack_integrations(&self, dsp: &str, team: &str) -> Result<Vec<String>> {
        let db = self.dsp(dsp)?;
        if !kept_workspace(&db, team)? {
            return Ok(Vec::new());
        }
        let ids = |table: &str| -> Result<Vec<String>> {
            let rows: Vec<(String,)> = db.query_as(
                &format!("SELECT integration FROM {table} WHERE team_id=?"),
                [team],
            )?;
            Ok(rows.into_iter().map(|(id,)| id).collect())
        };
        let (mut on, off) = (ids("slack_integrations")?, ids("slack_integrations_off")?);
        for (_, integration) in super::integrations::declared() {
            let id = integration.id.to_owned();
            if integration.starts_on && !off.contains(&id) && !on.contains(&id) {
                on.push(id);
            }
        }
        on.sort();
        Ok(on)
    }
    fn slack_integration_on_since(&self, dsp: &str, team: &str, id: &str) -> Result<Option<i64>> {
        let db = self.dsp(dsp)?;
        let switched: Option<(i64,)> = db.one_as(
            &format!(
                "SELECT {} FROM slack_integrations WHERE team_id=? AND integration=?",
                ms("turned_on_at")
            ),
            [team, id],
        )?;
        if switched.is_some() || !starts_on(id) {
            return Ok(switched.map(|(at,)| at));
        }
        let connected: Option<(i64,)> = db.one_as(
            &format!(
                "SELECT {} FROM slack_workspaces WHERE team_id=?1 AND NOT EXISTS \
                 (SELECT 1 FROM slack_integrations_off WHERE team_id=?1 AND integration=?2)",
                ms("added_at")
            ),
            [team, id],
        )?;
        Ok(connected.map(|(at,)| at))
    }
    fn turn_on_slack_integration(&self, dsp: &str, team: &str, id: &str, by: &str) -> Result<bool> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            if switched_on(&db, team, id)? {
                return Ok(false);
            }
            db.exec(
                "DELETE FROM slack_integrations_off WHERE team_id=? AND integration=?",
                [team, id],
            )?;
            db.exec(
                "INSERT OR IGNORE INTO slack_integrations \
                 (team_id,integration,turned_on_by,turned_on_at) VALUES (?,?,?,?)",
                params![team, id, by, iso()],
            )?;
            Ok(true)
        })
    }
    fn turn_off_slack_integration(
        &self,
        dsp: &str,
        team: &str,
        id: &str,
        by: &str,
    ) -> Result<bool> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            if !switched_on(&db, team, id)? {
                return Ok(false);
            }
            db.exec(
                "DELETE FROM slack_integrations WHERE team_id=? AND integration=?",
                [team, id],
            )?;
            // One that starts on stays off once switched off.
            if starts_on(id) {
                db.exec(
                    "INSERT OR REPLACE INTO slack_integrations_off \
                     (team_id,integration,turned_off_by,turned_off_at) VALUES (?,?,?,?)",
                    params![team, id, by, iso()],
                )?;
            }
            Ok(true)
        })
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
