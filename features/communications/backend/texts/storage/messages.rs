//! Each conversation by text and the texts filed in it, and whether each conversation waits, kept
//! beside its texts each time they change. A text is the phone side's row of it (`phones_texts`),
//! on which this keeps the conversation it's filed in (`conversation_id`, none while it waits to
//! be), the name its sender was shown by as it was filed, whether its number was blocked as it
//! came, and the Done that covered it; the rest of the row, and the text's files, are the phone
//! side's (`phone::storage`).
use crate::backend::{
    changes::changed_conversation,
    model::{ConversationState, TEXT_PREFIX},
    phone::storage::FileStore,
    texts::model::{Filed, Text, TextConversation, Unfiled},
};
use dispatch_core::{
    Error, Result,
    db::{Db, Store, migrations::add_column, now},
    ensure,
    foundation::crypto,
};
use rusqlite::{Params, params};
use std::collections::{BTreeSet, HashMap};

/// What texting reads and writes of its conversations and their texts.
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
    /// The conversations of the numbers `numbers`, in no order.
    fn communications_numbers_of(
        &self,
        dsp: &str,
        numbers: &[String],
    ) -> Result<Vec<TextConversation>>;
    /// The latest text of each conversation `ids` names.
    fn latest_communications_texts(&self, dsp: &str, ids: &[String]) -> Result<Vec<Text>>;
    /// The latest text the other party wrote in each conversation `ids` names, but one that came
    /// while their number was blocked.
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
    /// The text `id`, filed or waiting to be.
    fn communications_text(&self, dsp: &str, id: &str) -> Result<Option<Text>>;
    /// Of the texts `ids`, those waiting to be filed, in no order.
    fn unfiled_communications_texts_of(&self, dsp: &str, ids: &[String]) -> Result<Vec<Unfiled>>;
    /// Every text filed that was written from `from` until `to`, oldest first.
    fn communications_texts_between(&self, dsp: &str, from: i64, to: i64) -> Result<Vec<Text>>;
    /// The texts waiting to be filed after `after`, when written and the ID, the oldest `most`,
    /// oldest first: each when written and its ID.
    fn unfiled_communications_texts(
        &self,
        dsp: &str,
        after: Option<(i64, &str)>,
        most: usize,
    ) -> Result<Vec<(i64, String)>>;
    /// Files the text `id`, waiting to be, as `filed`, sent by someone at the DSP shown by
    /// `sender_name`, as their name stands now: one with a number of its own as blocked while its
    /// number is. The conversation it's in. Where a text is filed is decided again only as its
    /// day's route data changes (`refile_communications_texts`).
    fn place_communications_text(
        &self,
        dsp: &str,
        id: &str,
        filed: &Filed,
        sender_name: Option<&str>,
    ) -> Result<String>;
    /// Keeps whether the conversation each of the texts `ids` is filed in waits, as they changed:
    /// whether any is filed in one.
    fn settle_communications_texts(&self, dsp: &str, ids: &[String]) -> Result<bool>;
    /// Moves each text filed to where it's filed now, by its ID: the conversations changed.
    fn refile_communications_texts(
        &self,
        dsp: &str,
        moves: &[(String, Filed)],
    ) -> Result<BTreeSet<String>>;
    /// The conversations of the drivers `ids` of the source `source`, in no order.
    fn communications_drivers(
        &self,
        dsp: &str,
        source: &str,
        ids: &[String],
    ) -> Result<Vec<TextConversation>>;
    /// The work conversations, drivers' and work phones' nobody had, holding a text written from
    /// `from` on, in no order.
    fn communications_work_from(&self, dsp: &str, from: i64) -> Result<Vec<TextConversation>>;
    /// Every work conversation holding a text, in no order.
    fn communications_work(&self, dsp: &str) -> Result<Vec<TextConversation>>;
    /// The numbers each work conversation's texts were with, by its ID: each work phone a
    /// driver's was texted on, or a work phone's own.
    fn communications_work_numbers(&self, dsp: &str) -> Result<HashMap<String, Vec<String>>>;
    /// The work conversations waiting by all their texts, whichever day they were written.
    fn waiting_communications_work(&self, dsp: &str) -> Result<Vec<TextConversation>>;
    /// Of the conversations `ids`, those waiting on texts written from `from` on, each with since
    /// when: the first of those after its latest answer, or Done.
    fn communications_waiting_from(
        &self,
        dsp: &str,
        ids: &[String],
        from: i64,
    ) -> Result<Vec<(String, i64)>>;
    /// Notes changed, for the pages waiting on changes, each work conversation waiting by its
    /// texts written in from `from` on, as whether it waits counts only on the live day, which
    /// began or ended; and every driver's, texted in or not, as the work phone they have may have
    /// changed, one they had before as much as one they have now.
    fn changed_communications_work(&self, dsp: &str, from: i64) -> Result<()>;
    /// The conversation texts filed so would be in, made with no text if it's new.
    fn open_communications_text(&self, dsp: &str, filed: &Filed) -> Result<TextConversation>;
    /// Marks the conversation done without an answer, as the member `by`, while it waits, and
    /// each text in it the Done covers, which stays so wherever it's filed again: whether it was
    /// waiting.
    fn mark_communications_text_done(&self, dsp: &str, id: &str, by: &str) -> Result<bool>;
}

