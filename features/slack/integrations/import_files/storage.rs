//! What Import Files keeps in each DSP's own database: each file Slack told of in a linked
//! channel, with how saving it went; its links are `linked`'s. Only this module runs SQL on its
//! table; a workspace's rows go with it, as removing a workspace clears every table Slack declares by its `team_id`.
use dispatch_core::{
    Result,
    db::{Db, FromRow, Row, Store},
    text_enum,
};
use rusqlite::params;

text_enum! {
    /// How saving a file went: not yet, and tried again at its `due_at`; saved; larger than
    /// where it goes takes; not a file Slack hosts, as a canvas or a link to elsewhere is;
    /// deleted in Slack before it was saved; or tried as often as it's tried, or its folder gone.
    pub enum Status {
        Waiting => "waiting",
        Saved => "saved",
        TooLarge => "too_large",
        Skipped => "skipped",
        Gone => "gone",
        Failed => "failed",
    }
}

/// A file noticed in a linked channel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Import {
    pub team: String,
    pub file: String,
    pub channel: String,
    /// What Slack said of it, once it was asked.
    pub described: Option<Described>,
    pub status: Status,
    /// Why it's waiting or was left, as an error code.
    pub reason: Option<String>,
    pub attempts: i64,
    pub due_at: i64,
    /// Where it was saved, and where it opens.
    pub saved: Option<(String, String)>,
    pub noticed_at: i64,
}
impl FromRow for Import {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        let name: Option<String> = row.get("name")?;
        let saved_id: Option<String> = row.get("saved_id")?;
        Ok(Self {
            team: row.get("team_id")?,
            file: row.get("file_id")?,
            channel: row.get("channel_id")?,
            described: match name {
                Some(name) => Some(Described {
                    name,
                    mime: row.get::<Option<String>>("mime")?.unwrap_or_default(),
                    size: row.get::<Option<i64>>("size")?.unwrap_or(0),
                    posted_by: row.get("posted_by")?,
                    posted_name: row.get("posted_name")?,
                }),
                None => None,
            },
            status: row.get("status")?,
            reason: row.get("reason")?,
            attempts: row.get("attempts")?,
            due_at: row.get("due_at")?,
            saved: match saved_id {
                Some(id) => Some((
                    id,
                    row.get::<Option<String>>("saved_link")?.unwrap_or_default(),
                )),
                None => None,
            },
            noticed_at: row.get("noticed_at")?,
        })
    }
}

/// A file as Slack describes it: its name, type and size, and who posted it, by their Slack ID
/// and the name they're shown by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Described {
    pub name: String,
    pub mime: String,
    pub size: i64,
    pub posted_by: Option<String>,
    pub posted_name: Option<String>,
}

/// What trying to save a file came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Saved, with its ID where it went and where it opens.
    Saved { id: String, link: String },
    /// Left as it is, for `reason`.
    Left { status: Status, reason: String },
    /// Tried again at `at`, in milliseconds, for `reason`; `counted` when it's a failure, which
    /// counts towards giving up.
    Later {
        reason: String,
        at: i64,
        counted: bool,
    },
}

/// The files noticed, saved and listed, in each DSP's database.
pub trait ImportStore {
    /// Notices the file `file` in `channel`, due at `at`: whether it's new, as each file is
    /// noticed once, whichever channel it's in.
    fn notice_slack_import(
        &self,
        dsp: &str,
        team: &str,
        file: &str,
        channel: &str,
        at: i64,
    ) -> Result<bool>;
    /// The files waiting to be tried by `at`, the longest waiting first, `most` at most.
    fn due_slack_imports(&self, dsp: &str, at: i64, most: usize) -> Result<Vec<Import>>;
    /// Claims the waiting file for one try until `until`: whether it was still due at `at`, so
    /// two tries never run at once.
    fn claim_slack_import(
        &self,
        dsp: &str,
        team: &str,
        file: &str,
        at: i64,
        until: i64,
    ) -> Result<bool>;
    /// Keeps what Slack said of the file.
    fn describe_slack_import(
        &self,
        dsp: &str,
        team: &str,
        file: &str,
        described: &Described,
    ) -> Result<()>;
    /// Keeps what trying to save the file came to, at `at`.
    fn finish_slack_import(
        &self,
        dsp: &str,
        team: &str,
        file: &str,
        outcome: &Outcome,
        at: i64,
    ) -> Result<()>;
    /// The workspace's files noticed latest, `most` at most.
    fn recent_slack_imports(&self, dsp: &str, team: &str, most: usize) -> Result<Vec<Import>>;
    /// How many files each linked channel saved, and when it last did, in milliseconds.
    fn slack_import_totals(&self, dsp: &str, team: &str)
    -> Result<Vec<(String, i64, Option<i64>)>>;
}

