//! The photos and files attached to messages, kept in Dispatch, in the DSP's own data, so they
//! outlive Slack: each by its bytes' SHA-256, so one sent twice is kept once. A member's are
//! downloaded from Slack (`files:read`): each kept in the DSP's database until it is, tried at
//! once, then, when it fails, after a minute, five, fifteen, an hour, three, six and twelve, and
//! given up after that. Only a file Slack hosts is downloaded, up to 100 MB; one larger, or a
//! link to elsewhere, keeps its name, type and size alone. A photo someone at the DSP adds to a
//! message is kept as it's uploaded, before the message is sent, and let go if it isn't sent
//! within a day. Their bytes arrive, take their place and are let go as `bytes` keeps them, with
//! the files that keep them.
use super::{
    bytes::{Arrived, Folder},
    changes::{ChangeStore, changed_conversation},
    live,
    storage::DOMAIN,
};
use crate::integrations::messages::ID;
use dispatch_core::{
    Error, Result, State,
    db::{FromRow, Row, Store, now},
    foundation::crypto,
    manifest::{Maintenance, Upkeep},
    server::http::upload::{UPLOAD_LIMIT, Upload},
};
use dispatch_slack::kit::{self, Bot, Bytestream, Queue};
use rusqlite::params;
use serde_json::json;
use std::{sync::Arc, time::Duration};

/// Every minute, the files due to download in each DSP; hourly, the photos added and never sent
/// are let go, then the bytes no file keeps or left arriving.
pub const MAINTENANCE: Maintenance = Maintenance {
    every: Duration::from_secs(60 * 60),
    run: pass,
};
/// How long a file waits after each failure, in milliseconds: given up after the last.
const RETRIES_MS: &[i64] = &[
    60 * 1000,
    5 * 60 * 1000,
    15 * 60 * 1000,
    60 * 60 * 1000,
    3 * 60 * 60 * 1000,
    6 * 60 * 60 * 1000,
    12 * 60 * 60 * 1000,
];
/// How long a download waits while it can't be tried, as before the workspace's admin approves
/// reading files, in milliseconds.
const WAIT_MS: i64 = 15 * 60 * 1000;
/// How long a photo added for a message waits for it to be sent, in milliseconds.
const UNSENT_MS: i64 = 24 * 60 * 60 * 1000;
/// How a failure is reported.
const FAILED: &str = "communications_download_failed";
/// Where the DSP's photos and files are kept, and which bytes its messages' files and the photos
/// added for them keep.
pub const BYTES: Folder = Folder {
    owner: "communications",
    keeps: <Store as FileStore>::communications_bytes_kept,
    kept: <Store as FileStore>::communications_kept_bytes,
};

dispatch_core::text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/communications/api/generated/"))]
    /// Where a file stands: being downloaded from Slack; kept in Dispatch; too large to keep;
    /// not one Slack hosts, as a link to elsewhere; or given up after failing.
    pub enum FileStatus {
        Waiting => "waiting",
        Kept => "kept",
        TooLarge => "too_large",
        Unavailable => "unavailable",
        Failed => "failed",
    }
}

/// A photo or file attached to a message, as kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct File {
    pub id: String,
    /// Its message; none for a photo added for one not sent yet, or sent to a group.
    pub message: Option<String>,
    /// The group message it was sent with, which each of the group's people's own message shows.
    pub group_message: Option<String>,
    /// Who added it, for a photo someone at the DSP added.
    pub uploaded_by: Option<String>,
    pub name: String,
    pub mime: String,
    pub size: i64,
    pub status: FileStatus,
    /// Its bytes' SHA-256, once it's kept.
    pub sha256: Option<String>,
}
impl FromRow for File {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            message: row.get("message_id")?,
            group_message: row.get("group_message_id")?,
            uploaded_by: row.get("uploaded_by")?,
            name: row.get("name")?,
            mime: row.get("mime")?,
            size: row.get("size")?,
            status: row.get("status")?,
            sha256: row.get("sha256")?,
        })
    }
}