/// When the conversation `c`'s latest text out that didn't fail was written.
macro_rules! last_out {
    () => {
        "(SELECT max(o.at) FROM phones_texts o WHERE o.conversation_id=c.id \
         AND o.direction='out' AND coalesce(o.status,'sent')<>'failed')"
    };
}
/// Since when the conversation `c` waits, on texts written from `?1` on: the first text in, but
/// one that came while its number was blocked or one a Done covered, wherever it was filed then,
/// after the latest text out that didn't fail; none when it waits on nothing written from then.
macro_rules! waiting_since {
    () => {
        concat!(
            "(SELECT min(t.at) FROM phones_texts t WHERE t.conversation_id=c.id \
             AND t.direction='in' AND t.blocked=0 AND t.at>=?1 AND t.done_at IS NULL \
             AND t.at>coalesce(",
            last_out!(),
            ",-1))"
        )
    };
}

/// The numbers' conversations that are listed: those of numbers not blocked.
const LISTED: &str = "kind='number' AND number NOT IN (SELECT number FROM communications_blocked)";
/// Where a first page starts, as `before`: after every conversation and text.
const END: (i64, &str) = (i64::MAX, "\u{10FFFF}");
/// Where a first page of texts oldest first starts, as `after`: before every text.
const START: (i64, &str) = (i64::MIN, "");

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
        let (at, id) = before.unwrap_or(END);
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
    fn communications_numbers_of(
        &self,
        dsp: &str,
        numbers: &[String],
    ) -> Result<Vec<TextConversation>> {
        self.dsp(dsp)?.query_as(
            "SELECT * FROM communications_text_conversations WHERE kind='number' \
             AND number IN (SELECT value FROM json_each(?))",
            [serde_json::to_string(numbers)?],
        )
    }
    fn latest_communications_texts(&self, dsp: &str, ids: &[String]) -> Result<Vec<Text>> {
        texts(
            self,
            dsp,
            "SELECT t.* FROM phones_texts t \
             WHERE t.conversation_id IN (SELECT value FROM json_each(?)) AND t.id=(\
             SELECT id FROM phones_texts WHERE conversation_id=t.conversation_id \
             ORDER BY at DESC,id DESC LIMIT 1)",
            [serde_json::to_string(ids)?],
        )
    }
    fn latest_communications_texts_in(&self, dsp: &str, ids: &[String]) -> Result<Vec<Text>> {
        texts(
            self,
            dsp,
            "SELECT t.* FROM phones_texts t \
             WHERE t.conversation_id IN (SELECT value FROM json_each(?)) AND t.id=(\
             SELECT id FROM phones_texts WHERE conversation_id=t.conversation_id \
             AND direction='in' AND blocked=0 ORDER BY at DESC,id DESC LIMIT 1)",
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
        let (at, id) = before.unwrap_or(END);
        let mut texts = texts(
            self,
            dsp,
            "SELECT * FROM phones_texts WHERE conversation_id=?1 \
             AND (at<?2 OR (at=?2 AND id<?3)) ORDER BY at DESC,id DESC LIMIT ?4",
            params![conversation, at, id, most as i64],
        )?;
        texts.reverse();
        Ok(texts)
    }
    fn communications_text(&self, dsp: &str, id: &str) -> Result<Option<Text>> {
        Ok(texts(self, dsp, "SELECT * FROM phones_texts WHERE id=?", [id])?.pop())
    }
    fn unfiled_communications_texts_of(&self, dsp: &str, ids: &[String]) -> Result<Vec<Unfiled>> {
        self.dsp(dsp)?.query_as(
            "SELECT id,number,at,sent_by FROM phones_texts \
             WHERE id IN (SELECT value FROM json_each(?)) AND conversation_id IS NULL",
            [serde_json::to_string(ids)?],
        )
    }
    fn communications_texts_between(&self, dsp: &str, from: i64, to: i64) -> Result<Vec<Text>> {
        texts(
            self,
            dsp,
            "SELECT * FROM phones_texts WHERE conversation_id IS NOT NULL AND at>=?1 AND at<?2 \
             ORDER BY at,id",
            [from, to],
        )
    }
    fn unfiled_communications_texts(
        &self,
        dsp: &str,
        after: Option<(i64, &str)>,
        most: usize,
    ) -> Result<Vec<(i64, String)>> {
        let (at, id) = after.unwrap_or(START);
        self.dsp(dsp)?.query_as(
            "SELECT at,id FROM phones_texts WHERE conversation_id IS NULL \
             AND (at>?1 OR (at=?1 AND id>?2)) ORDER BY at,id LIMIT ?3",
            params![at, id, most as i64],
        )
    }
    fn place_communications_text(
        &self,
        dsp: &str,
        id: &str,
        filed: &Filed,
        sender_name: Option<&str>,
    ) -> Result<String> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let conversation = conversation(&db, filed)?;
            // A text from a number blocked as it came never makes its conversation wait.
            let blocked = match filed {
                Filed::Number(number) => {
                    db.count(
                        "SELECT count(*) FROM communications_blocked WHERE number=?",
                        [number],
                    )? > 0
                }
                Filed::Driver { .. } | Filed::WorkPhone(_) => false,
            };
            let placed = db.exec(
                "UPDATE phones_texts SET conversation_id=?,sender_name=?,blocked=? \
                 WHERE id=? AND conversation_id IS NULL",
                params![conversation, sender_name, blocked, id],
            )?;
            ensure(placed == 1, "communications_message_not_found", 404)?;
            Ok(conversation)
        })
    }
    fn settle_communications_texts(&self, dsp: &str, ids: &[String]) -> Result<bool> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let filed: Vec<(String,)> = db.query_as(
                "SELECT DISTINCT conversation_id FROM phones_texts \
                 WHERE id IN (SELECT value FROM json_each(?)) AND conversation_id IS NOT NULL",
                [serde_json::to_string(ids)?],
            )?;
            for (conversation,) in &filed {
                settle(&db, conversation)?;
            }
            Ok(!filed.is_empty())
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
                let Some(was) = filed_in(&db, text)? else {
                    continue;
                };
                if was == to {
                    continue;
                }
                db.exec(
                    "UPDATE phones_texts SET conversation_id=? WHERE id=?",
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
    fn communications_drivers(
        &self,
        dsp: &str,
        source: &str,
        ids: &[String],
    ) -> Result<Vec<TextConversation>> {
        self.dsp(dsp)?.query_as(
            "SELECT * FROM communications_text_conversations WHERE source=? \
             AND driver_id IN (SELECT value FROM json_each(?))",
            params![source, serde_json::to_string(ids)?],
        )
    }
    fn communications_work_from(&self, dsp: &str, from: i64) -> Result<Vec<TextConversation>> {
        self.dsp(dsp)?.query_as(
            "SELECT * FROM communications_text_conversations WHERE kind='work' AND last_at>=?",
            [from],
        )
    }
    fn communications_work(&self, dsp: &str) -> Result<Vec<TextConversation>> {
        self.dsp(dsp)?.query_as(
            "SELECT * FROM communications_text_conversations \
             WHERE kind='work' AND last_at IS NOT NULL",
            [],
        )
    }
    fn communications_work_numbers(&self, dsp: &str) -> Result<HashMap<String, Vec<String>>> {
        let texted: Vec<(String, String)> = self.dsp(dsp)?.query_as(
            "SELECT DISTINCT t.conversation_id,t.number FROM phones_texts t \
             JOIN communications_text_conversations c ON c.id=t.conversation_id \
             WHERE c.kind='work'",
            [],
        )?;
        let mut numbers: HashMap<String, Vec<String>> = HashMap::new();
        for (conversation, number) in texted {
            numbers.entry(conversation).or_default().push(number);
        }
        Ok(numbers)
    }
    fn waiting_communications_work(&self, dsp: &str) -> Result<Vec<TextConversation>> {
        self.dsp(dsp)?.query_as(
            "SELECT * FROM communications_text_conversations WHERE kind='work' AND state='waiting'",
            [],
        )
    }
    fn communications_waiting_from(
        &self,
        dsp: &str,
        ids: &[String],
        from: i64,
    ) -> Result<Vec<(String, i64)>> {
        self.dsp(dsp)?.query_as(
            concat!(
                "SELECT id,since FROM (SELECT c.id,",
                waiting_since!(),
                " AS since FROM communications_text_conversations c \
                 WHERE c.id IN (SELECT value FROM json_each(?2))) WHERE since IS NOT NULL"
            ),
            params![from, serde_json::to_string(ids)?],
        )
    }
    fn changed_communications_work(&self, dsp: &str, from: i64) -> Result<()> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let changed: Vec<(String,)> = db.query_as(
                "SELECT id FROM communications_text_conversations WHERE kind='work' \
                 AND (driver_id IS NOT NULL OR (state='waiting' AND last_at>=?1))",
                [from],
            )?;
            for (id,) in changed {
                changed_conversation(&db, &id)?;
            }
            Ok(())
        })
    }
    fn open_communications_text(&self, dsp: &str, filed: &Filed) -> Result<TextConversation> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let id = conversation(&db, filed)?;
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
            let at = now();
            let marked = db.exec(
                "UPDATE communications_text_conversations SET done_at=?,done_by=? \
                 WHERE id=? AND state='waiting'",
                params![at, by, id],
            )?;
            if marked > 0 {
                db.exec(
                    "UPDATE phones_texts SET done_at=?,done_by=? \
                     WHERE conversation_id=? AND direction='in' AND done_at IS NULL",
                    params![at, by, id],
                )?;
                settle(&db, id)?;
            }
            Ok(marked > 0)
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
    let id = crypto::id(TEXT_PREFIX)?;
    db.exec(
        "INSERT INTO communications_text_conversations (id,kind,source,driver_id,number) \
         VALUES (?,?,?,?,?)",
        params![id, kind, source, driver, number],
    )?;
    Ok(id)
}