impl ImportStore for Store {
    fn notice_slack_import(
        &self,
        dsp: &str,
        team: &str,
        file: &str,
        channel: &str,
        at: i64,
    ) -> Result<bool> {
        let added = self.dsp(dsp)?.exec(
            "INSERT OR IGNORE INTO slack_imports \
             (team_id,file_id,channel_id,status,due_at,noticed_at) VALUES (?,?,?,'waiting',?,?)",
            params![team, file, channel, at, at],
        )?;
        Ok(added > 0)
    }
    fn due_slack_imports(&self, dsp: &str, at: i64, most: usize) -> Result<Vec<Import>> {
        self.dsp(dsp)?.query_as(
            "SELECT * FROM slack_imports WHERE status='waiting' AND due_at<=? \
             ORDER BY due_at,noticed_at LIMIT ?",
            params![at, most as i64],
        )
    }
    fn claim_slack_import(
        &self,
        dsp: &str,
        team: &str,
        file: &str,
        at: i64,
        until: i64,
    ) -> Result<bool> {
        let claimed = self.dsp(dsp)?.exec(
            "UPDATE slack_imports SET due_at=? \
             WHERE team_id=? AND file_id=? AND status='waiting' AND due_at<=?",
            params![until, team, file, at],
        )?;
        Ok(claimed > 0)
    }
    fn describe_slack_import(
        &self,
        dsp: &str,
        team: &str,
        file: &str,
        described: &Described,
    ) -> Result<()> {
        self.dsp(dsp)?.exec(
            "UPDATE slack_imports SET name=?,mime=?,size=?,posted_by=?,posted_name=? \
             WHERE team_id=? AND file_id=?",
            params![
                described.name,
                described.mime,
                described.size,
                described.posted_by,
                described.posted_name,
                team,
                file
            ],
        )?;
        Ok(())
    }
    fn finish_slack_import(
        &self,
        dsp: &str,
        team: &str,
        file: &str,
        outcome: &Outcome,
        at: i64,
    ) -> Result<()> {
        let db = self.dsp(dsp)?;
        match outcome {
            Outcome::Saved { id, link } => db.exec(
                "UPDATE slack_imports SET status='saved',reason=NULL,saved_id=?,saved_link=?,\
                 done_at=? WHERE team_id=? AND file_id=?",
                params![id, link, at, team, file],
            )?,
            Outcome::Left { status, reason } => db.exec(
                "UPDATE slack_imports SET status=?,reason=?,done_at=? \
                 WHERE team_id=? AND file_id=?",
                params![status.as_str(), reason, at, team, file],
            )?,
            Outcome::Later {
                reason,
                at: due,
                counted,
            } => db.exec(
                "UPDATE slack_imports SET status='waiting',reason=?,due_at=?,\
                 attempts=attempts+? WHERE team_id=? AND file_id=?",
                params![reason, due, i64::from(*counted), team, file],
            )?,
        };
        Ok(())
    }
    fn recent_slack_imports(&self, dsp: &str, team: &str, most: usize) -> Result<Vec<Import>> {
        self.dsp(dsp)?.query_as(
            "SELECT * FROM slack_imports WHERE team_id=? ORDER BY noticed_at DESC LIMIT ?",
            params![team, most as i64],
        )
    }
    fn slack_import_totals(
        &self,
        dsp: &str,
        team: &str,
    ) -> Result<Vec<(String, i64, Option<i64>)>> {
        self.dsp(dsp)?.query_as(
            "SELECT channel_id,count(*),max(done_at) FROM slack_imports \
             WHERE team_id=? AND status='saved' GROUP BY channel_id",
            [team],
        )
    }
}

/// Forgets what was waiting from the workspace `team`'s channel `channel`, as it's unlinked.
pub(super) fn forget_waiting(db: &Db, team: &str, channel: &str) -> Result<()> {
    db.exec(
        "DELETE FROM slack_imports WHERE team_id=? AND channel_id=? AND status='waiting'",
        [team, channel],
    )?;
    Ok(())
}
