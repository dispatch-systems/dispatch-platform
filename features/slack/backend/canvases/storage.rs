//! What Sync Canvases keeps in each DSP's own database: the canvases chosen on their own, and
//! each canvas a linked channel or a choice covers, with when it was last edited, its document,
//! and how syncing it goes; its linked channels are `channel_links`'. Only this module runs SQL
//! on its tables; a workspace's rows go with it, in `storage::remove_slack_workspace`.
use super::ID;
use dispatch_core::{
    Error, Result,
    db::{FromRow, Row, Store},
};
use rusqlite::params;

/// A canvas chosen on its own, from a channel's, to keep in a folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pick {
    pub team: String,
    pub canvas: String,
    /// Its title, its channel's name and the folder's, as they were last seen.
    pub title: String,
    pub channel: String,
    pub channel_name: String,
    pub private: bool,
    pub folder: String,
    pub folder_name: String,
    pub picked_by: String,
    pub picked_at: i64,
}
impl FromRow for Pick {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            team: row.get("team_id")?,
            canvas: row.get("canvas_id")?,
            title: row.get("title")?,
            channel: row.get("channel_id")?,
            channel_name: row.get("channel_name")?,
            private: row.get::<i64>("private")? == 1,
            folder: row.get("folder_id")?,
            folder_name: row.get("folder_name")?,
            picked_by: row.get("picked_by")?,
            picked_at: row.get("picked_at")?,
        })
    }
}

/// How syncing a canvas goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// To be synced: it's tried at its `due_at`.
    Waiting,
    /// Its document holds it as it was last edited.
    Synced,
    /// Deleted in Slack: its document stays as it was.
    Gone,
    /// Tried as often as it's tried, or refused where it goes: tried again once it's edited.
    Failed,
}
impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Synced => "synced",
            Self::Gone => "gone",
            Self::Failed => "failed",
        }
    }
    fn parse(text: &str) -> Result<Self> {
        Ok(match text {
            "waiting" => Self::Waiting,
            "synced" => Self::Synced,
            "gone" => Self::Gone,
            "failed" => Self::Failed,
            _ => return Err(Error::new("slack_canvas_unreadable", 500)),
        })
    }
}

/// A canvas synced, or to be.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Canvas {
    pub team: String,
    pub canvas: String,
    /// The channel it was found in.
    pub channel: String,
    /// Its title, as it was last synced.
    pub title: Option<String>,
    pub status: Status,
    /// Why it's waiting or was left, as an error code.
    pub reason: Option<String>,
    pub attempts: i64,
    pub due_at: i64,
    /// When Slack last said it was edited, and the edit its document holds, in milliseconds.
    pub edited_at: Option<i64>,
    pub written_edit: Option<i64>,
    /// Its document, and where it opens.
    pub doc: Option<(String, String)>,
    pub noticed_at: i64,
    pub synced_at: Option<i64>,
}
impl FromRow for Canvas {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        let doc: Option<String> = row.get("doc_id")?;
        Ok(Self {
            team: row.get("team_id")?,
            canvas: row.get("canvas_id")?,
            channel: row.get("channel_id")?,
            title: row.get("title")?,
            status: Status::parse(&row.get::<String>("status")?)?,
            reason: row.get("reason")?,
            attempts: row.get("attempts")?,
            due_at: row.get("due_at")?,
            edited_at: row.get("edited_at")?,
            written_edit: row.get("written_edit")?,
            doc: match doc {
                Some(id) => Some((
                    id,
                    row.get::<Option<String>>("doc_link")?.unwrap_or_default(),
                )),
                None => None,
            },
            noticed_at: row.get("noticed_at")?,
            synced_at: row.get("synced_at")?,
        })
    }
}

/// What trying to sync a canvas came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Synced into the document `doc`, which opens at `link`, from the canvas titled `title` as
    /// it was edited at `edit`, in milliseconds.
    Synced {
        doc: String,
        link: String,
        title: String,
        edit: i64,
    },
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

