//! Communications' storage, in each DSP's own database: each conversation with a member of one of
//! its Slack workspaces, and its messages, each change to them noted for the pages waiting on
//! them (`changes`). Whether a conversation waits on an answer follows from its messages, and is
//! kept beside them each time they change. The photos and files attached are `files`'. Only this
//! crate writes SQL against its tables.
use super::{
    changes::changed_conversation,
    groups::storage::settle_group,
    model::{ConversationState, Direction, SLACK_PREFIX},
};
use dispatch_core::{
    Error, Result,
    db::{Db, FromRow, Row, Store, migrations::add_column, now},
    foundation::crypto,
    server::cache::DataDomain,
};
use rusqlite::params;

/// What Communications' writes change, for the read cache: its own data alone.
pub const DOMAIN: DataDomain = DataDomain::new("communications");

/// A conversation with a member of one of the DSP's workspaces, as kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conversation {
    pub id: String,
    pub team: String,
    /// The member, by their ID in the workspace.
    pub user: String,
    /// The direct message the app has with them.
    pub channel: String,
    /// Their name, as their Slack profile last gave it.
    pub name: String,
    pub state: ConversationState,
    /// Since when it waits: the first of the member's messages since it was last answered.
    pub waiting_since: Option<i64>,
    /// When its latest message was written; none before its first.
    pub last_at: Option<i64>,
    /// Up to when catching up has read its messages.
    pub caught_up_at: Option<i64>,
    /// Where catching up goes on from, with more since `caught_up_at` than a pass reads: up to
    /// when it will have read once it's back down there, and the oldest message it read, by its
    /// `ts`.
    pub catching_up: Option<(i64, String)>,
    /// The group it's listed under: the one the DSP last wrote to them in, while all they wait on
    /// answers a message to a group.
    pub group: Option<String>,
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
            catching_up: match (row.get("catching_up_to")?, row.get("catching_up_before")?) {
                (Some(to), Some(before)) => Some((to, before)),
                _ => None,
            },
            group: row.get("group_id")?,
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
    /// For one the DSP sent, the group it went to; for one the member wrote, the group it
    /// answers: that of the DSP's latest message to them before it.
    pub group: Option<String>,
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
}

