//! Slack messages and incoming edits, with their attachments and history queries.
//! Incoming persistence and its conversation/group settlement stay in one transaction.
use super::{conversations::opened, settlement::settle_with};
use crate::{
    api::types::MessageGif,
    backend::{changes::changed_conversation, gifs, model::Direction},
};
use dispatch_core::{
    Result,
    db::{Db, FromRow, Row, Store, now},
    foundation::crypto,
};
use rusqlite::params;

/// A message, as kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub id: String,
    pub conversation: String,
    /// Its ID in Slack, in its conversation, which orders its messages.
    pub ts: String,
    /// The thread it was written in, if any: its first message's `ts`.
    pub thread_ts: Option<String>,
    pub direction: Direction,
    /// Who at the DSP sent it, by their account, and the name it went out under.
    pub sent_by: Option<String>,
    pub sender_name: Option<String>,
    pub text: String,
    pub at: i64,
    /// When its text was last edited in Slack.
    pub edited_at: Option<i64>,
    /// For one the DSP sent, the group it went to; for one the member wrote, the group it
    /// answers: that of the DSP's latest message to them before it.
    pub group: Option<String>,
    /// The GIF one the DSP sent carries, if any.
    pub gif: Option<MessageGif>,
}
impl FromRow for Message {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            conversation: row.get("conversation_id")?,
            ts: row.get("ts")?,
            thread_ts: row.get("thread_ts")?,
            direction: row.get("direction")?,
            sent_by: row.get("sent_by")?,
            sender_name: row.get("sender_name")?,
            text: row.get("text")?,
            at: row.get("at")?,
            edited_at: row.get("edited_at")?,
            group: row.get("group_id")?,
            gif: gifs::of_column(row.get("gif")?)?,
        })
    }
}

/// A message a member wrote in their direct message with the app, as Slack has it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Written {
    pub team: String,
    pub user: String,
    pub channel: String,
    /// Their name, as their Slack profile gives it now.
    pub name: String,
    pub ts: String,
    pub thread_ts: Option<String>,
    pub text: String,
    pub at: i64,
    pub edited_at: Option<i64>,
    pub files: Vec<Attached>,
}
/// A file attached to a message in Slack, by Slack's ID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attached {
    pub slack_id: String,
    pub name: String,
    pub mime: String,
    pub size: i64,
}

/// A message someone at the DSP sent, as Slack took it: its `ts`, its text, who sent it under
/// what name, and its photos, as kept and as Slack has each.
pub struct Sent<'a> {
    pub ts: &'a str,
    pub text: &'a str,
    pub at: i64,
    pub by: &'a str,
    pub name: &'a str,
    pub photos: &'a [(String, String)],
    /// The GIF it carries in place of photos, if any.
    pub gif: Option<&'a MessageGif>,
}

/// What keeping a written message came to: its conversation, and the files to download, when
/// anything changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Kept {
    pub conversation: String,
    pub files: Vec<String>,
}

/// Slack message persistence and history.
pub trait MessageStore {
    /// Keeps a message a member wrote, with its files to download, or a later edit of one kept
    /// already: what changed, if anything did.
    fn keep_communications_message(&self, dsp: &str, written: &Written) -> Result<Option<Kept>>;
    /// The latest message of each conversation `ids` names.
    fn latest_communications_messages(&self, dsp: &str, ids: &[String]) -> Result<Vec<Message>>;
    /// The latest message the member wrote in each conversation `ids` names.
    fn latest_communications_written(&self, dsp: &str, ids: &[String]) -> Result<Vec<Message>>;
    /// The conversation's messages before the one Slack knows as `before`, the latest `most`,
    /// in the order they were written.
    fn communications_messages(
        &self,
        dsp: &str,
        conversation: &str,
        before: Option<&str>,
        most: usize,
    ) -> Result<Vec<Message>>;
    /// A complete bounded window at a stable position, or the latest window.
    fn communications_message_window(
        &self,
        dsp: &str,
        conversation: &str,
        anchor: Option<(&str, usize)>,
    ) -> Result<crate::backend::windows::Rows<Message>>;
    /// When the workspace's member `user`'s message `ts` was last edited, as kept: none when it
    /// isn't kept, and none within when it's kept unedited.
    fn communications_edited(
        &self,
        dsp: &str,
        team: &str,
        user: &str,
        ts: &str,
    ) -> Result<Option<Option<i64>>>;
}

