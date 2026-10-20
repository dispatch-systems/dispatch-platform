//! Communications' storage, in each DSP's own database: each conversation with a member of one of
//! its Slack workspaces, its messages, and each change to them in order, for the pages waiting
//! on them. Whether a conversation waits on an answer follows from its messages, and is kept
//! beside them each time they change. The photos and files attached are `files`'. Only this
//! crate writes SQL against its tables.
use dispatch_core::{
    Error, Result,
    db::{Db, FromRow, Row, Store, now},
    foundation::crypto,
    server::cache::DataDomain,
    text_enum,
};
use rusqlite::params;

/// What Communications' writes change, for the read cache: its own data alone.
pub const DOMAIN: DataDomain = DataDomain::new("communications");
/// How many changes are kept for the pages waiting on them: one further behind reads afresh.
const CHANGES_KEPT: i64 = 1000;

text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/communications/api/generated/"))]
    /// Whether a conversation waits on an answer: the member's latest message came after
    /// anyone at the DSP answered, or marked it done; someone at the DSP answered last; or
    /// marked it done without answering.
    pub enum ConversationState {
        Waiting => "waiting",
        Responded => "responded",
        Done => "done",
    }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/communications/api/generated/"))]
    /// Who wrote a message: the member (`in`), or someone at the DSP (`out`).
    pub enum Direction {
        In => "in",
        Out => "out",
    }
}

/// A conversation with a member of one of the DSP's workspaces, as kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conversation {
    pub id: String,
    pub team: String,
    /// The member, by their ID in the workspace.
    pub user: String,
    /// The direct message the app has with them, once known.
    pub channel: Option<String>,
    /// Their name, as their Slack profile last gave it.
    pub name: String,
    pub state: ConversationState,
    /// Since when it waits: the first of the member's messages since it was last answered.
    pub waiting_since: Option<i64>,
    /// When its latest message was written; none before its first.
    pub last_at: Option<i64>,
    /// Up to when catching up has read its messages.
    pub caught_up_at: Option<i64>,
}
impl FromRow for Conversation {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            team: row.get("team_id")?,
            user: row.get("user_id")?,
            channel: row.get("channel_id")?,
            name: row.get("name")?,
            state: row.get("state")?,
            waiting_since: row.get("waiting_since")?,
            last_at: row.get("last_at")?,
            caught_up_at: row.get("caught_up_at")?,
        })
    }
}

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
/// what name, and the photos they added, as kept and as Slack has each.
pub struct Sent<'a> {
    pub ts: &'a str,
    pub text: &'a str,
    pub at: i64,
    pub by: &'a str,
    pub name: &'a str,
    pub photos: &'a [(String, String)],
}

/// What keeping a written message came to: its conversation, and the files to download, when
/// anything changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Kept {
    pub conversation: String,
    pub files: Vec<String>,
}

