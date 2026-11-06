//! Durable direct replies: notes, claims and recovery, then keeping what Slack took.
//! Keeping a reply clears its note, attaches its photos and settles it in one transaction.
use super::{
    messages::{Message, Sent, sent_out},
    settlement::settle_with,
};
use dispatch_core::{
    Error, Result,
    db::{FromRow, Row, Store},
    ensure,
};
use rusqlite::params;

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

/// Pending direct replies and their claim ownership.
pub trait ReplyStore {
    /// Notes an answer before it goes to Slack, or, noted already, its try again, while that try
    /// holds it (`claimed_until`): its photos and the name it goes under, from when it may have
    /// been posted kept as it was. Another try holding it is refused
    /// (`communications_reply_sending`).
    fn note_communications_reply(&self, dsp: &str, noted: &Noted) -> Result<()>;
    /// Claims the answer noted as `id` for a try until `until`, while no try holds it at `at`:
    /// whether it did.
    fn claim_communications_reply(&self, dsp: &str, id: &str, at: i64, until: i64) -> Result<bool>;
    /// Claims each answer noted that no try holds at `at`, until `until`: those it claimed.
    fn claim_communications_replies(&self, dsp: &str, at: i64, until: i64) -> Result<Vec<Noted>>;
    /// The answers noted in the conversation `conversation`, or in every one, no try holding them
    /// at `at`, the earliest first.
    fn noted_communications_replies(
        &self,
        dsp: &str,
        conversation: Option<&str>,
        at: i64,
    ) -> Result<Vec<Noted>>;
    /// Clears the answer noted as `id`, not in Slack, while the try that claimed it until
    /// `claimed_until` holds it.
    fn clear_communications_reply(&self, dsp: &str, id: &str, claimed_until: i64) -> Result<()>;
    /// Lets go of the answer noted as `id` while the try that claimed it until `claimed_until`
    /// holds it, as that try ended without keeping it: another may claim it.
    fn release_communications_reply(&self, dsp: &str, id: &str, claimed_until: i64) -> Result<()>;
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
}

impl ReplyStore for Store {
    fn note_communications_reply(&self, dsp: &str, noted: &Noted) -> Result<()> {
        let noted = self.dsp(dsp)?.exec(
            "INSERT INTO communications_replies (id,conversation_id,text,sent_by,sender_name,\
             photos,posting_since,claimed_until) VALUES (?,?,?,?,?,?,?,?) \
             ON CONFLICT(id) DO UPDATE SET sender_name=excluded.sender_name,\
             photos=excluded.photos WHERE claimed_until=excluded.claimed_until",
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
        ensure(noted > 0, "communications_reply_sending", 409)
    }
    fn claim_communications_reply(&self, dsp: &str, id: &str, at: i64, until: i64) -> Result<bool> {
        let claimed = self.dsp(dsp)?.exec(
            "UPDATE communications_replies SET claimed_until=? WHERE id=? AND claimed_until<=?",
            params![until, id, at],
        )?;
        Ok(claimed > 0)
    }
    fn claim_communications_replies(&self, dsp: &str, at: i64, until: i64) -> Result<Vec<Noted>> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            db.exec(
                "UPDATE communications_replies SET claimed_until=? WHERE claimed_until<=?",
                params![until, at],
            )?;
            db.query_as(
                "SELECT * FROM communications_replies WHERE claimed_until=? \
                 ORDER BY posting_since,id",
                [until],
            )
        })
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
    fn clear_communications_reply(&self, dsp: &str, id: &str, claimed_until: i64) -> Result<()> {
        self.dsp(dsp)?.exec(
            "DELETE FROM communications_replies WHERE id=? AND claimed_until=?",
            params![id, claimed_until],
        )?;
        Ok(())
    }
    fn release_communications_reply(&self, dsp: &str, id: &str, claimed_until: i64) -> Result<()> {
        self.dsp(dsp)?.exec(
            "UPDATE communications_replies SET claimed_until=0 WHERE id=? AND claimed_until=?",
            params![id, claimed_until],
        )?;
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
}