/// An answer on its way to Slack, noted before it's posted: the ID its post carries, its
/// conversation, its text, who sent it under what name, its photos, as kept and as Slack has each,
/// from when it may have been posted, and until when the try posting it holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Noted {
    pub id: String,
    pub conversation: String,
    pub text: String,
    pub by: String,
    pub name: String,
    pub photos: Vec<(String, String)>,
    pub posting_since: i64,
    pub claimed_until: i64,
}
impl FromRow for Noted {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            conversation: row.get("conversation_id")?,
            text: row.get("text")?,
            by: row.get("sent_by")?,
            name: row.get("sender_name")?,
            photos: serde_json::from_str(&row.get::<String>("photos")?)?,
            posting_since: row.get("posting_since")?,
            claimed_until: row.get("claimed_until")?,
        })
    }
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
    /// Notes an answer before it goes to Slack, or, noted already, its try again: its photos, the
    /// name it goes under and its claim, from when it may have been posted kept as it was.
    fn note_communications_reply(&self, dsp: &str, noted: &Noted) -> Result<()>;
    /// The answers noted in the conversation `conversation`, or in every one, no try holding them
    /// at `at`, the earliest first.
    fn noted_communications_replies(
        &self,
        dsp: &str,
        conversation: Option<&str>,
        at: i64,
    ) -> Result<Vec<Noted>>;
    /// Clears the answer noted as `id`: it's not in Slack.
    fn clear_communications_reply(&self, dsp: &str, id: &str) -> Result<()>;
    /// Keeps the answer noted as `noted` in the conversation as Slack took it, `sent`, with the
    /// photos added for it, which become its own, and clears its note. Kept already, by its `ts`,
    /// it's that one.
    fn keep_communications_reply(
        &self,
        dsp: &str,
        conversation: &str,
        sent: &Sent,
        noted: &str,
    ) -> Result<Message>;
    /// Marks the conversation done without an answer, as the member `by`, while it waits, covering
    /// what was written up to `seen`, the latest message the member was shown, when it was written
    /// and its `ts`, else up to now: whether it was waiting.
    fn mark_communications_done(
        &self,
        dsp: &str,
        conversation: &str,
        by: &str,
        seen: Option<(i64, &str)>,
    ) -> Result<bool>;
    /// The conversations waiting, the longest waiting first.
    fn waiting_communications(&self, dsp: &str) -> Result<Vec<Conversation>>;
    /// Every conversation with a message, the latest first.
    fn communications_with_messages(&self, dsp: &str) -> Result<Vec<Conversation>>;
    /// The conversations `ids` name, in no order.
    fn communications_conversations(&self, dsp: &str, ids: &[String]) -> Result<Vec<Conversation>>;
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
    /// The workspace's conversations.
    fn communications_in(&self, dsp: &str, team: &str) -> Result<Vec<Conversation>>;
    /// Notes that catching up read the conversation's messages up to `at`.
    fn caught_up_communications(&self, dsp: &str, conversation: &str, at: i64) -> Result<()>;
    /// Notes that catching up read the conversation's messages down to the one Slack knows as
    /// `before`, of those up to `to`, and goes on from there.
    fn catching_up_communications(
        &self,
        dsp: &str,
        conversation: &str,
        to: i64,
        before: &str,
    ) -> Result<()>;
    /// When the workspace's member `user`'s message `ts` was last edited, as kept: none when it
    /// isn't kept, and none within when it's kept unedited.
    fn communications_edited(
        &self,
        dsp: &str,
        team: &str,
        user: &str,
        ts: &str,
    ) -> Result<Option<Option<i64>>>;
    /// Clears what's kept of the workspace beside its messages: each conversation with none, but
    /// one with an answer noted on its way to Slack.
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
    fn note_communications_reply(&self, dsp: &str, noted: &Noted) -> Result<()> {
        self.dsp(dsp)?.exec(
            "INSERT INTO communications_replies (id,conversation_id,text,sent_by,sender_name,\
             photos,posting_since,claimed_until) VALUES (?,?,?,?,?,?,?,?) \
             ON CONFLICT(id) DO UPDATE SET sender_name=excluded.sender_name,\
             photos=excluded.photos,claimed_until=excluded.claimed_until",
            params![
                noted.id,
                noted.conversation,
                noted.text,
                noted.by,
                noted.name,
                serde_json::to_string(&noted.photos)?,
                noted.posting_since,
                noted.claimed_until
            ],
        )?;
        Ok(())
    }
    fn noted_communications_replies(
        &self,
        dsp: &str,
        conversation: Option<&str>,
        at: i64,
    ) -> Result<Vec<Noted>> {
        self.dsp(dsp)?.query_as(
            "SELECT * FROM communications_replies WHERE claimed_until<=?1 \
             AND (?2 IS NULL OR conversation_id=?2) ORDER BY posting_since,id",
            params![at, conversation],
        )
    }
    fn clear_communications_reply(&self, dsp: &str, id: &str) -> Result<()> {
        self.dsp(dsp)?
            .exec("DELETE FROM communications_replies WHERE id=?", [id])?;
        Ok(())
    }
    fn keep_communications_reply(
        &self,
        dsp: &str,
        conversation: &str,
        sent: &Sent,
        noted: &str,
    ) -> Result<Message> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            db.exec("DELETE FROM communications_replies WHERE id=?", [noted])?;
            let kept: Option<Message> = db.one_as(
                "SELECT * FROM communications_messages WHERE conversation_id=? AND ts=?",
                [conversation, sent.ts],
            )?;
            if let Some(kept) = kept {
                return Ok(kept);
            }
            let (id, touched) = sent_out(&db, conversation, sent, None)?;
            for (photo, slack_id) in sent.photos {
                db.exec(
                    "UPDATE communications_files SET message_id=?,slack_file_id=?,\
                     claimed_until=NULL WHERE id=? AND message_id IS NULL",
                    params![id, slack_id, photo],
                )?;
            }
            settle_with(&db, conversation, touched)?;
            db.one_as("SELECT * FROM communications_messages WHERE id=?", [&id])?
                .ok_or_else(|| Error::new("communications_message_not_found", 404))
        })
    }
    fn mark_communications_done(
        &self,
        dsp: &str,
        conversation: &str,
        by: &str,
        seen: Option<(i64, &str)>,
    ) -> Result<bool> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let waiting = db.count(
                "SELECT count(*) FROM communications_conversations WHERE id=? AND state='waiting'",
                [conversation],
            )? > 0;
            if waiting {
                mark_done(&db, conversation, by, seen)?;
                settle(&db, conversation)?;
            }
            Ok(waiting)
        })
    }
    fn waiting_communications(&self, dsp: &str) -> Result<Vec<Conversation>> {
        self.dsp(dsp)?.query_as(
            "SELECT * FROM communications_conversations WHERE state='waiting' \
             ORDER BY waiting_since,id",
            [],
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
    fn latest_communications_written(&self, dsp: &str, ids: &[String]) -> Result<Vec<Message>> {
        self.dsp(dsp)?.query_as(
            "SELECT m.* FROM communications_messages m \
             WHERE m.conversation_id IN (SELECT value FROM json_each(?)) AND m.id=(\
             SELECT id FROM communications_messages WHERE conversation_id=m.conversation_id \
             AND direction='in' ORDER BY ts DESC LIMIT 1)",
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
    fn communications_in(&self, dsp: &str, team: &str) -> Result<Vec<Conversation>> {
        self.dsp(dsp)?.query_as(
            "SELECT * FROM communications_conversations WHERE team_id=? ORDER BY id",
            [team],
        )
    }
    fn caught_up_communications(&self, dsp: &str, conversation: &str, at: i64) -> Result<()> {
        self.dsp(dsp)?.exec(
            "UPDATE communications_conversations SET caught_up_at=max(coalesce(caught_up_at,0),?),\
             catching_up_to=NULL,catching_up_before=NULL WHERE id=?",
            params![at, conversation],
        )?;
        Ok(())
    }
    fn catching_up_communications(
        &self,
        dsp: &str,
        conversation: &str,
        to: i64,
        before: &str,
    ) -> Result<()> {
        self.dsp(dsp)?.exec(
            "UPDATE communications_conversations SET catching_up_to=?,catching_up_before=? \
             WHERE id=?",
            params![to, before, conversation],
        )?;
        Ok(())
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
    fn forget_empty_communications(&self, dsp: &str, team: &str) -> Result<()> {
        self.dsp(dsp)?.exec(
            "DELETE FROM communications_conversations WHERE team_id=? AND last_at IS NULL \
             AND id NOT IN (SELECT conversation_id FROM communications_replies)",
            [team],
        )?;
        Ok(())
    }
}

/// The conversation with the workspace's member `user`, made if it's new, with their direct
/// message `channel` and the name `name`: its ID.
pub(crate) fn opened(db: &Db, team: &str, user: &str, channel: &str, name: &str) -> Result<String> {
    db.exec(
        "INSERT INTO communications_conversations (id,team_id,user_id,channel_id,name) \
         VALUES (?,?,?,?,?) ON CONFLICT(team_id,user_id) DO UPDATE SET \
         channel_id=excluded.channel_id,name=excluded.name",
        params![crypto::id(SLACK_PREFIX)?, team, user, channel, name],
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
         sender_name,text,at,group_id) VALUES (?,?,?,'out',?,?,?,?,?)",
        params![
            id,
            conversation,
            sent.ts,
            sent.by,
            sent.name,
            sent.text,
            sent.at,
            group
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

/// Marks the conversation done as the member `by`, covering what was written up to `seen`, the
/// latest message they were shown, when it was written and its `ts`, else up to now: never less
/// than a Done before covered. A Done covers its millisecond (`done_at`) up to its `ts`
/// (`done_ts`), so one of the member's in that millisecond after it, kept before or after, still
/// waits; one with no `ts` covers all of it.
pub(crate) fn mark_done(
    db: &Db,
    conversation: &str,
    by: &str,
    seen: Option<(i64, &str)>,
) -> Result<()> {
    let at = now();
    let (until, ts) = match seen {
        Some((seen, ts)) if seen < at => (seen, Some(ts.to_owned())),
        _ => (at, None),
    };
    let (done_at, done_ts): (Option<i64>, Option<String>) = db
        .one_as(
            "SELECT done_at,done_ts FROM communications_conversations WHERE id=?",
            [conversation],
        )?
        .ok_or_else(|| Error::new("communications_conversation_not_found", 404))?;
    // How far a Done covers: to its millisecond, then all of it, else up to its `ts` in it.
    let reach = |at: Option<i64>, ts: &Option<String>| (at, ts.is_none(), ts.clone());
    if reach(Some(until), &ts) <= reach(done_at, &done_ts) {
        return Ok(());
    }
    db.exec(
        "UPDATE communications_conversations SET done_at=?,done_ts=?,done_by=? WHERE id=?",
        params![until, ts, by, conversation],
    )?;
    Ok(())
}
/// Keeps whether the conversation waits on an answer, and since when, from its messages and
/// when it was marked done, and the group it's listed under, and notes it changed, with each
/// group that changed with it. It waits while the member's latest message came after anyone at
/// the DSP answered them alone or marked it done: since the first of theirs after that. A
/// message to a group answers no one. It's listed under the group the DSP's latest message to
/// them went to, while all it waits on answers a message to a group: nothing they wrote before
/// one, since the DSP last wrote to them alone, still waits.
pub(crate) fn settle(db: &Db, conversation: &str) -> Result<()> {
    settle_with(db, conversation, Vec::new())
}
/// `settle`, with the groups `touched` settled again too, as those whose answers changed.
pub(crate) fn settle_with(db: &Db, conversation: &str, mut touched: Vec<String>) -> Result<()> {
    let (last_alone, done_at, was): (Option<i64>, Option<i64>, Option<String>) = db
        .one_as(
            "SELECT (SELECT max(at) FROM communications_messages \
             WHERE conversation_id=?1 AND direction='out' AND group_id IS NULL),\
             done_at,group_id FROM communications_conversations WHERE id=?1",
            [conversation],
        )?
        .ok_or_else(|| Error::new("communications_conversation_not_found", 404))?;
    // What it waits on, if it waits: the first of the member's after the latest answer that the
    // latest Done didn't cover (`mark_done`), and whether it answers a group.
    let first: Option<(i64, Option<String>)> = db.one_as(
        "SELECT m.at,m.group_id FROM communications_messages m \
         JOIN communications_conversations c ON c.id=m.conversation_id \
         WHERE m.conversation_id=?1 AND m.direction='in' AND m.at>?2 \
         AND (m.at>coalesce(c.done_at,-1) OR (m.at=c.done_at AND m.ts>c.done_ts)) \
         ORDER BY m.at,m.ts LIMIT 1",
        params![conversation, last_alone.unwrap_or(-1)],
    )?;
    let state = if first.is_some() {
        ConversationState::Waiting
    } else if done_at > last_alone {
        ConversationState::Done
    } else {
        ConversationState::Responded
    };
    let since = first.as_ref().map(|(at, _)| *at);
    // Under the group the DSP last wrote to them in, while all it waits on answers a group.
    let latest: Option<(Option<String>,)> = db.one_as(
        "SELECT group_id FROM communications_messages WHERE conversation_id=? \
         AND direction='out' ORDER BY ts DESC LIMIT 1",
        [conversation],
    )?;
    let group = latest
        .and_then(|(group,)| group)
        .filter(|_| first.as_ref().is_none_or(|(_, answers)| answers.is_some()));
    db.exec(
        "UPDATE communications_conversations SET state=?2,waiting_since=?3,group_id=?4,\
         last_at=(SELECT max(at) FROM communications_messages WHERE conversation_id=?1) \
         WHERE id=?1",
        params![conversation, state, since, group],
    )?;
    changed_conversation(db, conversation)?;
    touched.extend(was.into_iter().chain(group));
    touched.sort();
    touched.dedup();
    for group in touched {
        settle_group(db, &group)?;
    }
    Ok(())
}

/// Communications' migration 13: how far within its millisecond each conversation's latest Done
/// covered (`done_ts`, the `ts` of the latest message it covered), none for one before, which
/// covered all of it.
pub(crate) fn add_done_places(db: &Db) -> Result<()> {
    add_column(db, "communications_conversations", "done_ts", "TEXT")
}

/// Communications' migration 3: what ties conversations, messages and photos to groups. A
/// conversation's group is the one it's listed under (`group_id`); a message's, the group it
/// went to or answers; a photo's, the group message it was sent with (`group_message_id`).
pub(crate) fn add_groups(db: &Db) -> Result<()> {
    add_column(db, "communications_conversations", "group_id", "TEXT")?;
    add_column(db, "communications_messages", "group_id", "TEXT")?;
    add_column(db, "communications_files", "group_message_id", "TEXT")?;
    db.exec(
        "CREATE INDEX IF NOT EXISTS communications_conversations_groups \
         ON communications_conversations (group_id, state)",
        [],
    )?;
    db.exec(
        "CREATE INDEX IF NOT EXISTS communications_messages_groups \
         ON communications_messages (group_id, at)",
        [],
    )?;
    db.exec(
        "CREATE INDEX IF NOT EXISTS communications_files_group_messages \
         ON communications_files (group_message_id)",
        [],
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "../tests/backend/storage.rs"]
mod tests;