/// The conversation the text `id` is filed in, if it's filed.
fn filed_in(db: &Db, id: &str) -> Result<Option<String>> {
    let found: Option<(Option<String>,)> =
        db.one_as("SELECT conversation_id FROM phones_texts WHERE id=?", [id])?;
    Ok(found.and_then(|(conversation,)| conversation))
}

/// The DSP's texts `sql` reads, each with its files as the phone side keeps them.
fn texts(db: &Store, dsp: &str, sql: &str, params: impl Params) -> Result<Vec<Text>> {
    let mut texts: Vec<Text> = db.dsp(dsp)?.query_as(sql, params)?;
    let ids: Vec<String> = texts.iter().map(|text| text.id.clone()).collect();
    let mut files = db.phones_files_of(dsp, &ids)?;
    for text in &mut texts {
        text.files = files.remove(&text.id).unwrap_or_default();
    }
    Ok(texts)
}

/// Keeps whether the conversation waits on an answer, and since when, from its texts and which a
/// Done covered, done when it was marked so after its latest answer, and notes it changed: by all
/// its texts, whichever day they were written (`waiting_since`, from the earliest).
fn settle(db: &Db, conversation: &str) -> Result<()> {
    let (since, done): (Option<i64>, bool) = db
        .one_as(
            concat!(
                "SELECT ",
                waiting_since!(),
                ",coalesce(c.done_at,-1)>coalesce(",
                last_out!(),
                ",-1) FROM communications_text_conversations c WHERE c.id=?2"
            ),
            params![i64::MIN, conversation],
        )?
        .ok_or_else(|| Error::new("communications_conversation_not_found", 404))?;
    let state = if since.is_some() {
        ConversationState::Waiting
    } else if done {
        ConversationState::Done
    } else {
        ConversationState::Responded
    };
    db.exec(
        "UPDATE communications_text_conversations SET state=?2,waiting_since=?3,\
         last_at=(SELECT max(at) FROM phones_texts WHERE conversation_id=?1) \
         WHERE id=?1",
        params![conversation, state, since],
    )?;
    changed_conversation(db, conversation)
}

