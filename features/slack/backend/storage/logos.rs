//! Cached workspace logos and their picture-derived versions.
use super::workspaces::kept_workspace;
use dispatch_core::{Result, db::Store, foundation::crypto};
use rusqlite::params;
use std::collections::BTreeMap;

/// A workspace's logo, as Dispatch keeps a copy of it: the address Slack gave it at, and the
/// picture downloaded from there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Logo {
    pub source: String,
    pub picture: Vec<u8>,
}

pub trait LogoStore {
    /// The version of each of the DSP's workspaces' logos, by team ID: of each it keeps one of.
    fn slack_logos(&self, dsp: &str) -> Result<BTreeMap<String, String>>;
    fn slack_logo(&self, dsp: &str, team: &str) -> Result<Option<Logo>>;
    /// Keeps the workspace's logo in place of the one it had, while the DSP has the workspace:
    /// its version is its picture's digest.
    fn keep_slack_logo(&self, dsp: &str, team: &str, logo: &Logo) -> Result<()>;
    /// Forgets the workspace's logo, as Slack says it has none of its own.
    fn drop_slack_logo(&self, dsp: &str, team: &str) -> Result<()>;
}

impl LogoStore for Store {
    fn slack_logos(&self, dsp: &str) -> Result<BTreeMap<String, String>> {
        let rows: Vec<(String, String)> = self
            .dsp(dsp)?
            .query_as("SELECT team_id,version FROM slack_logos", [])?;
        Ok(rows.into_iter().collect())
    }
    fn slack_logo(&self, dsp: &str, team: &str) -> Result<Option<Logo>> {
        let found: Option<(String, Vec<u8>)> = self.dsp(dsp)?.one_as(
            "SELECT source,picture FROM slack_logos WHERE team_id=?",
            [team],
        )?;
        Ok(found.map(|(source, picture)| Logo { source, picture }))
    }
    fn keep_slack_logo(&self, dsp: &str, team: &str, logo: &Logo) -> Result<()> {
        let db = self.dsp(dsp)?;
        let version = &crypto::sha(&logo.picture)[..16];
        db.transaction(|| {
            if kept_workspace(&db, team)? {
                db.exec(
                    "INSERT OR REPLACE INTO slack_logos (team_id,source,version,picture) \
                     VALUES (?,?,?,?)",
                    params![team, logo.source, version, logo.picture],
                )?;
            }
            Ok(())
        })
    }
    fn drop_slack_logo(&self, dsp: &str, team: &str) -> Result<()> {
        self.dsp(dsp)?
            .exec("DELETE FROM slack_logos WHERE team_id=?", [team])?;
        Ok(())
    }
}
