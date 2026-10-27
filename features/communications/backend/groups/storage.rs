//! What Communications keeps of its groups, in each DSP's own database: each group, whom it went
//! to, each message the DSP sent it, and each of those going to each of its people. Whether a
//! group waits follows from the conversations listed under it, and is kept beside it each time
//! one of them changes. Only this crate writes SQL against its tables.
use crate::backend::{
    changes::changed_conversation,
    model::ConversationState,
    storage::{Message, Sent, opened, sent_out, settle, settle_with},
};
use dispatch_core::{
    Error, Result,
    db::{Db, FromRow, Row, Store, now},
    foundation::crypto,
    text_enum,
};
use rusqlite::params;

/// A group, as kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub id: String,
    pub name: String,
    /// Whether any conversation listed under it waits on an answer, and since when the longest
    /// has.
    pub state: ConversationState,
    pub waiting_since: Option<i64>,
    /// When it was last written in: the DSP's latest message to it, or the latest answer.
    pub last_at: i64,
}
impl FromRow for Group {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            name: row.get("name")?,
            state: row.get("state")?,
            waiting_since: row.get("waiting_since")?,
            last_at: row.get("last_at")?,
        })
    }
}

/// One of a group's people: the member of a workspace it went to, under the name their Slack
/// profile gave them then.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    pub team: String,
    pub user: String,
    pub name: String,
}
impl FromRow for Member {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            team: row.get("team_id")?,
            user: row.get("user_id")?,
            name: row.get("name")?,
        })
    }
}

/// Who a group is, as its row shows it: how many people, its first few, and the workspaces its
/// people are in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outline {
    pub group: String,
    pub people: i64,
    pub faces: Vec<Member>,
    pub teams: Vec<String>,
}

/// A message someone at the DSP sent a group, as kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupMessage {
    pub id: String,
    pub group: String,
    pub text: String,
    /// Who sent it, by their account, the name it goes out under, and the letters of their
    /// initials.
    pub sent_by: String,
    pub sender_name: String,
    pub sender_initials: Option<String>,
    pub at: i64,
}
impl FromRow for GroupMessage {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            group: row.get("group_id")?,
            text: row.get("text")?,
            sent_by: row.get("sent_by")?,
            sender_name: row.get("sender_name")?,
            sender_initials: row.get("sender_initials")?,
            at: row.get("at")?,
        })
    }
}

/// A message to a group being written: what, by whom, and the photos its writer added for it.
pub struct Writing<'a> {
    pub text: &'a str,
    pub by: &'a str,
    pub name: &'a str,
    pub initials: Option<&'a str>,
    pub photos: &'a [String],
}

text_enum! {
    /// Where a group message stands with one of its people: going to them, in their direct
    /// message, or not sent, as when Slack wouldn't take it.
    pub enum DeliveryStatus {
        Waiting => "waiting",
        Sent => "sent",
        Failed => "failed",
    }
}

/// How far a group message has gone: to how many in all, how many it reached, and how many it
/// couldn't.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    pub people: i64,
    pub sent: i64,
    pub failed: i64,
}

/// A row of the list once those waiting are listed: a conversation answered or done, not listed
/// under a group, or a group nothing under waits in.
pub enum Answered {
    Conversation(String),
    Group(String),
}

/// A group message due to go to one of its people.
#[derive(Clone, Debug)]
pub struct Due {
    pub message: String,
    pub team: String,
    pub user: String,
    pub attempts: i64,
}
impl FromRow for Due {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            message: row.get("message_id")?,
            team: row.get("team_id")?,
            user: row.get("user_id")?,
            attempts: row.get("attempts")?,
        })
    }
}
/// A group message as Slack took it in one of its people's direct message.
pub struct Delivered<'a> {
    pub channel: &'a str,
    pub ts: &'a str,
    pub at: i64,
    /// Each of its photos, by its ID in Dispatch and Slack's in their workspace.
    pub photos: &'a [(String, String)],
}