/// What Communications reads and writes of its conversations and messages.
pub trait CommunicationsStore {
    fn communications_conversation(&self, dsp: &str, id: &str) -> Result<Option<Conversation>>;
    /// The conversation with the workspace's member `user`, if there is one.
    fn communications_conversation_with(
        &self,
        dsp: &str,
        team: &str,
        user: &str,
    ) -> Result<Option<Conversation>>;
    /// The conversation with the workspace's member `user`, in their direct message `channel`
    /// with the app, under the name `name`: made with no message if it's new.
    fn open_communications_conversation(
        &self,
        dsp: &str,
        team: &str,
        user: &str,
        channel: &str,
        name: &str,
    ) -> Result<Conversation>;
    /// Keeps a message a member wrote, with its files to download, or a later edit of one kept
    /// already: what changed, if anything did.
    fn keep_communications_message(&self, dsp: &str, written: &Written) -> Result<Option<Kept>>;
    /// Edits the text of the message `ts` the workspace's member `user` wrote, edited at
    /// `edited_at`, unless it's kept with a later edit: its conversation, if it changed.
    fn edit_communications_message(
        &self,
        dsp: &str,
        team: &str,
        user: &str,
        ts: &str,
        text: &str,
        edited_at: i64,
    ) -> Result<Option<String>>;
    /// Keeps a message someone at the DSP sent in the conversation, with the photos they
    /// added, which become its own.
    fn keep_communications_reply(
        &self,
        dsp: &str,
        conversation: &str,
        sent: &Sent,
    ) -> Result<Message>;
    /// Marks the conversation done without an answer, as the member `by`, while it waits:
    /// whether it was waiting.
    fn mark_communications_done(&self, dsp: &str, conversation: &str, by: &str) -> Result<bool>;
    /// The conversations waiting, the longest waiting first.
    fn waiting_communications(&self, dsp: &str) -> Result<Vec<Conversation>>;
    /// The conversations answered or done, the latest first: those before `before`, its
    /// latest message's time and its ID, `most` at most.
    fn answered_communications(
        &self,
        dsp: &str,
        before: Option<(i64, &str)>,
        most: usize,
    ) -> Result<Vec<Conversation>>;
    /// Every conversation with a message, the latest first.
    fn communications_with_messages(&self, dsp: &str) -> Result<Vec<Conversation>>;
    /// The conversations `ids` name, in no order.
    fn communications_conversations(&self, dsp: &str, ids: &[String]) -> Result<Vec<Conversation>>;
    /// The latest message of each conversation `ids` names.
    fn latest_communications_messages(&self, dsp: &str, ids: &[String]) -> Result<Vec<Message>>;
    /// The conversation's messages before the one Slack knows as `before`, the latest `most`,
    /// in the order they were written.
    fn communications_messages(
        &self,
        dsp: &str,
        conversation: &str,
        before: Option<&str>,
        most: usize,
    ) -> Result<Vec<Message>>;
    /// The latest change to any conversation: 0 before the first.
    fn communications_revision(&self, dsp: &str) -> Result<i64>;
    /// The conversations changed since the change `after`, and the latest change: none for
    /// changes no longer kept, which reading afresh answers.
    fn communications_changed_since(
        &self,
        dsp: &str,
        after: i64,
    ) -> Result<(Option<Vec<String>>, i64)>;
    /// The workspace's conversations whose direct message is known.
    fn communications_in(&self, dsp: &str, team: &str) -> Result<Vec<Conversation>>;
    /// Notes that catching up read the conversation's messages up to `at`.
    fn caught_up_communications(&self, dsp: &str, conversation: &str, at: i64) -> Result<()>;
    /// Clears what's kept of the workspace beside its messages: each conversation with none.
    fn forget_empty_communications(&self, dsp: &str, team: &str) -> Result<()>;
}

