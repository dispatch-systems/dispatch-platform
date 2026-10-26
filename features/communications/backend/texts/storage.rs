//! Texting's storage, in each DSP's own database, beside the Slack conversations': each
//! conversation by text, each text filed in one, how far Phones' changes were read, and the
//! names and blocks the DSP keeps for numbers. Whether a conversation waits follows from its
//! texts, and is kept beside them each time they change, as a Slack conversation's is.
use crate::backend::storage::{ConversationState, Direction, changed_conversation};
use dispatch_core::{
    Error, Result,
    db::{Db, FromRow, Row, Store, now},
    foundation::crypto,
    text_enum,
};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/communications/api/generated/"))]
    /// What a conversation is: a Slack direct message, the texts of the work phones a driver
    /// held (or of one nobody held as they were written), or a number's texts.
    pub enum ConversationKind {
        Slack => "slack",
        Work => "work",
        Number => "number",
    }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/communications/api/generated/"))]
    /// How a text Dispatch sent went: waiting on the phone to send it, sent, delivered, or
    /// failed.
    pub enum TextStatus {
        Waiting => "waiting",
        Sent => "sent",
        Delivered => "delivered",
        Failed => "failed",
    }
}

/// A conversation by text, as kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextConversation {
    pub id: String,
    pub kind: ConversationKind,
    /// For a driver's, the source of their ID and the ID.
    pub driver: Option<(String, String)>,
    /// For a number's, its number; for a work phone's nobody held, the phone's.
    pub number: Option<String>,
    pub state: ConversationState,
    pub waiting_since: Option<i64>,
    pub last_at: Option<i64>,
}
impl FromRow for TextConversation {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            kind: row.get("kind")?,
            driver: match (row.get("source")?, row.get("driver_id")?) {
                (Some(source), Some(id)) => Some((source, id)),
                _ => None,
            },
            number: row.get("number")?,
            state: row.get("state")?,
            waiting_since: row.get("waiting_since")?,
            last_at: row.get("last_at")?,
        })
    }
}

/// A text, as kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Text {
    /// Phones' ID of it.
    pub id: String,
    pub conversation: String,
    /// The DSP's phone it went through.
    pub phone: String,
    pub direction: Direction,
    /// The other party's number.
    pub number: String,
    pub body: String,
    pub at: i64,
    /// Who at the DSP sent it, for one Dispatch sent, and the name they're shown by.
    pub sent_by: Option<String>,
    pub sender_name: Option<String>,
    /// How one Dispatch sent went, and why it failed.
    pub status: Option<TextStatus>,
    pub error: Option<String>,
    pub files: Vec<TextFile>,
}
impl FromRow for Text {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        let files: String = row.get("files")?;
        Ok(Self {
            id: row.get("id")?,
            conversation: row.get("conversation_id")?,
            phone: row.get("phone")?,
            direction: row.get("direction")?,
            number: row.get("number")?,
            body: row.get("body")?,
            at: row.get("at")?,
            sent_by: row.get("sent_by")?,
            sender_name: row.get("sender_name")?,
            status: row.get("status")?,
            error: row.get("error")?,
            files: serde_json::from_str(&files)?,
        })
    }
}
/// A text's picture or file, as Phones has it: whether its bytes are kept yet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextFile {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub size: i64,
    pub kept: bool,
}

/// Where a text is filed: under the driver who held the work phone it was written with, by
/// their ID of a source; under a work phone nobody held then; or under its number.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Filed {
    Driver { source: String, id: String },
    WorkPhone(String),
    Number(String),
}

/// A text to keep, as Phones has it now, and where it's filed.
pub struct Incoming {
    pub id: String,
    pub phone: String,
    pub direction: Direction,
    pub number: String,
    pub body: String,
    pub at: i64,
    pub sent_by: Option<String>,
    pub status: Option<TextStatus>,
    pub error: Option<String>,
    pub files: Vec<TextFile>,
    pub filed: Filed,
}