/// What Communications reads and writes of its groups.
pub trait GroupStore {
    /// Makes a group of `people`, named `name`, by the member `by`, with its first message: the
    /// group. The photos added for the message become its own; one that isn't the writer's to
    /// send is refused.
    fn create_communications_group(
        &self,
        dsp: &str,
        name: &str,
        people: &[Member],
        writing: &Writing,
    ) -> Result<Group>;
    /// Sends the group another message, to the same people: its ID.
    fn add_communications_group_message(
        &self,
        dsp: &str,
        group: &str,
        writing: &Writing,
    ) -> Result<String>;
    fn communications_group(&self, dsp: &str, id: &str) -> Result<Option<Group>>;
    /// The groups `ids` names, in no order.
    fn communications_groups(&self, dsp: &str, ids: &[String]) -> Result<Vec<Group>>;
    /// The groups something under waits in, the longest waiting first.
    fn waiting_communications_groups(&self, dsp: &str) -> Result<Vec<Group>>;
    /// The conversations answered or done not listed under a group, and the groups nothing under
    /// waits in, of the workspace `team` or every one, the latest first: those before `before`,
    /// the time last written in and the ID, `most` at most.
    fn answered_communications(
        &self,
        dsp: &str,
        team: Option<&str>,
        before: Option<(i64, &str)>,
        most: usize,
    ) -> Result<Vec<Answered>>;
    /// The group's people, in the order they were added.
    fn communications_group_people(&self, dsp: &str, group: &str) -> Result<Vec<Member>>;
    /// How many people each of the groups `ids` names has, its first `faces`, and the workspaces
    /// its people are in.
    fn communications_group_outlines(
        &self,
        dsp: &str,
        ids: &[String],
        faces: usize,
    ) -> Result<Vec<Outline>>;
    /// The group's messages from the DSP, in the order sent.
    fn communications_group_messages(&self, dsp: &str, group: &str) -> Result<Vec<GroupMessage>>;
    fn communications_group_message(&self, dsp: &str, id: &str) -> Result<Option<GroupMessage>>;
    /// The latest of the DSP's messages to each of the groups `ids` names.
    fn latest_communications_group_messages(
        &self,
        dsp: &str,
        ids: &[String],
    ) -> Result<Vec<GroupMessage>>;
    /// The latest answer to each of the groups `ids` names.
    fn latest_communications_answers(&self, dsp: &str, ids: &[String]) -> Result<Vec<Message>>;
    /// The group's messages from the DSP and its answers before `before`, the time written and
    /// the ID, the latest `most`, in the order written: which of each.
    fn communications_group_entries(
        &self,
        dsp: &str,
        group: &str,
        before: Option<(i64, &str)>,
        most: usize,
    ) -> Result<(Vec<GroupMessage>, Vec<Message>)>;
    /// How far each of the group messages `ids` names has gone.
    fn communications_group_progress(
        &self,
        dsp: &str,
        ids: &[String],
    ) -> Result<Vec<(String, Progress)>>;
    /// Where the group message `message` stands with each of its people.
    fn communications_deliveries(
        &self,
        dsp: &str,
        message: &str,
    ) -> Result<Vec<(String, String, DeliveryStatus)>>;
    /// Each of the group's people who answered it, by workspace and ID, with when they first
    /// did.
    fn communications_group_answered(
        &self,
        dsp: &str,
        group: &str,
    ) -> Result<Vec<(String, String, i64)>>;
    /// Marks each conversation listed under the group done, as the member `by`, while it waits:
    /// how many were.
    fn mark_communications_group_done(&self, dsp: &str, group: &str, by: &str) -> Result<usize>;
    /// The group messages due to go to one of their people by `at`, the longest due first, `most`
    /// at most.
    fn due_communications_deliveries(&self, dsp: &str, at: i64, most: usize) -> Result<Vec<Due>>;
    /// Claims the delivery for one try, from `at` until `until`, while it's due: since when an
    /// earlier try may have posted it, if one may have; none when it's no longer due.
    fn claim_communications_delivery(
        &self,
        dsp: &str,
        due: &Due,
        at: i64,
        until: i64,
    ) -> Result<Option<Option<i64>>>;
    /// Keeps the group message as Slack took it in the person's direct message, in their
    /// conversation, under the name their Slack profile gives them now.
    fn deliver_communications_message(
        &self,
        dsp: &str,
        due: &Due,
        name: &str,
        delivered: &Delivered,
    ) -> Result<()>;
    /// Tries the delivery again at `at`, for `reason`, counted as a failure or not; `unposted`,
    /// known to be nowhere in Slack, so the next try needn't look for an earlier post.
    fn retry_communications_delivery(
        &self,
        dsp: &str,
        due: &Due,
        at: i64,
        reason: &str,
        counted: bool,
        unposted: bool,
    ) -> Result<()>;
    /// Gives the delivery up, for `reason`: the message wasn't sent them.
    fn fail_communications_delivery(&self, dsp: &str, due: &Due, reason: &str) -> Result<()>;
    /// Slack's IDs in the workspace `team` of the group message's photos uploaded there, by their
    /// IDs in Dispatch.
    fn communications_group_uploads(
        &self,
        dsp: &str,
        message: &str,
        team: &str,
    ) -> Result<Vec<(String, String)>>;
    /// Keeps Slack's ID in the workspace `team` of the group message's photo `file`.
    fn add_communications_group_upload(
        &self,
        dsp: &str,
        message: &str,
        team: &str,
        file: &str,
        slack_file: &str,
    ) -> Result<()>;
}