/// Communications' migration 6: Done marks each text it covered (`communications_texts.done_at`,
/// `done_by`), so it stays handled wherever it's filed again; each text a conversation's Done
/// covered as it stands, written before it, is marked so. On the copy that kept the texts filed
/// until migration 9, which takes the marks onto the texts' own rows.
pub(crate) fn add_text_done(db: &Db) -> Result<()> {
    add_column(db, "communications_texts", "done_at", "INTEGER")?;
    add_column(db, "communications_texts", "done_by", "TEXT")?;
    db.exec(
        "UPDATE communications_texts SET \
         done_at=(SELECT c.done_at FROM communications_text_conversations c \
         WHERE c.id=communications_texts.conversation_id),\
         done_by=(SELECT c.done_by FROM communications_text_conversations c \
         WHERE c.id=communications_texts.conversation_id) \
         WHERE direction='in' AND done_at IS NULL AND at<=(SELECT c.done_at \
         FROM communications_text_conversations c WHERE c.id=communications_texts.conversation_id)",
        [],
    )?;
    Ok(())
}

/// Communications' migration 9: each text is filed on the phone side's row of it, `phones_texts`,
/// which alone keeps it from now on: the conversation it's in (`conversation_id`, none while it
/// waits to be filed), the name its sender was shown by as it was filed (`sender_name`), whether
/// its number was blocked as it came (`blocked`), and the Done that covered it (`done_at`,
/// `done_by`), each copied as `communications_texts` kept it, never worked out again. A text that
/// copy never had, as one changed after the copy last read the phone side, waits to be filed,
/// which the next upkeep does; a conversation whose texts changed after that read is settled from
/// them as they are. Migration 10 then drops the copy, and how far it read
/// (`communications_texting`). Each step is taken only if it wasn't.
pub(crate) fn add_filing(db: &Db) -> Result<()> {
    add_column(
        db,
        "phones_texts",
        "conversation_id",
        "TEXT REFERENCES communications_text_conversations (id)",
    )?;
    add_column(db, "phones_texts", "sender_name", "TEXT")?;
    add_column(db, "phones_texts", "blocked", "INTEGER NOT NULL DEFAULT 0")?;
    add_column(db, "phones_texts", "done_at", "INTEGER")?;
    add_column(db, "phones_texts", "done_by", "TEXT")?;
    db.exec(
        "CREATE INDEX IF NOT EXISTS phones_texts_filed ON phones_texts (conversation_id, at, id)",
        [],
    )?;
    db.exec(
        "CREATE INDEX IF NOT EXISTS phones_texts_written ON phones_texts (at)",
        [],
    )?;
    db.exec(
        "UPDATE phones_texts SET conversation_id=c.conversation_id,sender_name=c.sender_name,\
         blocked=c.blocked,done_at=c.done_at,done_by=c.done_by FROM communications_texts c \
         WHERE c.id=phones_texts.id AND phones_texts.conversation_id IS NULL",
        [],
    )?;
    let changed: Vec<(String,)> = db.query_as(
        "SELECT DISTINCT conversation_id FROM phones_texts WHERE conversation_id IS NOT NULL \
         AND change>(SELECT coalesce(max(read_to),0) FROM communications_texting)",
        [],
    )?;
    for (conversation,) in changed {
        settle(db, &conversation)?;
    }
    Ok(())
}