/// What texting reads and writes of its conversations, texts, contacts and blocks.
pub trait TextStore {
    fn communications_text_conversation(
        &self,
        dsp: &str,
        id: &str,
    ) -> Result<Option<TextConversation>>;
    /// The conversations `ids` names, in no order.
    fn communications_text_conversations(
        &self,
        dsp: &str,
        ids: &[String],
    ) -> Result<Vec<TextConversation>>;
    /// The numbers' conversations waiting, but blocked numbers', the longest waiting first.
    fn waiting_communications_texts(&self, dsp: &str) -> Result<Vec<TextConversation>>;
    /// The numbers' conversations answered, but blocked numbers', the latest first, before
    /// `before` (when last written in, and the ID), the latest `most`.
    fn answered_communications_texts(
        &self,
        dsp: &str,
        before: Option<(i64, &str)>,
        most: usize,
    ) -> Result<Vec<TextConversation>>;
    /// Every number's conversation, those holding no text too, blocked ones too.
    fn communications_numbers(&self, dsp: &str) -> Result<Vec<TextConversation>>;
    /// The latest text of each conversation `ids` names.
    fn latest_communications_texts(&self, dsp: &str, ids: &[String]) -> Result<Vec<Text>>;
    /// The latest text the other party wrote in each conversation `ids` names.
    fn latest_communications_texts_in(&self, dsp: &str, ids: &[String]) -> Result<Vec<Text>>;
    /// The conversation's texts before `before`, when written and the ID, the latest `most`, in
    /// the order written.
    fn communications_texts(
        &self,
        dsp: &str,
        conversation: &str,
        before: Option<(i64, &str)>,
        most: usize,
    ) -> Result<Vec<Text>>;
    fn communications_text(&self, dsp: &str, id: &str) -> Result<Option<Text>>;
    /// Every text written from `from` on, oldest first.
    fn communications_texts_from(&self, dsp: &str, from: i64) -> Result<Vec<Text>>;
    /// The last of Phones' changes read: 0 before the first.
    fn communications_texts_read_to(&self, dsp: &str) -> Result<i64>;
    /// Keeps each of `texts` where it's filed, moving one kept elsewhere before, and notes that
    /// Phones' changes were read up to `read_to`: the conversations changed.
    fn file_communications_texts(
        &self,
        dsp: &str,
        texts: &[Incoming],
        read_to: i64,
    ) -> Result<BTreeSet<String>>;
    /// Names the text `id` Dispatch sent with the name of whoever at the DSP wrote it.
    fn name_communications_text(&self, dsp: &str, id: &str, name: &str) -> Result<()>;
    /// Moves each text kept to where it's filed now, by its ID: the conversations changed.
    fn refile_communications_texts(
        &self,
        dsp: &str,
        moves: &[(String, Filed)],
    ) -> Result<BTreeSet<String>>;
    /// The number's conversation, made with no text if it's new.
    fn open_communications_number(&self, dsp: &str, number: &str) -> Result<TextConversation>;
    /// Marks the conversation done without an answer, as the member `by`, while it waits:
    /// whether it was waiting.
    fn mark_communications_text_done(&self, dsp: &str, id: &str, by: &str) -> Result<bool>;
    /// Every contact the DSP saved: each number with its name.
    fn communications_contacts(&self, dsp: &str) -> Result<Vec<(String, String)>>;
    /// Saves `name` for the number, as the member `by`.
    fn save_communications_contact(
        &self,
        dsp: &str,
        number: &str,
        name: &str,
        by: &str,
    ) -> Result<()>;
    /// Every number blocked.
    fn communications_blocked(&self, dsp: &str) -> Result<BTreeSet<String>>;
    /// Blocks the number, as the member `by`, or unblocks it.
    fn block_communications_number(&self, dsp: &str, number: &str, by: Option<&str>) -> Result<()>;
}

/// The numbers' conversations that are listed: those of numbers not blocked.
const LISTED: &str = "kind='number' AND number NOT IN (SELECT number FROM communications_blocked)";

