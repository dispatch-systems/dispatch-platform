//! The channels each integration that links them to folders keeps, in its own table of each
//! DSP's database. Only this module writes those tables, and an integration's own storage may
//! read its own in its queries; a workspace's rows go with it, in
//! `storage::remove_slack_workspace`.
use super::Linking;
use dispatch_core::{
    Result,
    db::{FromRow, Row, Store},
};
use rusqlite::params;

/// A channel linked to the folder its things are kept in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub team: String,
    pub channel: String,
    /// The channel's name, and the folder's, as they were last seen.
    pub channel_name: String,
    pub private: bool,
    pub folder: String,
    pub folder_name: String,
    pub linked_by: String,
    /// When it was linked, in milliseconds.
    pub linked_at: i64,
}
impl Link {
    /// The folder it keeps its channel's things in, and its name as last seen.
    pub fn folder(&self) -> (&str, &str) {
        (&self.folder, &self.folder_name)
    }
}
impl FromRow for Link {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            team: row.get("team_id")?,
            channel: row.get("channel_id")?,
            channel_name: row.get("channel_name")?,
            private: row.get::<i64>("private")? == 1,
            folder: row.get("folder_id")?,
            folder_name: row.get("folder_name")?,
            linked_by: row.get("linked_by")?,
            linked_at: row.get("linked_at")?,
        })
    }
}

/// The channels each integration linked, in each DSP's database, by the table it declares.
pub trait LinkStore {
    /// The workspace `team`'s channels `of` linked, by name.
    fn slack_channel_links(&self, dsp: &str, of: &Linking, team: &str) -> Result<Vec<Link>>;
    fn slack_channel_link(
        &self,
        dsp: &str,
        of: &Linking,
        team: &str,
        channel: &str,
    ) -> Result<Option<Link>>;
    /// Keeps the link, in place of any the channel had.
    fn save_slack_channel_link(&self, dsp: &str, of: &Linking, link: &Link) -> Result<()>;
    /// Takes the link away, with what `of` takes with it: whether there was one.
    fn remove_slack_channel_link(
        &self,
        dsp: &str,
        of: &Linking,
        team: &str,
        channel: &str,
    ) -> Result<bool>;
}

impl LinkStore for Store {
    fn slack_channel_links(&self, dsp: &str, of: &Linking, team: &str) -> Result<Vec<Link>> {
        self.dsp(dsp)?.query_as(
            &format!(
                "SELECT * FROM {} WHERE team_id=? ORDER BY channel_name,channel_id",
                of.table
            ),
            [team],
        )
    }
    fn slack_channel_link(
        &self,
        dsp: &str,
        of: &Linking,
        team: &str,
        channel: &str,
    ) -> Result<Option<Link>> {
        self.dsp(dsp)?.one_as(
            &format!(
                "SELECT * FROM {} WHERE team_id=? AND channel_id=?",
                of.table
            ),
            [team, channel],
        )
    }
    fn save_slack_channel_link(&self, dsp: &str, of: &Linking, link: &Link) -> Result<()> {
        self.dsp(dsp)?.exec(
            &format!(
                "INSERT OR REPLACE INTO {} (team_id,channel_id,channel_name,private,folder_id,\
                 folder_name,linked_by,linked_at) VALUES (?,?,?,?,?,?,?,?)",
                of.table
            ),
            params![
                link.team,
                link.channel,
                link.channel_name,
                link.private,
                link.folder,
                link.folder_name,
                link.linked_by,
                link.linked_at
            ],
        )?;
        Ok(())
    }
    fn remove_slack_channel_link(
        &self,
        dsp: &str,
        of: &Linking,
        team: &str,
        channel: &str,
    ) -> Result<bool> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            (of.unlinking)(&db, team, channel)?;
            let removed = db.exec(
                &format!("DELETE FROM {} WHERE team_id=? AND channel_id=?", of.table),
                [team, channel],
            )?;
            Ok(removed > 0)
        })
    }
}
