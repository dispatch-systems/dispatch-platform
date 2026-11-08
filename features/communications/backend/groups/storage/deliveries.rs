//! Durable group deliveries: claims, recovery, retries and per-workspace photo uploads.
use super::groups::GroupMessage;
use crate::backend::{
    changes::changed_conversation,
    storage::{
        conversations::opened,
        messages::{Sent, sent_out},
        settlement::settle_with,
    },
};
use dispatch_core::{
    Error, Result,
    db::{FromRow, Row, Store, now},
    foundation::crypto,
};
use rusqlite::params;

/// A group message due to go to one of its people, and when it was written.
#[derive(Clone, Debug)]
pub struct Due {
    pub message: String,
    pub team: String,
    pub user: String,
    pub attempts: i64,
    pub written: i64,
}
impl FromRow for Due {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            message: row.get("message_id")?,
            team: row.get("team_id")?,
            user: row.get("user_id")?,
            attempts: row.get("attempts")?,
            written: row.get("at")?,
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

/// What Communications keeps of group deliveries and their workspace uploads.
pub trait DeliveryStore {
    /// The group messages due to go to one of their people by `at`, written since `fresh`, or
    /// that a try may have posted, to be looked for, the longest due first, `most` at most.
    fn due_communications_deliveries(
        &self,
        dsp: &str,
        at: i64,
        fresh: i64,
        most: usize,
    ) -> Result<Vec<Due>>;
    /// Whether any group message written before `before` waits to go to one of its people, no
    /// try holding it at `at` and none that may have posted it.
    fn stale_communications_deliveries(&self, dsp: &str, before: i64, at: i64) -> Result<bool>;
    /// Gives up each group message written before `before` waiting to go to one of its people,
    /// no try holding it at `at` and none that may have posted it, for `reason`, or what it last
    /// waited on, if it waited on Slack.
    fn expire_communications_deliveries(
        &self,
        dsp: &str,
        before: i64,
        at: i64,
        reason: &str,
    ) -> Result<()>;
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

impl DeliveryStore for Store {
    fn due_communications_deliveries(
        &self,
        dsp: &str,
        at: i64,
        fresh: i64,
        most: usize,
    ) -> Result<Vec<Due>> {
        self.dsp(dsp)?.query_as(
            "SELECT d.message_id,d.team_id,d.user_id,d.attempts,m.at \
             FROM communications_deliveries d \
             JOIN communications_group_messages m ON m.id=d.message_id \
             JOIN communications_group_people p ON p.group_id=m.group_id \
             AND p.team_id=d.team_id AND p.user_id=d.user_id \
             WHERE d.status='waiting' AND d.due_at<=?1 AND coalesce(d.claimed_until,0)<=?1 \
             AND (m.at>=?2 OR d.posting_since IS NOT NULL) \
             ORDER BY d.due_at,m.at,m.id,p.place LIMIT ?3",
            params![at, fresh, most as i64],
        )
    }
    fn stale_communications_deliveries(&self, dsp: &str, before: i64, at: i64) -> Result<bool> {
        let stale = self.dsp(dsp)?.count(
            "SELECT count(*) FROM communications_deliveries d \
             JOIN communications_group_messages m ON m.id=d.message_id \
             WHERE d.status='waiting' AND m.at<?1 AND coalesce(d.claimed_until,0)<=?2 \
             AND d.posting_since IS NULL",
            params![before, at],
        )?;
        Ok(stale > 0)
    }
    fn expire_communications_deliveries(
        &self,
        dsp: &str,
        before: i64,
        at: i64,
        reason: &str,
    ) -> Result<()> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let stale = "FROM communications_deliveries d \
                         JOIN communications_group_messages m ON m.id=d.message_id \
                         WHERE d.status='waiting' AND m.at<?1 AND coalesce(d.claimed_until,0)<=?2 \
                         AND d.posting_since IS NULL";
            let groups: Vec<(String,)> = db.query_as(
                &format!("SELECT DISTINCT m.group_id {stale}"),
                params![before, at],
            )?;
            db.exec(
                &format!(
                    "UPDATE communications_deliveries SET status='failed',claimed_until=NULL,\
                     reason=CASE WHEN reason IN \
                     ('slack_needs_update','slack_connection_broken','slack_not_connected') \
                     THEN reason ELSE ?3 END \
                     WHERE (message_id,team_id,user_id) IN \
                     (SELECT d.message_id,d.team_id,d.user_id {stale})"
                ),
                params![before, at, reason],
            )?;
            for (group,) in &groups {
                changed_conversation(&db, group)?;
            }
            Ok(())
        })
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
                gif: None,
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