/// The canvases chosen, noticed, synced and listed, in each DSP's database.
pub trait CanvasStore {
    /// The workspace `team`'s canvases chosen on their own, by title.
    fn slack_canvas_picks(&self, dsp: &str, team: &str) -> Result<Vec<Pick>>;
    fn slack_canvas_pick(&self, dsp: &str, team: &str, canvas: &str) -> Result<Option<Pick>>;
    /// Keeps the choice, in place of any the canvas had.
    fn save_slack_canvas_pick(&self, dsp: &str, pick: &Pick) -> Result<()>;
    /// Takes the choice away: whether there was one.
    fn remove_slack_canvas_pick(&self, dsp: &str, team: &str, canvas: &str) -> Result<bool>;
    /// Notes that Slack has the canvas `canvas`, found in `channel`, as last edited at `edited`,
    /// in milliseconds, at `at`: one new to it, or edited since its document was written, is
    /// synced at `due`. Whether it's to be synced.
    #[allow(clippy::too_many_arguments)]
    fn note_slack_canvas(
        &self,
        dsp: &str,
        team: &str,
        canvas: &str,
        channel: &str,
        edited: i64,
        at: i64,
        due: i64,
    ) -> Result<bool>;
    /// Notes that Slack told of a change to the canvas `canvas` at `at`: one Sync Canvases
    /// keeps is synced at `due`, but no later than `latest` after it began waiting, so a canvas
    /// edited on and on is still synced. Whether it keeps it.
    fn heard_slack_canvas(
        &self,
        dsp: &str,
        team: &str,
        canvas: &str,
        at: i64,
        due: i64,
        latest: i64,
    ) -> Result<bool>;
    /// The canvases waiting to be synced by `at`, the longest waiting first, `most` at most:
    /// only those a link or a choice covers, in a workspace with Sync Canvases on.
    fn due_slack_canvases(&self, dsp: &str, at: i64, most: usize) -> Result<Vec<Canvas>>;
    /// Claims the waiting canvas for one try until `until`: whether it was still due at `at`,
    /// so two tries never run at once.
    fn claim_slack_canvas(
        &self,
        dsp: &str,
        team: &str,
        canvas: &str,
        at: i64,
        until: i64,
    ) -> Result<bool>;
    /// Keeps what trying to sync the canvas came to, at `at`: one Slack told of a change to
    /// while it was tried waits to be synced again.
    fn finish_slack_canvas(
        &self,
        dsp: &str,
        team: &str,
        canvas: &str,
        outcome: &Outcome,
        at: i64,
    ) -> Result<()>;
    /// The workspace's canvases a link or a choice covers, the latest synced first.
    fn slack_canvases(&self, dsp: &str, team: &str) -> Result<Vec<Canvas>>;
}

/// Whether the canvas `c` is covered: chosen on its own, or found in a linked channel.
const COVERED: &str = "(EXISTS (SELECT 1 FROM slack_canvas_picks p WHERE p.team_id=c.team_id \
     AND p.canvas_id=c.canvas_id) OR EXISTS (SELECT 1 FROM slack_canvas_links l \
     WHERE l.team_id=c.team_id AND l.channel_id=c.channel_id))";

