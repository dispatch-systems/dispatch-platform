//! Pending installs and hashed install links, with their expiry and consumption rules.
use dispatch_core::{
    Error, Result,
    db::{FromRow, Row, Store, iso, now},
    foundation::crypto,
};
use rusqlite::params;

/// How long an install started at Slack may take to come back.
const INSTALL_MS: i64 = 10 * 60 * 1000;
/// How long an install link lasts.
pub const LINK_MS: i64 = 7 * 24 * 60 * 60 * 1000;

/// Who an install was started by: a member, or the install link, by its hash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Starter {
    Member(String),
    Link(String),
}

/// An install someone started: who started it, the workspace it named to Slack, to reconnect
/// or update, none for a new one, the name the workspace it adds is to have, and the DSP page
/// it returns to, none for Settings' Connections.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Install {
    pub starter: Starter,
    pub team: Option<String>,
    pub label: Option<String>,
    pub page: Option<String>,
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
                    page: row.get("page")?,
                }),
                None => None,
            },
            expires_at: row.get("expires_at")?,
        })
    }
}

/// One of the DSP's install links, as stored: never the link itself, only its hash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    /// The workspace it updates or reconnects; none for one that adds a workspace.
    pub team: Option<String>,
    pub created_by: String,
    pub created_at: String,
    pub expires_at: i64,
    /// The name the workspace it adds is to have.
    pub label: Option<String>,
}
impl FromRow for Link {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            team: row.get("team_id")?,
            created_by: row.get("created_by")?,
            created_at: row.get("created_at")?,
            expires_at: row.get("expires_at")?,
            label: row.get("label")?,
        })
    }
}

pub trait InstallationStore {
    /// Records an install someone started, by the hash of the state Slack carries back.
    fn start_slack_install(&self, dsp: &str, state: &str, install: &Install) -> Result<()>;
    /// The install `state` names, used up: each comes back once, before it expires.
    fn finish_slack_install(&self, dsp: &str, state: &str) -> Result<Install>;
    /// The DSP page the install `state` names returns to, if it started on another than
    /// Settings' Connections: a read, which leaves it to finish.
    fn slack_install_page(&self, dsp: &str, state: &str) -> Result<Option<String>>;
    /// The DSP's install link that works for adding a workspace, or for the workspace `team`.
    fn slack_link(&self, dsp: &str, team: Option<&str>) -> Result<Option<Link>>;
    /// Keeps a new install link for the DSP, by its hash, in place only of the one for the
    /// same: for adding a workspace, with the name it's to have, or for the workspace `team`.
    fn save_slack_link(
        &self,
        dsp: &str,
        team: Option<&str>,
        hash: &str,
        user: &str,
        label: Option<&str>,
    ) -> Result<Link>;
    /// The DSP's link with this hash, while it works.
    fn working_slack_link(&self, dsp: &str, hash: &str) -> Result<Option<Link>>;
    /// Ends the DSP's install link for adding a workspace, or for the workspace `team`:
    /// whether there was one.
    fn end_slack_link(&self, dsp: &str, team: Option<&str>) -> Result<bool>;
    /// Ends the link with this hash, used: whether there was one.
    fn use_slack_link(&self, dsp: &str, hash: &str) -> Result<bool>;
}

impl InstallationStore for Store {
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
                 (state_hash,user_id,link_hash,team_id,label,page,expires_at) \
                 VALUES (?,?,?,?,?,?,?)",
                params![
                    crypto::sha(state),
                    user,
                    link,
                    install.team,
                    install.label,
                    install.page,
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
    fn slack_install_page(&self, dsp: &str, state: &str) -> Result<Option<String>> {
        let found: Option<(Option<String>,)> = self.dsp(dsp)?.one_as(
            "SELECT page FROM slack_connect_requests WHERE state_hash=? AND expires_at>?",
            params![crypto::sha(state), now()],
        )?;
        Ok(found.and_then(|(page,)| page))
    }
    fn slack_link(&self, dsp: &str, team: Option<&str>) -> Result<Option<Link>> {
        self.dsp(dsp)?.one_as(
            "SELECT * FROM slack_install_links WHERE team_id IS ? AND expires_at>?",
            params![team, now()],
        )
    }
    fn save_slack_link(
        &self,
        dsp: &str,
        team: Option<&str>,
        hash: &str,
        user: &str,
        label: Option<&str>,
    ) -> Result<Link> {
        let link = Link {
            team: team.map(str::to_owned),
            created_by: user.to_owned(),
            created_at: iso(),
            expires_at: now() + LINK_MS,
            label: label.map(str::to_owned),
        };
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            db.exec("DELETE FROM slack_install_links WHERE team_id IS ?", [team])?;
            db.exec(
                "INSERT INTO slack_install_links (link_hash,team_id,created_by,created_at,\
                 expires_at,label) VALUES (?,?,?,?,?,?)",
                params![
                    hash,
                    link.team,
                    link.created_by,
                    link.created_at,
                    link.expires_at,
                    link.label
                ],
            )
        })?;
        Ok(link)
    }
    fn working_slack_link(&self, dsp: &str, hash: &str) -> Result<Option<Link>> {
        self.dsp(dsp)?.one_as(
            "SELECT * FROM slack_install_links WHERE link_hash=? AND expires_at>?",
            params![hash, now()],
        )
    }
    fn end_slack_link(&self, dsp: &str, team: Option<&str>) -> Result<bool> {
        let ended = self
            .dsp(dsp)?
            .exec("DELETE FROM slack_install_links WHERE team_id IS ?", [team])?;
        Ok(ended > 0)
    }
    fn use_slack_link(&self, dsp: &str, hash: &str) -> Result<bool> {
        let ended = self
            .dsp(dsp)?
            .exec("DELETE FROM slack_install_links WHERE link_hash=?", [hash])?;
        Ok(ended > 0)
    }
}