impl MessageStore for Store {
    fn keep_communications_message(&self, dsp: &str, written: &Written) -> Result<Option<Kept>> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let conversation = opened(
                &db,
                &written.team,
                &written.user,
                &written.channel,
                &written.name,
            )?;
            let id = crypto::id("msg")?;
            let added = db.exec(
                "INSERT OR IGNORE INTO communications_messages (id,conversation_id,ts,thread_ts,\
                 direction,text,at,edited_at) VALUES (?,?,?,?,'in',?,?,?)",
                params![
                    id,
                    conversation,
                    written.ts,
                    written.thread_ts,
                    written.text,
                    written.at,
                    written.edited_at
                ],
            )?;
            if added == 0 {
                // Kept already: only a later edit changes it.
                let Some(edited_at) = written.edited_at else {
                    return Ok(None);
                };
                return Ok(
                    edited(&db, &conversation, &written.ts, &written.text, edited_at)?.then(|| {
                        Kept {
                            conversation,
                            files: Vec::new(),
                        }
                    }),
                );
            }
            let mut files = Vec::new();
            for attached in &written.files {
                let file = crypto::id("file")?;
                db.exec(
                    "INSERT INTO communications_files (id,message_id,slack_file_id,name,mime,\
                     size,status,added_at,due_at) VALUES (?,?,?,?,?,?,'waiting',?,?)",
                    params![
                        file,
                        id,
                        attached.slack_id,
                        attached.name,
                        attached.mime,
                        attached.size,
                        now(),
                        now()
                    ],
                )?;
                files.push(file);
            }
            // The group it answers, if the DSP's latest message to them before it went to one.
            let answers: Option<(Option<String>,)> = db.one_as(
                "SELECT group_id FROM communications_messages WHERE conversation_id=? \
                 AND direction='out' AND ts<? ORDER BY ts DESC LIMIT 1",
                [&conversation, &written.ts],
            )?;
            let answers = answers.and_then(|(group,)| group);
            db.exec(
                "UPDATE communications_messages SET group_id=? WHERE id=?",
                params![answers, id],
            )?;
            settle_with(&db, &conversation, answers.into_iter().collect())?;
            Ok(Some(Kept {
                conversation,
                files,
            }))
        })
    }
    fn latest_communications_messages(&self, dsp: &str, ids: &[String]) -> Result<Vec<Message>> {
        self.dsp(dsp)?.query_as(
            "SELECT * FROM communications_messages WHERE id IN (\
             SELECT (SELECT id FROM communications_messages WHERE conversation_id=requested.value \
             ORDER BY ts DESC LIMIT 1) FROM json_each(?) requested)",
            [serde_json::to_string(ids)?],
        )
    }
    fn latest_communications_written(&self, dsp: &str, ids: &[String]) -> Result<Vec<Message>> {
        self.dsp(dsp)?.query_as(
            "SELECT * FROM communications_messages WHERE id IN (\
             SELECT (SELECT id FROM communications_messages WHERE conversation_id=requested.value \
             AND direction='in' ORDER BY ts DESC LIMIT 1) FROM json_each(?) requested)",
            [serde_json::to_string(ids)?],
        )
    }
    fn communications_messages(
        &self,
        dsp: &str,
        conversation: &str,
        before: Option<&str>,
        most: usize,
    ) -> Result<Vec<Message>> {
        let mut messages: Vec<Message> = self.dsp(dsp)?.query_as(
            "SELECT * FROM communications_messages WHERE conversation_id=?1 \
             AND (?2 IS NULL OR ts<?2) ORDER BY ts DESC LIMIT ?3",
            params![conversation, before, most as i64],
        )?;
        messages.reverse();
        Ok(messages)
    }
    fn communications_message_window(
        &self,
        dsp: &str,
        conversation: &str,
        anchor: Option<(&str, usize)>,
    ) -> Result<crate::backend::windows::Rows<Message>> {
        use crate::backend::{model::WINDOW, windows::select};
        let db = self.dsp(dsp)?;
        let most = (WINDOW + 1) as i64;
        let Some((place, leading)) = anchor else {
            let left = db.query_as(
                "SELECT * FROM communications_messages WHERE conversation_id=? \
                 ORDER BY ts DESC LIMIT ?",
                params![conversation, most],
            )?;
            return Ok(select(left, Vec::new(), WINDOW));
        };
        let left = db.query_as(
            "SELECT * FROM communications_messages WHERE conversation_id=? AND ts<? \
             ORDER BY ts DESC LIMIT ?",
            params![conversation, place, most],
        )?;
        let right = db.query_as(
            "SELECT * FROM communications_messages WHERE conversation_id=? AND ts>=? \
             ORDER BY ts ASC LIMIT ?",
            params![conversation, place, most],
        )?;
        Ok(select(left, right, leading))
    }
    fn communications_edited(
        &self,
        dsp: &str,
        team: &str,
        user: &str,
        ts: &str,
    ) -> Result<Option<Option<i64>>> {
        let found: Option<(Option<i64>,)> = self.dsp(dsp)?.one_as(
            "SELECT m.edited_at FROM communications_messages m \
             JOIN communications_conversations c ON c.id=m.conversation_id \
             WHERE c.team_id=? AND c.user_id=? AND m.ts=?",
            [team, user, ts],
        )?;
        Ok(found.map(|(edited,)| edited))
    }
}