/// What Communications reads and writes of its files.
pub trait FileStore {
    fn communications_file(&self, dsp: &str, id: &str) -> Result<Option<File>>;
    /// The files of the messages `ids` name, in the order they were added.
    fn communications_files(&self, dsp: &str, messages: &[String]) -> Result<Vec<File>>;
    /// The photos of the group messages `ids` name, in the order they were added.
    fn communications_group_files(&self, dsp: &str, messages: &[String]) -> Result<Vec<File>>;
    /// Keeps a photo the member `by` added for a message not sent yet: its ID.
    fn add_communications_upload(
        &self,
        dsp: &str,
        by: &str,
        name: &str,
        mime: &str,
        size: i64,
        sha256: &str,
    ) -> Result<String>;
    /// Claims the photos `ids` names that the member `by` added for a message not sent yet, in
    /// their order, until `until`, while their message goes to Slack: refused if any isn't one,
    /// or another message is being sent with it.
    fn claim_communications_uploads(
        &self,
        dsp: &str,
        by: &str,
        ids: &[String],
        until: i64,
    ) -> Result<Vec<File>>;
    /// Lets go of the photos `ids` names, claimed for a message Slack didn't take.
    fn release_communications_uploads(&self, dsp: &str, ids: &[String]) -> Result<()>;
    /// Lets go of the photos added before `before` for a message never sent, but one going with
    /// a message now, whose bytes upkeep then lets go of once no file keeps them.
    fn forget_unsent_communications(&self, dsp: &str, before: i64) -> Result<()>;
    /// Whether any file is kept by the SHA-256 `sha256`.
    fn communications_bytes_kept(&self, dsp: &str, sha256: &str) -> Result<bool>;
    /// The SHA-256 of every bytes a file keeps.
    fn communications_kept_bytes(&self, dsp: &str) -> Result<Vec<String>>;
}
impl FileStore for Store {
    fn communications_file(&self, dsp: &str, id: &str) -> Result<Option<File>> {
        self.dsp(dsp)?
            .one_as("SELECT * FROM communications_files WHERE id=?", [id])
    }
    fn communications_files(&self, dsp: &str, messages: &[String]) -> Result<Vec<File>> {
        self.dsp(dsp)?.query_as(
            "SELECT * FROM communications_files \
             WHERE message_id IN (SELECT value FROM json_each(?)) ORDER BY added_at,id",
            [serde_json::to_string(messages)?],
        )
    }
    fn communications_group_files(&self, dsp: &str, messages: &[String]) -> Result<Vec<File>> {
        self.dsp(dsp)?.query_as(
            "SELECT * FROM communications_files \
             WHERE group_message_id IN (SELECT value FROM json_each(?)) ORDER BY added_at,id",
            [serde_json::to_string(messages)?],
        )
    }
    fn add_communications_upload(
        &self,
        dsp: &str,
        by: &str,
        name: &str,
        mime: &str,
        size: i64,
        sha256: &str,
    ) -> Result<String> {
        let id = crypto::id("file")?;
        self.dsp(dsp)?.exec(
            "INSERT INTO communications_files (id,uploaded_by,name,mime,size,status,sha256,\
             added_at) VALUES (?,?,?,?,?,'kept',?,?)",
            params![id, by, name, mime, size, sha256, now()],
        )?;
        Ok(id)
    }
    fn claim_communications_uploads(
        &self,
        dsp: &str,
        by: &str,
        ids: &[String],
        until: i64,
    ) -> Result<Vec<File>> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let found: Vec<File> = db.query_as(
                "SELECT * FROM communications_files WHERE id IN (SELECT value FROM json_each(?)) \
                 AND uploaded_by=? AND message_id IS NULL AND group_message_id IS NULL \
                 AND coalesce(claimed_until,0)<=?",
                params![serde_json::to_string(ids)?, by, now()],
            )?;
            let claimed = ids
                .iter()
                .map(|id| {
                    found
                        .iter()
                        .find(|file| file.id == *id)
                        .cloned()
                        .ok_or_else(|| Error::new("communications_upload_not_found", 404))
                })
                .collect::<Result<Vec<File>>>()?;
            db.exec(
                "UPDATE communications_files SET claimed_until=? \
                 WHERE id IN (SELECT value FROM json_each(?))",
                params![until, serde_json::to_string(ids)?],
            )?;
            Ok(claimed)
        })
    }
    fn release_communications_uploads(&self, dsp: &str, ids: &[String]) -> Result<()> {
        self.dsp(dsp)?.exec(
            "UPDATE communications_files SET claimed_until=NULL \
             WHERE id IN (SELECT value FROM json_each(?)) AND message_id IS NULL \
             AND group_message_id IS NULL",
            [serde_json::to_string(ids)?],
        )?;
        Ok(())
    }
    fn forget_unsent_communications(&self, dsp: &str, before: i64) -> Result<()> {
        // Not one going to Slack with a message now, nor a group message's.
        self.dsp(dsp)?.exec(
            "DELETE FROM communications_files WHERE message_id IS NULL \
             AND group_message_id IS NULL AND added_at<? AND coalesce(claimed_until,0)<?",
            params![before, now()],
        )?;
        Ok(())
    }
    fn communications_bytes_kept(&self, dsp: &str, sha256: &str) -> Result<bool> {
        Ok(self.dsp(dsp)?.count(
            "SELECT count(*) FROM communications_files WHERE sha256=?",
            [sha256],
        )? > 0)
    }
    fn communications_kept_bytes(&self, dsp: &str) -> Result<Vec<String>> {
        let kept: Vec<(String,)> = self.dsp(dsp)?.query_as(
            "SELECT DISTINCT sha256 FROM communications_files WHERE sha256 IS NOT NULL",
            [],
        )?;
        Ok(kept.into_iter().map(|(sha256,)| sha256).collect())
    }
}