impl GroupStore for Store {
    fn create_communications_group(
        &self,
        dsp: &str,
        name: &str,
        people: &[Member],
        writing: &Writing,
    ) -> Result<Group> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let id = crypto::id("grp")?;
            db.exec(
                "INSERT INTO communications_groups (id,name,created_by,created_at,last_at) \
                 VALUES (?,?,?,?,?)",
                params![id, name, writing.by, now(), now()],
            )?;
            for (place, member) in people.iter().enumerate() {
                db.exec(
                    "INSERT INTO communications_group_people (group_id,team_id,user_id,name,place) \
                     VALUES (?,?,?,?,?)",
                    params![id, member.team, member.user, member.name, place as i64],
                )?;
            }
            written(&db, &id, writing)?;
            db.one_as("SELECT * FROM communications_groups WHERE id=?", [&id])?
                .ok_or_else(|| Error::new("communications_group_not_found", 404))
        })
    }
    fn add_communications_group_message(
        &self,
        dsp: &str,
        group: &str,
        writing: &Writing,
    ) -> Result<String> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let found = db.count(
                "SELECT count(*) FROM communications_groups WHERE id=?",
                [group],
            )?;
            if found == 0 {
                return Err(Error::new("communications_group_not_found", 404));
            }
            written(&db, group, writing)
        })
    }
    fn communications_group(&self, dsp: &str, id: &str) -> Result<Option<Group>> {
        self.dsp(dsp)?
            .one_as("SELECT * FROM communications_groups WHERE id=?", [id])
    }
    fn communications_groups(&self, dsp: &str, ids: &[String]) -> Result<Vec<Group>> {
        self.dsp(dsp)?.query_as(
            "SELECT * FROM communications_groups WHERE id IN (SELECT value FROM json_each(?))",
            [serde_json::to_string(ids)?],
        )
    }
    fn waiting_communications_groups(&self, dsp: &str) -> Result<Vec<Group>> {
        self.dsp(dsp)?.query_as(
            "SELECT * FROM communications_groups WHERE state='waiting' ORDER BY waiting_since,id",
            [],
        )
    }
    fn answered_communications(
        &self,
        dsp: &str,
        team: Option<&str>,
        before: Option<(i64, &str)>,
        most: usize,
    ) -> Result<Vec<Answered>> {
        let (at, id) = before.unwrap_or((i64::MAX, "\u{10FFFF}"));
        let rows: Vec<(String, String)> = self.dsp(dsp)?.query_as(
            "SELECT kind,id FROM (\
             SELECT 'conversation' kind,id,last_at FROM communications_conversations \
             WHERE state<>'waiting' AND last_at IS NOT NULL AND group_id IS NULL \
             AND (?4 IS NULL OR team_id=?4) \
             UNION ALL SELECT 'group',id,last_at FROM communications_groups g \
             WHERE state<>'waiting' AND (?4 IS NULL OR EXISTS (SELECT 1 FROM \
             communications_group_people p WHERE p.group_id=g.id AND p.team_id=?4))) \
             WHERE last_at<?1 OR (last_at=?1 AND id<?2) \
             ORDER BY last_at DESC,id DESC LIMIT ?3",
            params![at, id, most as i64, team],
        )?;
        Ok(rows
            .into_iter()
            .map(|(kind, id)| match kind.as_str() {
                "group" => Answered::Group(id),
                _ => Answered::Conversation(id),
            })
            .collect())
    }
    fn communications_group_people(&self, dsp: &str, group: &str) -> Result<Vec<Member>> {
        self.dsp(dsp)?.query_as(
            "SELECT team_id,user_id,name FROM communications_group_people WHERE group_id=? \
             ORDER BY place",
            [group],
        )
    }
    fn communications_group_outlines(
        &self,
        dsp: &str,
        ids: &[String],
        faces: usize,
    ) -> Result<Vec<Outline>> {
        let db = self.dsp(dsp)?;
        let asked = serde_json::to_string(ids)?;
        let counts: Vec<(String, i64)> = db.query_as(
            "SELECT group_id,count(*) FROM communications_group_people \
             WHERE group_id IN (SELECT value FROM json_each(?)) GROUP BY group_id",
            [&asked],
        )?;
        let first: Vec<(String, String, String, String)> = db.query_as(
            "SELECT group_id,team_id,user_id,name FROM communications_group_people \
             WHERE group_id IN (SELECT value FROM json_each(?1)) AND place<?2 \
             ORDER BY group_id,place",
            params![asked, faces as i64],
        )?;
        let teams: Vec<(String, String)> = db.query_as(
            "SELECT DISTINCT group_id,team_id FROM communications_group_people \
             WHERE group_id IN (SELECT value FROM json_each(?)) ORDER BY group_id,team_id",
            [&asked],
        )?;
        Ok(counts
            .into_iter()
            .map(|(group, count)| {
                let faces = first
                    .iter()
                    .filter(|(of, ..)| *of == group)
                    .map(|(_, team, user, name)| Member {
                        team: team.clone(),
                        user: user.clone(),
                        name: name.clone(),
                    })
                    .collect();
                let teams = teams
                    .iter()
                    .filter(|(of, _)| *of == group)
                    .map(|(_, team)| team.clone())
                    .collect();
                Outline {
                    group,
                    people: count,
                    faces,
                    teams,
                }
            })
            .collect())
    }
    fn communications_group_messages(&self, dsp: &str, group: &str) -> Result<Vec<GroupMessage>> {
        self.dsp(dsp)?.query_as(
            "SELECT * FROM communications_group_messages WHERE group_id=? ORDER BY at,id",
            [group],
        )
    }
    fn communications_group_message(&self, dsp: &str, id: &str) -> Result<Option<GroupMessage>> {
        self.dsp(dsp)?.one_as(
            "SELECT * FROM communications_group_messages WHERE id=?",
            [id],
        )
    }
    fn latest_communications_group_messages(
        &self,
        dsp: &str,
        ids: &[String],
    ) -> Result<Vec<GroupMessage>> {
        self.dsp(dsp)?.query_as(
            "SELECT m.* FROM communications_group_messages m \
             WHERE m.group_id IN (SELECT value FROM json_each(?)) AND m.id=(\
             SELECT id FROM communications_group_messages WHERE group_id=m.group_id \
             ORDER BY at DESC,id DESC LIMIT 1)",
            [serde_json::to_string(ids)?],
        )
    }
    fn latest_communications_answers(&self, dsp: &str, ids: &[String]) -> Result<Vec<Message>> {
        self.dsp(dsp)?.query_as(
            "SELECT m.* FROM communications_messages m \
             WHERE m.group_id IN (SELECT value FROM json_each(?)) AND m.direction='in' AND m.id=(\
             SELECT id FROM communications_messages WHERE group_id=m.group_id \
             AND direction='in' ORDER BY at DESC,id DESC LIMIT 1)",
            [serde_json::to_string(ids)?],
        )
    }
    fn communications_group_entries(
        &self,
        dsp: &str,
        group: &str,
        before: Option<(i64, &str)>,
        most: usize,
    ) -> Result<(Vec<GroupMessage>, Vec<Message>)> {
        let db = self.dsp(dsp)?;
        let (at, id) = before.unwrap_or((i64::MAX, "\u{10FFFF}"));
        let rows: Vec<(String, String)> = db.query_as(
            "SELECT kind,id FROM (\
             SELECT 'sent' kind,id,at FROM communications_group_messages WHERE group_id=?1 \
             UNION ALL SELECT 'answer',id,at FROM communications_messages \
             WHERE group_id=?1 AND direction='in') \
             WHERE at<?2 OR (at=?2 AND id<?3) ORDER BY at DESC,id DESC LIMIT ?4",
            params![group, at, id, most as i64],
        )?;
        let of = |kind: &str| -> Result<String> {
            Ok(serde_json::to_string(
                &rows
                    .iter()
                    .filter(|(each, _)| each == kind)
                    .map(|(_, id)| id)
                    .collect::<Vec<_>>(),
            )?)
        };
        let sent = db.query_as(
            "SELECT * FROM communications_group_messages \
             WHERE id IN (SELECT value FROM json_each(?)) ORDER BY at,id",
            [of("sent")?],
        )?;
        let answers = db.query_as(
            "SELECT * FROM communications_messages \
             WHERE id IN (SELECT value FROM json_each(?)) ORDER BY at,id",
            [of("answer")?],
        )?;
        Ok((sent, answers))
    }
    fn communications_group_progress(
        &self,
        dsp: &str,
        ids: &[String],
    ) -> Result<Vec<(String, Progress)>> {
        let rows: Vec<(String, i64, i64, i64)> = self.dsp(dsp)?.query_as(
            "SELECT message_id,count(*),sum(status='sent'),sum(status='failed') \
             FROM communications_deliveries WHERE message_id IN (SELECT value FROM json_each(?)) \
             GROUP BY message_id",
            [serde_json::to_string(ids)?],
        )?;
        Ok(rows
            .into_iter()
            .map(|(message, people, sent, failed)| {
                (
                    message,
                    Progress {
                        people,
                        sent,
                        failed,
                    },
                )
            })
            .collect())
    }
    fn communications_deliveries(
        &self,
        dsp: &str,
        message: &str,
    ) -> Result<Vec<(String, String, DeliveryStatus)>> {
        self.dsp(dsp)?.query_as(
            "SELECT team_id,user_id,status FROM communications_deliveries WHERE message_id=?",
            [message],
        )
    }
    fn communications_group_answered(
        &self,
        dsp: &str,
        group: &str,
    ) -> Result<Vec<(String, String, i64)>> {
        self.dsp(dsp)?.query_as(
            "SELECT c.team_id,c.user_id,min(m.at) FROM communications_messages m \
             JOIN communications_conversations c ON c.id=m.conversation_id \
             WHERE m.group_id=? AND m.direction='in' GROUP BY c.id ORDER BY min(m.at)",
            [group],
        )
    }
    fn mark_communications_group_done(&self, dsp: &str, group: &str, by: &str) -> Result<usize> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let waiting: Vec<(String,)> = db.query_as(
                "SELECT id FROM communications_conversations WHERE group_id=? AND state='waiting'",
                [group],
            )?;
            for (conversation,) in &waiting {
                db.exec(
                    "UPDATE communications_conversations SET done_at=?,done_by=? WHERE id=?",
                    params![now(), by, conversation],
                )?;
                settle(&db, conversation)?;
            }
            Ok(waiting.len())
        })
    }
    fn due_communications_deliveries(&self, dsp: &str, at: i64, most: usize) -> Result<Vec<Due>> {
        self.dsp(dsp)?.query_as(
            "SELECT d.message_id,d.team_id,d.user_id,d.attempts FROM communications_deliveries d \
             JOIN communications_group_messages m ON m.id=d.message_id \
             JOIN communications_group_people p ON p.group_id=m.group_id \
             AND p.team_id=d.team_id AND p.user_id=d.user_id \
             WHERE d.status='waiting' AND d.due_at<=?1 AND coalesce(d.claimed_until,0)<=?1 \
             ORDER BY d.due_at,m.at,m.id,p.place LIMIT ?2",
            params![at, most as i64],
        )
    }
    fn claim_communications_delivery(
        &self,
        dsp: &str,
        due: &Due,
        at: i64,
        until: i64,
    ) -> Result<Option<Option<i64>>> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let found: Option<(Option<i64>,)> = db.one_as(
                "SELECT posting_since FROM communications_deliveries WHERE message_id=?1 \
                 AND team_id=?2 AND user_id=?3 AND status='waiting' AND due_at<=?4 \
                 AND coalesce(claimed_until,0)<=?4",
                params![due.message, due.team, due.user, at],
            )?;
            let Some((posting_since,)) = found else {
                return Ok(None);
            };
            db.exec(
                "UPDATE communications_deliveries SET claimed_until=?4,\
                 posting_since=coalesce(posting_since,?5) \
                 WHERE message_id=?1 AND team_id=?2 AND user_id=?3",
                params![due.message, due.team, due.user, until, at],
            )?;
            Ok(Some(posting_since))
        })
    }
    fn deliver_communications_message(
        &self,
        dsp: &str,
        due: &Due,
        name: &str,
        delivered: &Delivered,
    ) -> Result<()> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let message: GroupMessage = db
                .one_as(
                    "SELECT * FROM communications_group_messages WHERE id=?",
                    [&due.message],
                )?
                .ok_or_else(|| Error::new("communications_group_not_found", 404))?;
            let conversation = opened(&db, &due.team, &due.user, delivered.channel, name)?;
            let sent = Sent {
                ts: delivered.ts,
                text: &message.text,
                at: delivered.at,
                by: &message.sent_by,
                name: &message.sender_name,
                photos: delivered.photos,
            };
            let (id, touched) = sent_out(&db, &conversation, &sent, Some(&message.group))?;
            // Their message shows the group message's photos, as Slack shows them in it.
            for (photo, slack_id) in delivered.photos {
                db.exec(
                    "INSERT INTO communications_files (id,message_id,slack_file_id,uploaded_by,\
                     name,mime,size,status,sha256,added_at) SELECT ?,?,?,uploaded_by,name,mime,\
                     size,status,sha256,? FROM communications_files WHERE id=?",
                    params![crypto::id("file")?, id, slack_id, now(), photo],
                )?;
            }
            db.exec(
                "UPDATE communications_deliveries SET status='sent',delivered_id=?4,\
                 claimed_until=NULL,posting_since=NULL,reason=NULL \
                 WHERE message_id=?1 AND team_id=?2 AND user_id=?3",
                params![due.message, due.team, due.user, id],
            )?;
            // Its group changes with the conversation, as how far its message has gone does.
            settle_with(&db, &conversation, touched)
        })
    }
    fn retry_communications_delivery(
        &self,
        dsp: &str,
        due: &Due,
        at: i64,
        reason: &str,
        counted: bool,
        unposted: bool,
    ) -> Result<()> {
        self.dsp(dsp)?.exec(
            "UPDATE communications_deliveries SET due_at=?4,reason=?5,claimed_until=NULL,\
             attempts=attempts+?6,posting_since=CASE WHEN ?7 THEN NULL ELSE posting_since END \
             WHERE message_id=?1 AND team_id=?2 AND user_id=?3",
            params![
                due.message,
                due.team,
                due.user,
                at,
                reason,
                i64::from(counted),
                unposted
            ],
        )?;
        Ok(())
    }
    fn fail_communications_delivery(&self, dsp: &str, due: &Due, reason: &str) -> Result<()> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            db.exec(
                "UPDATE communications_deliveries SET status='failed',reason=?4,\
                 claimed_until=NULL WHERE message_id=?1 AND team_id=?2 AND user_id=?3",
                params![due.message, due.team, due.user, reason],
            )?;
            let (group,): (String,) = db
                .one_as(
                    "SELECT group_id FROM communications_group_messages WHERE id=?",
                    [&due.message],
                )?
                .ok_or_else(|| Error::new("communications_group_not_found", 404))?;
            changed_conversation(&db, &group)
        })
    }
    fn communications_group_uploads(
        &self,
        dsp: &str,
        message: &str,
        team: &str,
    ) -> Result<Vec<(String, String)>> {
        self.dsp(dsp)?.query_as(
            "SELECT file_id,slack_file_id FROM communications_group_uploads \
             WHERE message_id=? AND team_id=?",
            [message, team],
        )
    }
    fn add_communications_group_upload(
        &self,
        dsp: &str,
        message: &str,
        team: &str,
        file: &str,
        slack_file: &str,
    ) -> Result<()> {
        self.dsp(dsp)?.exec(
            "INSERT OR REPLACE INTO communications_group_uploads \
             (message_id,team_id,file_id,slack_file_id) VALUES (?,?,?,?)",
            params![message, team, file, slack_file],
        )?;
        Ok(())
    }
}

