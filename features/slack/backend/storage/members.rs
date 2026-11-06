//! The members kept for a DSP's connected workspaces.
use super::workspaces::kept_workspace;
use dispatch_core::{
    Result,
    db::{Db, FromRow, Row, Store},
};
use rusqlite::params;

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

pub trait MemberStore {
    /// Every member of the DSP's workspaces, with their workspace.
    fn slack_members(&self, dsp: &str) -> Result<Vec<(String, Member)>>;
    /// Keeps the workspace's members, all of them as Slack lists them now, while the DSP has
    /// the workspace: whether any is new.
    fn keep_slack_members(&self, dsp: &str, team: &str, members: &[Member]) -> Result<bool>;
    /// Keeps one member of the workspace, as Slack describes them now, while the DSP has the
    /// workspace: whether they're new.
    fn keep_slack_member(&self, dsp: &str, team: &str, member: &Member) -> Result<bool>;
}

impl MemberStore for Store {
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
}