/// The bytes of the DSP's file kept by `sha256`, as they're read.
pub async fn read(state: &Arc<State>, dsp: &str, sha256: &str) -> Result<Bytestream> {
    let kept = BYTES
        .read(state, dsp, sha256)
        .await?
        .ok_or_else(|| Error::new("communications_file_not_found", 404))?;
    Ok(kept.chunks)
}

/// Downloads the DSP's files that are due now, then tells the pages waiting on its
/// conversations, when any download came to something.
pub async fn download_due(state: &Arc<State>, dsp: &str) {
    let revision = || {
        let at = dsp.to_owned();
        state.read(move |db| db.communications_revision(&at))
    };
    let before = revision().await;
    kit::due::<Downloads>(state, dsp).await;
    if before.is_err() || revision().await.ok() != before.ok() {
        live::changed(state, dsp);
    }
}
fn pass(state: Arc<State>, due: bool) -> Upkeep {
    Box::pin(async move {
        for dsp in kit::dsps(&state, FAILED).await {
            download_due(&state, &dsp).await;
            if !due {
                continue;
            }
            let at = dsp.clone();
            let tidied = state
                .run_scoped(dsp, DOMAIN, move |db| {
                    let_go(db, &at, now() - UNSENT_MS);
                    Ok(())
                })
                .await;
            if let Err(error) = tidied {
                kit::failed(FAILED, &error);
            }
        }
    })
}
/// Lets go of the DSP's photos added before `before` and never sent, then of the bytes no file
/// keeps or left arriving, each job on its own, and each of the bytes, so one that fails is
/// reported and tried again the next hour, and the rest still go. Inside a write.
fn let_go(db: &Store, dsp: &str, before: i64) {
    if let Err(error) = db.forget_unsent_communications(dsp, before) {
        kit::failed(FAILED, &error);
    }
    for error in BYTES.reconcile(db, dsp).unwrap_or_else(|error| vec![error]) {
        kit::failed(FAILED, &error);
    }
}