impl TextStore for Store {
    fn communications_text_conversation(
        &self,
        dsp: &str,
        id: &str,
    ) -> Result<Option<TextConversation>> {
        self.dsp(dsp)?.one_as(
            "SELECT * FROM communications_text_conversations WHERE id=?",
            [id],
        )
    }
    fn communications_text_conversations(
        &self,
        dsp: &str,
        ids: &[String],
    ) -> Result<Vec<TextConversation>> {
        self.dsp(dsp)?.query_as(
            "SELECT * FROM communications_text_conversations \
             WHERE id IN (SELECT value FROM json_each(?))",
            [serde_json::to_string(ids)?],
        )
    }
    fn waiting_communications_texts(&self, dsp: &str) -> Result<Vec<TextConversation>> {
        self.dsp(dsp)?.query_as(
            &format!(
                "SELECT * FROM communications_text_conversations WHERE state='waiting' \
                 AND {LISTED} ORDER BY waiting_since,id"
            ),
            [],
        )
    }
    fn answered_communications_texts(
        &self,
        dsp: &str,
        before: Option<(i64, &str)>,
        most: usize,
    ) -> Result<Vec<TextConversation>> {
        let (at, id) = before.unwrap_or((i64::MAX, "\u{10FFFF}"));
        self.dsp(dsp)?.query_as(
            &format!(
                "SELECT * FROM communications_text_conversations WHERE state<>'waiting' \
                 AND last_at IS NOT NULL AND {LISTED} AND (last_at<?1 OR (last_at=?1 AND id<?2)) \
                 ORDER BY last_at DESC,id DESC LIMIT ?3"
            ),
            params![at, id, most as i64],
        )
    }
    fn communications_numbers(&self, dsp: &str) -> Result<Vec<TextConversation>> {
        self.dsp(dsp)?.query_as(
            "SELECT * FROM communications_text_conversations WHERE kind='number' ORDER BY id",
            [],
        )
    }
    fn latest_communications_texts(&self, dsp: &str, ids: &[String]) -> Result<Vec<Text>> {
        self.dsp(dsp)?.query_as(
            "SELECT t.* FROM communications_texts t \
             WHERE t.conversation_id IN (SELECT value FROM json_each(?)) AND t.id=(\
             SELECT id FROM communications_texts WHERE conversation_id=t.conversation_id \
             ORDER BY at DESC,id DESC LIMIT 1)",
            [serde_json::to_string(ids)?],
        )
    }
    fn latest_communications_texts_in(&self, dsp: &str, ids: &[String]) -> Result<Vec<Text>> {
        self.dsp(dsp)?.query_as(
            "SELECT t.* FROM communications_texts t \
             WHERE t.conversation_id IN (SELECT value FROM json_each(?)) AND t.id=(\
             SELECT id FROM communications_texts WHERE conversation_id=t.conversation_id \
             AND direction='in' ORDER BY at DESC,id DESC LIMIT 1)",
            [serde_json::to_string(ids)?],
        )
    }
    fn communications_texts(
        &self,
        dsp: &str,
        conversation: &str,
        before: Option<(i64, &str)>,
        most: usize,
    ) -> Result<Vec<Text>> {
        let (at, id) = before.unwrap_or((i64::MAX, "\u{10FFFF}"));
        let mut texts: Vec<Text> = self.dsp(dsp)?.query_as(
            "SELECT * FROM communications_texts WHERE conversation_id=?1 \
             AND (at<?2 OR (at=?2 AND id<?3)) ORDER BY at DESC,id DESC LIMIT ?4",
            params![conversation, at, id, most as i64],
        )?;
        texts.reverse();
        Ok(texts)
    }
    fn communications_text(&self, dsp: &str, id: &str) -> Result<Option<Text>> {
        self.dsp(dsp)?
            .one_as("SELECT * FROM communications_texts WHERE id=?", [id])
    }
    fn communications_texts_from(&self, dsp: &str, from: i64) -> Result<Vec<Text>> {
        self.dsp(dsp)?.query_as(
            "SELECT * FROM communications_texts WHERE at>=? ORDER BY at,id",
            [from],
        )
    }
    fn communications_texts_read_to(&self, dsp: &str) -> Result<i64> {
        self.dsp(dsp)?.count(
            "SELECT coalesce((SELECT read_to FROM communications_texting WHERE id=1),0)",
            [],
        )
    }
    fn file_communications_texts(
        &self,
        dsp: &str,
        texts: &[Incoming],
        read_to: i64,
    ) -> Result<BTreeSet<String>> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let mut touched = BTreeSet::new();
            for text in texts {
                let conversation = conversation(&db, &text.filed)?;
                let was: Option<(String,)> = db.one_as(
                    "SELECT conversation_id FROM communications_texts WHERE id=?",
                    [&text.id],
                )?;
                // A text from a number blocked as it came never makes its conversation wait.
                let blocked = matches!(text.filed, Filed::Number(_))
                    && db.count(
                        "SELECT count(*) FROM communications_blocked WHERE number=?",
                        [&text.number],
                    )? > 0;
                db.exec(
                    "INSERT INTO communications_texts (id,conversation_id,phone,direction,number,\
                     body,at,sent_by,status,error,files,blocked) \
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12) \
                     ON CONFLICT(id) DO UPDATE SET conversation_id=excluded.conversation_id,\
                     body=excluded.body,status=excluded.status,error=excluded.error,\
                     files=excluded.files",
                    params![
                        text.id,
                        conversation,
                        text.phone,
                        text.direction,
                        text.number,
                        text.body,
                        text.at,
                        text.sent_by,
                        text.status,
                        text.error,
                        serde_json::to_string(&text.files)?,
                        blocked,
                    ],
                )?;
                touched.extend(was.map(|(was,)| was));
                touched.insert(conversation);
            }
            db.exec(
                "INSERT INTO communications_texting (id,read_to) VALUES (1,?) \
                 ON CONFLICT(id) DO UPDATE SET read_to=max(read_to,excluded.read_to)",
                [read_to],
            )?;
            for conversation in &touched {
                settle(&db, conversation)?;
            }
            Ok(touched)
        })
    }
    fn name_communications_text(&self, dsp: &str, id: &str, name: &str) -> Result<()> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            db.exec(
                "UPDATE communications_texts SET sender_name=? WHERE id=?",
                params![name, id],
            )?;
            let found: Option<(String,)> = db.one_as(
                "SELECT conversation_id FROM communications_texts WHERE id=?",
                [id],
            )?;
            match found {
                Some((conversation,)) => changed_conversation(&db, &conversation),
                None => Ok(()),
            }
        })
    }
    fn refile_communications_texts(
        &self,
        dsp: &str,
        moves: &[(String, Filed)],
    ) -> Result<BTreeSet<String>> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let mut touched = BTreeSet::new();
            for (text, filed) in moves {
                let to = conversation(&db, filed)?;
                let Some((was,)): Option<(String,)> = db.one_as(
                    "SELECT conversation_id FROM communications_texts WHERE id=?",
                    [text],
                )?
                else {
                    continue;
                };
                if was == to {
                    continue;
                }
                db.exec(
                    "UPDATE communications_texts SET conversation_id=? WHERE id=?",
                    params![to, text],
                )?;
                touched.insert(was);
                touched.insert(to);
            }
            for conversation in &touched {
                settle(&db, conversation)?;
            }
            Ok(touched)
        })
    }
    fn open_communications_number(&self, dsp: &str, number: &str) -> Result<TextConversation> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let id = conversation(&db, &Filed::Number(number.to_owned()))?;
            db.one_as(
                "SELECT * FROM communications_text_conversations WHERE id=?",
                [&id],
            )?
            .ok_or_else(|| Error::new("communications_conversation_not_found", 404))
        })
    }
    fn mark_communications_text_done(&self, dsp: &str, id: &str, by: &str) -> Result<bool> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let marked = db.exec(
                "UPDATE communications_text_conversations SET done_at=?,done_by=? \
                 WHERE id=? AND state='waiting'",
                params![now(), by, id],
            )?;
            if marked > 0 {
                settle(&db, id)?;
            }
            Ok(marked > 0)
        })
    }
    fn communications_contacts(&self, dsp: &str) -> Result<Vec<(String, String)>> {
        self.dsp(dsp)?.query_as(
            "SELECT number,name FROM communications_contacts ORDER BY number",
            [],
        )
    }
    fn save_communications_contact(
        &self,
        dsp: &str,
        number: &str,
        name: &str,
        by: &str,
    ) -> Result<()> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            db.exec(
                "INSERT INTO communications_contacts (number,name,saved_by,saved_at) \
                 VALUES (?,?,?,?) ON CONFLICT(number) DO UPDATE SET name=excluded.name,\
                 saved_by=excluded.saved_by,saved_at=excluded.saved_at",
                params![number, name, by, now()],
            )?;
            changed_number(&db, number)
        })
    }
    fn communications_blocked(&self, dsp: &str) -> Result<BTreeSet<String>> {
        let blocked: Vec<(String,)> = self
            .dsp(dsp)?
            .query_as("SELECT number FROM communications_blocked", [])?;
        Ok(blocked.into_iter().map(|(number,)| number).collect())
    }
    fn block_communications_number(&self, dsp: &str, number: &str, by: Option<&str>) -> Result<()> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            match by {
                Some(by) => db.exec(
                    "INSERT OR IGNORE INTO communications_blocked (number,blocked_by,blocked_at) \
                     VALUES (?,?,?)",
                    params![number, by, now()],
                )?,
                None => db.exec(
                    "DELETE FROM communications_blocked WHERE number=?",
                    [number],
                )?,
            };
            changed_number(&db, number)
        })
    }
}