impl CommunicationsStore for Store {
    fn communications_conversation(&self, dsp: &str, id: &str) -> Result<Option<Conversation>> {
        self.dsp(dsp)?.one_as(
            "SELECT * FROM communications_conversations WHERE id=?",
            [id],
        )
    }
    fn communications_conversation_with(
        &self,
        dsp: &str,
        team: &str,
        user: &str,
    ) -> Result<Option<Conversation>> {
        self.dsp(dsp)?.one_as(
            "SELECT * FROM communications_conversations WHERE team_id=? AND user_id=?",
            [team, user],
        )
    }
    fn open_communications_conversation(
        &self,
        dsp: &str,
        team: &str,
        user: &str,
        channel: &str,
        name: &str,
    ) -> Result<Conversation> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let id = opened(&db, team, user, channel, name)?;
            db.one_as(
                "SELECT * FROM communications_conversations WHERE id=?",
                [&id],
            )?
            .ok_or_else(|| Error::new("communications_conversation_not_found", 404))
        })
    }
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
            settle(&db, &conversation)?;
            Ok(Some(Kept {
                conversation,
                files,
            }))
        })
    }
    fn edit_communications_message(
        &self,
        dsp: &str,
        team: &str,
        user: &str,
        ts: &str,
        text: &str,
        edited_at: i64,
    ) -> Result<Option<String>> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let Some((conversation,)): Option<(String,)> = db.one_as(
                "SELECT id FROM communications_conversations WHERE team_id=? AND user_id=?",
                [team, user],
            )?
            else {
                return Ok(None);
            };
            Ok(edited(&db, &conversation, ts, text, edited_at)?.then_some(conversation))
        })
    }
    fn keep_communications_reply(
        &self,
        dsp: &str,
        conversation: &str,
        sent: &Sent,
    ) -> Result<Message> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let id = crypto::id("msg")?;
            db.exec(
                "INSERT INTO communications_messages (id,conversation_id,ts,direction,sent_by,\
                 sender_name,text,at) VALUES (?,?,?,'out',?,?,?,?)",
                params![
                    id,
                    conversation,
                    sent.ts,
                    sent.by,
                    sent.name,
                    sent.text,
                    sent.at
                ],
            )?;
            for (photo, slack_id) in sent.photos {
                db.exec(
                    "UPDATE communications_files SET message_id=?,slack_file_id=? \
                     WHERE id=? AND message_id IS NULL",
                    params![id, slack_id, photo],
                )?;
            }
            settle(&db, conversation)?;
            db.one_as("SELECT * FROM communications_messages WHERE id=?", [&id])?
                .ok_or_else(|| Error::new("communications_message_not_found", 404))
        })
    }
    fn mark_communications_done(&self, dsp: &str, conversation: &str, by: &str) -> Result<bool> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let marked = db.exec(
                "UPDATE communications_conversations SET done_at=?,done_by=? \
                 WHERE id=? AND state='waiting'",
                params![now(), by, conversation],
            )?;
            if marked > 0 {
                settle(&db, conversation)?;
            }
            Ok(marked > 0)
        })
    }
    fn waiting_communications(&self, dsp: &str) -> Result<Vec<Conversation>> {
        self.dsp(dsp)?.query_as(
            "SELECT * FROM communications_conversations WHERE state='waiting' \
             ORDER BY waiting_since,id",
            [],
        )
    }
    fn answered_communications(
        &self,
        dsp: &str,
        before: Option<(i64, &str)>,
        most: usize,
    ) -> Result<Vec<Conversation>> {
        let (at, id) = before.unwrap_or((i64::MAX, "\u{10FFFF}"));
        self.dsp(dsp)?.query_as(
            "SELECT * FROM communications_conversations WHERE state<>'waiting' \
             AND last_at IS NOT NULL AND (last_at<?1 OR (last_at=?1 AND id<?2)) \
             ORDER BY last_at DESC,id DESC LIMIT ?3",
            params![at, id, most as i64],
        )
    }
    fn communications_with_messages(&self, dsp: &str) -> Result<Vec<Conversation>> {
        self.dsp(dsp)?.query_as(
            "SELECT * FROM communications_conversations WHERE last_at IS NOT NULL \
             ORDER BY last_at DESC,id DESC",
            [],
        )
    }
    fn communications_conversations(&self, dsp: &str, ids: &[String]) -> Result<Vec<Conversation>> {
        self.dsp(dsp)?.query_as(
            "SELECT * FROM communications_conversations \
             WHERE id IN (SELECT value FROM json_each(?))",
            [serde_json::to_string(ids)?],
        )
    }
    fn latest_communications_messages(&self, dsp: &str, ids: &[String]) -> Result<Vec<Message>> {
        self.dsp(dsp)?.query_as(
            "SELECT m.* FROM communications_messages m \
             WHERE m.conversation_id IN (SELECT value FROM json_each(?)) AND m.id=(\
             SELECT id FROM communications_messages WHERE conversation_id=m.conversation_id \
             ORDER BY ts DESC LIMIT 1)",
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
    fn communications_revision(&self, dsp: &str) -> Result<i64> {
        let db = self.dsp(dsp)?;
        revision(&db)
    }
    fn communications_changed_since(
        &self,
        dsp: &str,
        after: i64,
    ) -> Result<(Option<Vec<String>>, i64)> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let latest = revision(&db)?;
            let first = db.count(
                "SELECT coalesce(min(revision),0) FROM communications_changes",
                [],
            )?;
            // Changes since one no longer kept, or one never made, are read afresh.
            if after > latest || after < first - 1 {
                return Ok((None, latest));
            }
            let changed: Vec<(String,)> = db.query_as(
                "SELECT DISTINCT conversation_id FROM communications_changes WHERE revision>?",
                [after],
            )?;
            Ok((Some(changed.into_iter().map(|(id,)| id).collect()), latest))
        })
    }
    fn communications_in(&self, dsp: &str, team: &str) -> Result<Vec<Conversation>> {
        self.dsp(dsp)?.query_as(
            "SELECT * FROM communications_conversations WHERE team_id=? \
             AND channel_id IS NOT NULL ORDER BY id",
            [team],
        )
    }
    fn caught_up_communications(&self, dsp: &str, conversation: &str, at: i64) -> Result<()> {
        self.dsp(dsp)?.exec(
            "UPDATE communications_conversations SET caught_up_at=max(coalesce(caught_up_at,0),?) \
             WHERE id=?",
            params![at, conversation],
        )?;
        Ok(())
    }
    fn forget_empty_communications(&self, dsp: &str, team: &str) -> Result<()> {
        self.dsp(dsp)?.exec(
            "DELETE FROM communications_conversations WHERE team_id=? AND last_at IS NULL",
            [team],
        )?;
        Ok(())
    }
}