/// A member's file due to download: its workspace, Slack's ID of it, and how often it failed.
#[derive(Clone)]
pub struct Due {
    id: String,
    team: String,
    slack_id: String,
    attempts: i64,
}
impl FromRow for Due {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            team: row.get("team_id")?,
            slack_id: row.get("slack_file_id")?,
            attempts: row.get("attempts")?,
        })
    }
}
/// What trying to download a file came to: one kept carries its bytes, beside the DSP's until
/// finishing keeps it.
pub enum Outcome {
    Kept(Arrived),
    Later {
        reason: String,
        at: i64,
        counted: bool,
    },
    Left {
        status: FileStatus,
        reason: String,
    },
}

/// The members' files, downloaded from Slack as they're due, while Communications is on.
struct Downloads;
impl Queue for Downloads {
    type Item = Due;
    type Claimed = ();
    type Outcome = Outcome;
    const BATCH: usize = 20;
    const CLAIM_MS: i64 = 30 * 60 * 1000;
    const FAILED: &'static str = FAILED;

    fn due(db: &Store, dsp: &str, at: i64, most: usize) -> Result<Vec<Due>> {
        if !db.feature_enabled(dsp, crate::FEATURE.name)? {
            return Ok(Vec::new());
        }
        db.dsp(dsp)?.query_as(
            "SELECT f.id,c.team_id,f.slack_file_id,f.attempts FROM communications_files f \
             JOIN communications_messages m ON m.id=f.message_id \
             JOIN communications_conversations c ON c.id=m.conversation_id \
             WHERE f.status='waiting' AND f.due_at<=?1 AND coalesce(f.claimed_until,0)<=?1 \
             ORDER BY f.due_at,f.id LIMIT ?2",
            params![at, most as i64],
        )
    }
    fn claim(db: &Store, dsp: &str, item: &Due, at: i64, until: i64) -> Result<Option<()>> {
        let claimed = db.dsp(dsp)?.exec(
            "UPDATE communications_files SET claimed_until=?1 WHERE id=?2 AND status='waiting' \
             AND due_at<=?3 AND coalesce(claimed_until,0)<=?3",
            params![until, item.id, at],
        )?;
        Ok((claimed > 0).then_some(()))
    }
    async fn try_one(state: &Arc<State>, dsp: &str, item: &Due, (): ()) -> Outcome {
        match download(state, dsp, item).await {
            Ok(outcome) => outcome,
            Err(error) => failure(item, &error),
        }
    }
    fn finish(db: &Store, dsp: &str, item: &Due, outcome: &Outcome, _: i64) -> Result<()> {
        match outcome {
            // Its bytes take their place with the file that keeps them.
            Outcome::Kept(arrived) => {
                BYTES.keep(db, dsp, arrived, || finished(db, dsp, item, outcome))
            }
            _ => finished(db, dsp, item, outcome),
        }
    }
}
/// Keeps what trying to download `item` came to, and notes its conversation changed when its
/// message shows it.
fn finished(db: &Store, dsp: &str, item: &Due, outcome: &Outcome) -> Result<()> {
    let db = db.dsp(dsp)?;
    db.transaction(|| {
        // Its message shows it kept, or not to be: a wait changes nothing it shows.
        let shown = !matches!(outcome, Outcome::Later { .. });
        match outcome {
            Outcome::Kept(arrived) => db.exec(
                "UPDATE communications_files SET status='kept',sha256=?,size=?,reason=NULL,\
                 claimed_until=NULL WHERE id=?",
                params![arrived.sha256, arrived.size, item.id],
            )?,
            Outcome::Later {
                reason,
                at,
                counted,
            } => db.exec(
                "UPDATE communications_files SET due_at=?,reason=?,claimed_until=NULL,\
                 attempts=attempts+? WHERE id=?",
                params![at, reason, i64::from(*counted), item.id],
            )?,
            Outcome::Left { status, reason } => db.exec(
                "UPDATE communications_files SET status=?,reason=?,claimed_until=NULL \
                 WHERE id=?",
                params![status, reason, item.id],
            )?,
        };
        if !shown {
            return Ok(());
        }
        let (conversation,): (String,) = db
            .one_as(
                "SELECT m.conversation_id FROM communications_files f \
                 JOIN communications_messages m ON m.id=f.message_id WHERE f.id=?",
                [&item.id],
            )?
            .ok_or_else(|| Error::new("communications_file_not_found", 404))?;
        changed_conversation(&db, &conversation)
    })
}