impl CanvasStore for Store {
    fn slack_canvas_picks(&self, dsp: &str, team: &str) -> Result<Vec<Pick>> {
        self.dsp(dsp)?.query_as(
            "SELECT * FROM slack_canvas_picks WHERE team_id=? ORDER BY title,canvas_id",
            [team],
        )
    }
    fn slack_canvas_pick(&self, dsp: &str, team: &str, canvas: &str) -> Result<Option<Pick>> {
        self.dsp(dsp)?.one_as(
            "SELECT * FROM slack_canvas_picks WHERE team_id=? AND canvas_id=?",
            [team, canvas],
        )
    }
    fn save_slack_canvas_pick(&self, dsp: &str, pick: &Pick) -> Result<()> {
        self.dsp(dsp)?.exec(
            "INSERT OR REPLACE INTO slack_canvas_picks (team_id,canvas_id,title,channel_id,\
             channel_name,private,folder_id,folder_name,picked_by,picked_at) \
             VALUES (?,?,?,?,?,?,?,?,?,?)",
            params![
                pick.team,
                pick.canvas,
                pick.title,
                pick.channel,
                pick.channel_name,
                pick.private,
                pick.folder,
                pick.folder_name,
                pick.picked_by,
                pick.picked_at
            ],
        )?;
        Ok(())
    }
    fn remove_slack_canvas_pick(&self, dsp: &str, team: &str, canvas: &str) -> Result<bool> {
        let removed = self.dsp(dsp)?.exec(
            "DELETE FROM slack_canvas_picks WHERE team_id=? AND canvas_id=?",
            [team, canvas],
        )?;
        Ok(removed > 0)
    }
    fn note_slack_canvas(
        &self,
        dsp: &str,
        team: &str,
        canvas: &str,
        channel: &str,
        edited: i64,
        at: i64,
        due: i64,
    ) -> Result<bool> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            db.exec(
                "INSERT INTO slack_canvases (team_id,canvas_id,channel_id,status,due_at,\
                 waiting_since,edited_at,noticed_at) VALUES (?1,?2,?3,'waiting',?4,?5,?6,?5) \
                 ON CONFLICT(team_id,canvas_id) DO UPDATE SET \
                 edited_at=max(coalesce(edited_at,0),excluded.edited_at)",
                params![team, canvas, channel, due, at, edited],
            )?;
            // Edited since its document was written, it's synced again.
            db.exec(
                "UPDATE slack_canvases SET status='waiting',reason=NULL,attempts=0,due_at=?,\
                 waiting_since=? WHERE team_id=? AND canvas_id=? \
                 AND status IN ('synced','failed') AND edited_at>coalesce(written_edit,-1)",
                params![due, at, team, canvas],
            )?;
            let waiting: Option<(String,)> = db.one_as(
                "SELECT status FROM slack_canvases WHERE team_id=? AND canvas_id=?",
                [team, canvas],
            )?;
            Ok(waiting.is_some_and(|(status,)| status == "waiting"))
        })
    }
    fn heard_slack_canvas(
        &self,
        dsp: &str,
        team: &str,
        canvas: &str,
        at: i64,
        due: i64,
        latest: i64,
    ) -> Result<bool> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let kept = db.exec(
                "UPDATE slack_canvases SET heard_at=? WHERE team_id=? AND canvas_id=?",
                params![at, team, canvas],
            )?;
            db.exec(
                "UPDATE slack_canvases SET status='waiting',reason=NULL,attempts=0,due_at=?,\
                 waiting_since=? WHERE team_id=? AND canvas_id=? \
                 AND status IN ('synced','failed')",
                params![due, at, team, canvas],
            )?;
            // Waiting already, and not for a failure, it waits for the edits to stop: since
            // the first of them, or since a try under way began.
            db.exec(
                "UPDATE slack_canvases SET waiting_since=coalesce(waiting_since,?1),\
                 due_at=min(?2,coalesce(waiting_since,?1)+?3) \
                 WHERE team_id=?4 AND canvas_id=?5 AND status='waiting' AND attempts=0",
                params![at, due, latest, team, canvas],
            )?;
            Ok(kept > 0)
        })
    }
    fn due_slack_canvases(&self, dsp: &str, at: i64, most: usize) -> Result<Vec<Canvas>> {
        self.dsp(dsp)?.query_as(
            &format!(
                "SELECT c.* FROM slack_canvases c WHERE c.status='waiting' AND c.due_at<=?1 \
                 AND coalesce(c.claimed_until,0)<=?1 AND {COVERED} AND EXISTS (SELECT 1 \
                 FROM slack_integrations i WHERE i.team_id=c.team_id AND i.integration=?3) \
                 ORDER BY c.due_at,c.noticed_at LIMIT ?2"
            ),
            params![at, most as i64, ID],
        )
    }
    fn claim_slack_canvas(
        &self,
        dsp: &str,
        team: &str,
        canvas: &str,
        at: i64,
        until: i64,
    ) -> Result<bool> {
        let claimed = self.dsp(dsp)?.exec(
            "UPDATE slack_canvases SET claimed_at=?1,claimed_until=?2,waiting_since=NULL \
             WHERE team_id=?3 AND canvas_id=?4 AND status='waiting' AND due_at<=?1 \
             AND coalesce(claimed_until,0)<=?1",
            params![at, until, team, canvas],
        )?;
        Ok(claimed > 0)
    }
    fn finish_slack_canvas(
        &self,
        dsp: &str,
        team: &str,
        canvas: &str,
        outcome: &Outcome,
        at: i64,
    ) -> Result<()> {
        let db = self.dsp(dsp)?;
        match outcome {
            Outcome::Synced {
                doc,
                link,
                title,
                edit,
            } => db.exec(
                "UPDATE slack_canvases SET status=CASE WHEN coalesce(heard_at,0)>claimed_at \
                 THEN 'waiting' ELSE 'synced' END,reason=NULL,attempts=0,doc_id=?,doc_link=?,\
                 title=?,written_edit=?,edited_at=max(coalesce(edited_at,0),?),synced_at=?,\
                 claimed_until=NULL WHERE team_id=? AND canvas_id=?",
                params![doc, link, title, edit, edit, at, team, canvas],
            )?,
            Outcome::Left { status, reason } => db.exec(
                "UPDATE slack_canvases SET status=?,reason=?,claimed_until=NULL \
                 WHERE team_id=? AND canvas_id=?",
                params![status.as_str(), reason, team, canvas],
            )?,
            Outcome::Later {
                reason,
                at: due,
                counted,
            } => db.exec(
                "UPDATE slack_canvases SET status='waiting',reason=?,due_at=?,\
                 attempts=attempts+?,claimed_until=NULL WHERE team_id=? AND canvas_id=?",
                params![reason, due, i64::from(*counted), team, canvas],
            )?,
        };
        Ok(())
    }
    fn slack_canvases(&self, dsp: &str, team: &str) -> Result<Vec<Canvas>> {
        self.dsp(dsp)?.query_as(
            &format!(
                "SELECT c.* FROM slack_canvases c WHERE c.team_id=? AND {COVERED} \
                 ORDER BY coalesce(c.synced_at,c.noticed_at) DESC,c.canvas_id"
            ),
            [team],
        )
    }
}