/// Gives the member's message `ts` in the conversation the text `text`, edited at `edited_at`,
/// unless it's kept with a later edit: whether it changed. Only its latest text is kept.
fn edited(db: &Db, conversation: &str, ts: &str, text: &str, edited_at: i64) -> Result<bool> {
    let changed = db.exec(
        "UPDATE communications_messages SET text=?,edited_at=? WHERE conversation_id=? AND ts=? \
         AND direction='in' AND coalesce(edited_at,0)<? AND text<>?",
        params![text, edited_at, conversation, ts, edited_at, text],
    )?;
    if changed > 0 {
        changed_conversation(db, conversation)?;
        // A group it answers reads it again too.
        let answers: Option<(Option<String>,)> = db.one_as(
            "SELECT group_id FROM communications_messages WHERE conversation_id=? AND ts=?",
            [conversation, ts],
        )?;
        if let Some((Some(group),)) = answers {
            changed_conversation(db, &group)?;
        }
    }
    Ok(changed > 0)
}

/// Keeps a message someone at the DSP sent in the conversation, alone or to the group `group`:
/// its ID, and the groups whose answers it changed. What the member wrote after it, as one
/// Slack told of while it went, answers it, and so its group, or none.
pub(crate) fn sent_out(
    db: &Db,
    conversation: &str,
    sent: &Sent,
    group: Option<&str>,
) -> Result<(String, Vec<String>)> {
    let id = crypto::id("msg")?;
    db.exec(
        "INSERT INTO communications_messages (id,conversation_id,ts,direction,sent_by,\
         sender_name,text,at,group_id,gif) VALUES (?,?,?,'out',?,?,?,?,?,?)",
        params![
            id,
            conversation,
            sent.ts,
            sent.by,
            sent.name,
            sent.text,
            sent.at,
            group,
            gifs::column(sent.gif)?
        ],
    )?;
    let after = "FROM communications_messages WHERE conversation_id=?1 AND direction='in' \
                 AND ts>?2 AND group_id IS NOT ?3";
    let mut touched: Vec<String> = db
        .query_as::<(String,)>(
            &format!("SELECT DISTINCT group_id {after} AND group_id IS NOT NULL"),
            params![conversation, sent.ts, group],
        )?
        .into_iter()
        .map(|(group,)| group)
        .collect();
    db.exec(
        &format!("UPDATE communications_messages SET group_id=?3 WHERE id IN (SELECT id {after})"),
        params![conversation, sent.ts, group],
    )?;
    touched.extend(group.map(str::to_owned));
    Ok((id, touched))
}