/// Keeps a message to the group `group`, due to go at once to each of its people, its photos
/// its own: its ID. A photo that isn't the writer's, added for a message not sent yet, is
/// refused.
fn written(db: &Db, group: &str, writing: &Writing) -> Result<String> {
    let id = crypto::id("gmsg")?;
    let at = now();
    db.exec(
        "INSERT INTO communications_group_messages (id,group_id,text,sent_by,sender_name,\
         sender_initials,at) VALUES (?,?,?,?,?,?,?)",
        params![
            id,
            group,
            writing.text,
            writing.by,
            writing.name,
            writing.initials,
            at
        ],
    )?;
    let photos = serde_json::to_string(writing.photos)?;
    let taken = db.exec(
        "UPDATE communications_files SET group_message_id=?1,claimed_until=NULL \
         WHERE id IN (SELECT value FROM json_each(?2)) AND uploaded_by=?3 \
         AND message_id IS NULL AND group_message_id IS NULL AND coalesce(claimed_until,0)<=?4",
        params![id, photos, writing.by, at],
    )?;
    if taken != writing.photos.len() {
        return Err(Error::new("communications_upload_not_found", 404));
    }
    db.exec(
        "INSERT INTO communications_deliveries (message_id,team_id,user_id,due_at) \
         SELECT ?,team_id,user_id,? FROM communications_group_people WHERE group_id=?",
        params![id, at, group],
    )?;
    settle_group(db, group)?;
    Ok(id)
}

/// Keeps whether the group waits, since when, and when it was last written in, from the
/// conversations listed under it, its messages and their answers, and notes it changed.
pub(crate) fn settle_group(db: &Db, group: &str) -> Result<()> {
    db.exec(
        "UPDATE communications_groups SET \
         waiting_since=(SELECT min(waiting_since) FROM communications_conversations \
         WHERE group_id=?1 AND state='waiting'),\
         state=CASE WHEN EXISTS (SELECT 1 FROM communications_conversations \
         WHERE group_id=?1 AND state='waiting') THEN 'waiting' ELSE 'responded' END,\
         last_at=max(\
         coalesce((SELECT max(at) FROM communications_group_messages WHERE group_id=?1),0),\
         coalesce((SELECT max(at) FROM communications_messages \
         WHERE group_id=?1 AND direction='in'),0)) WHERE id=?1",
        [group],
    )?;
    changed_conversation(db, group)
}