/// The conversation a text filed so is in, made if it's new: its ID.
fn conversation(db: &Db, filed: &Filed) -> Result<String> {
    let (kind, source, driver, number) = match filed {
        Filed::Driver { source, id } => ("work", Some(source), Some(id), None),
        Filed::WorkPhone(number) => ("work", None, None, Some(number)),
        Filed::Number(number) => ("number", None, None, Some(number)),
    };
    let found: Option<(String,)> = match (source, driver) {
        (Some(source), Some(driver)) => db.one_as(
            "SELECT id FROM communications_text_conversations WHERE source=? AND driver_id=?",
            [source, driver],
        )?,
        _ => db.one_as(
            "SELECT id FROM communications_text_conversations \
             WHERE kind=? AND number=? AND driver_id IS NULL",
            params![kind, number],
        )?,
    };
    if let Some((id,)) = found {
        return Ok(id);
    }
    let id = crypto::id("tc")?;
    db.exec(
        "INSERT INTO communications_text_conversations (id,kind,source,driver_id,number) \
         VALUES (?,?,?,?,?)",
        params![id, kind, source, driver, number],
    )?;
    Ok(id)
}

/// Notes that the number's conversation changed, as a name saved for it or its block, for the
/// pages waiting on changes.
fn changed_number(db: &Db, number: &str) -> Result<()> {
    let found: Option<(String,)> = db.one_as(
        "SELECT id FROM communications_text_conversations \
         WHERE kind='number' AND number=? AND driver_id IS NULL",
        [number],
    )?;
    match found {
        Some((id,)) => changed_conversation(db, &id),
        None => Ok(()),
    }
}