/// Downloads one of a member's files from Slack, while Communications is on in its workspace:
/// one of a workspace the DSP no longer has fails, as any failure does.
async fn download(state: &Arc<State>, dsp: &str, item: &Due) -> Result<Outcome> {
    let (at, team) = (dsp.to_owned(), item.team.clone());
    let (had, on) = state
        .read(move |db| {
            let had = kit::workspaces(db, &at)?
                .iter()
                .any(|kept| kept.team == team);
            Ok((had, kit::workspaces_on(db, &at, ID)?.contains(&team)))
        })
        .await?;
    // Tried again later each time, in case it's connected again, then given up.
    if !had {
        return Err(Error::new("slack_workspace_gone", 404));
    }
    if !on {
        return Ok(wait("communications_off"));
    }
    let bot = Bot::new(state, dsp, &item.team);
    let info = match bot
        .call("files.info", &json!({"file": item.slack_id}))
        .await
    {
        Err(error) if error.code == "slack_file_gone" => {
            return Ok(left(FileStatus::Unavailable, "slack_file_gone"));
        }
        answer => answer?,
    };
    let file = &info["file"];
    let (Some(url), true) = (
        file["url_private_download"].as_str(),
        file["mode"] == "hosted",
    ) else {
        return Ok(left(FileStatus::Unavailable, "slack_file_not_hosted"));
    };
    let size = file["size"].as_u64().unwrap_or(0);
    if size > UPLOAD_LIMIT {
        return Ok(left(FileStatus::TooLarge, "upload_too_large"));
    }
    let mime = file["mimetype"]
        .as_str()
        .unwrap_or("application/octet-stream");
    let bytes = bot.download(url, mime).await?;
    let bytes = Upload::streamed(size, UPLOAD_LIMIT, bytes).await?;
    Ok(Outcome::Kept(
        BYTES.receive(state, dsp, bytes, UPLOAD_LIMIT).await?,
    ))
}

/// Waits a while, uncounted, for `reason`.
fn wait(reason: &str) -> Outcome {
    Outcome::Later {
        reason: reason.to_owned(),
        at: now() + WAIT_MS,
        counted: false,
    }
}
fn left(status: FileStatus, reason: &str) -> Outcome {
    Outcome::Left {
        status,
        reason: reason.to_owned(),
    }
}
/// What `error` makes of trying `item`: it waits uncounted for an approval or the workspace to
/// work again, is tried again later each time, or is given up.
fn failure(item: &Due, error: &Error) -> Outcome {
    if [
        "slack_needs_update",
        "slack_connection_broken",
        "slack_not_connected",
    ]
    .contains(&error.code.as_str())
    {
        return wait(&error.code);
    }
    match RETRIES_MS.get(item.attempts.max(0) as usize) {
        Some(after) => Outcome::Later {
            reason: error.code.clone(),
            at: now() + after,
            counted: true,
        },
        None => left(FileStatus::Failed, &error.code),
    }
}

#[cfg(test)]
#[path = "../tests/backend/files.rs"]
mod tests;