/// The conversation with the workspace's member `user`, made if it's new, with their direct
/// message `channel` and the name `name`: its ID.
fn opened(db: &Db, team: &str, user: &str, channel: &str, name: &str) -> Result<String> {
    db.exec(
        "INSERT INTO communications_conversations (id,team_id,user_id,channel_id,name) \
         VALUES (?,?,?,?,?) ON CONFLICT(team_id,user_id) DO UPDATE SET \
         channel_id=excluded.channel_id,name=excluded.name",
        params![crypto::id("cv")?, team, user, channel, name],
    )?;
    let (id,): (String,) = db
        .one_as(
            "SELECT id FROM communications_conversations WHERE team_id=? AND user_id=?",
            [team, user],
        )?
        .ok_or_else(|| Error::new("communications_conversation_not_found", 404))?;
    Ok(id)
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
    }
    Ok(changed > 0)
}

/// Keeps whether the conversation waits on an answer, and since when, from its messages and
/// when it was marked done, and notes it changed. It waits while the member's latest message
/// came after anyone at the DSP answered or marked it done: since the first of theirs after
/// that.
fn settle(db: &Db, conversation: &str) -> Result<()> {
    let (last_in, last_out, done_at): (Option<i64>, Option<i64>, Option<i64>) = db
        .one_as(
            "SELECT (SELECT max(at) FROM communications_messages \
             WHERE conversation_id=?1 AND direction='in'),\
             (SELECT max(at) FROM communications_messages \
             WHERE conversation_id=?1 AND direction='out'),\
             (SELECT done_at FROM communications_conversations WHERE id=?1)",
            [conversation],
        )?
        .unwrap_or_default();
    let answered = last_out.max(done_at);
    let waiting = last_in.is_some() && last_in > answered;
    let state = if waiting {
        ConversationState::Waiting
    } else if done_at > last_out {
        ConversationState::Done
    } else {
        ConversationState::Responded
    };
    db.exec(
        "UPDATE communications_conversations SET state=?2,\
         waiting_since=CASE WHEN ?3 THEN (SELECT min(at) FROM communications_messages \
         WHERE conversation_id=?1 AND direction='in' AND at>coalesce(?4,-1)) END,\
         last_at=(SELECT max(at) FROM communications_messages WHERE conversation_id=?1) \
         WHERE id=?1",
        params![conversation, state, waiting, answered],
    )?;
    changed_conversation(db, conversation)
}

/// Notes the conversation changed, for the pages waiting on changes, keeping the latest
/// thousand.
pub(crate) fn changed_conversation(db: &Db, conversation: &str) -> Result<()> {
    db.exec(
        "INSERT INTO communications_changes (conversation_id) VALUES (?)",
        [conversation],
    )?;
    db.exec(
        "DELETE FROM communications_changes WHERE revision<=\
         (SELECT max(revision) FROM communications_changes)-?",
        [CHANGES_KEPT],
    )?;
    Ok(())
}

fn revision(db: &Db) -> Result<i64> {
    db.count(
        "SELECT coalesce(max(revision),0) FROM communications_changes",
        [],
    )
}

#[cfg(test)]
#[path = "../tests/backend/storage.rs"]
mod tests;