/// Keeps whether the conversation waits on an answer, and since when, from its texts and when
/// it was marked done, and notes it changed. It waits while the other party's latest text, but
/// one that came while their number was blocked, came after the latest text to them that didn't
/// fail, or after it was marked done: since the first of theirs after that.
pub(crate) fn settle(db: &Db, conversation: &str) -> Result<()> {
    let (last_in, last_out, done_at): (Option<i64>, Option<i64>, Option<i64>) = db
        .one_as(
            "SELECT (SELECT max(at) FROM communications_texts \
             WHERE conversation_id=?1 AND direction='in' AND blocked=0),\
             (SELECT max(at) FROM communications_texts WHERE conversation_id=?1 \
             AND direction='out' AND coalesce(status,'sent')<>'failed'),\
             done_at FROM communications_text_conversations WHERE id=?1",
            [conversation],
        )?
        .ok_or_else(|| Error::new("communications_conversation_not_found", 404))?;
    let answered = last_out.max(done_at);
    let waiting = last_in.is_some() && last_in > answered;
    let state = if waiting {
        ConversationState::Waiting
    } else if done_at > last_out {
        ConversationState::Done
    } else {
        ConversationState::Responded
    };
    let since: Option<i64> = if waiting {
        db.one_as::<(Option<i64>,)>(
            "SELECT min(at) FROM communications_texts WHERE conversation_id=? \
             AND direction='in' AND blocked=0 AND at>?",
            params![conversation, answered.unwrap_or(-1)],
        )?
        .and_then(|(at,)| at)
    } else {
        None
    };
    db.exec(
        "UPDATE communications_text_conversations SET state=?2,waiting_since=?3,\
         last_at=(SELECT max(at) FROM communications_texts WHERE conversation_id=?1) \
         WHERE id=?1",
        params![conversation, state, since],
    )?;
    changed_conversation(db, conversation)
}
