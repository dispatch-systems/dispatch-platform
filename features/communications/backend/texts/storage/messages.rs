//! Each conversation by text and the texts filed in it, how far Phones' changes were read, and
//! whether each conversation waits, kept beside its texts each time they change.
use crate::backend::{
    changes::changed_conversation,
    model::{ConversationState, TEXT_PREFIX},
    texts::model::{Filed, Incoming, Text, TextConversation},
};
use dispatch_core::{
    Error, Result,
    db::{Db, Store, now},
    foundation::crypto,
};
use rusqlite::params;
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
    fn communications_text(&self, dsp: &str, id: &str) -> Result<Option<Text>>;
    /// Every text written from `from` until `to`, oldest first.
    fn communications_texts_between(&self, dsp: &str, from: i64, to: i64) -> Result<Vec<Text>>;
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
    /// The conversations of the drivers `ids` of the source `source`, in no order.
    fn communications_drivers(
        &self,
        dsp: &str,
        source: &str,
        ids: &[String],
    ) -> Result<Vec<TextConversation>>;
    /// The work conversations, drivers' and work phones' nobody held, holding a text written from
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
    /// began or ended; and each of the drivers `drivers`', by the source of their ID and the ID,
    /// texted in or not, as the work phone they hold may have changed.
    fn changed_communications_work(
        &self,
        dsp: &str,
        from: i64,
        drivers: &[(String, String)],
    ) -> Result<()>;
    /// The conversation texts filed so would be in, made with no text if it's new.
    fn open_communications_text(&self, dsp: &str, filed: &Filed) -> Result<TextConversation>;
    /// Marks the conversation done without an answer, as the member `by`, while it waits:
    /// whether it was waiting.
    fn mark_communications_text_done(&self, dsp: &str, id: &str, by: &str) -> Result<bool>;
}

/// When the conversation `c`'s latest text out that didn't fail was written.
macro_rules! last_out {
    () => {
        "(SELECT max(o.at) FROM communications_texts o WHERE o.conversation_id=c.id \
         AND o.direction='out' AND coalesce(o.status,'sent')<>'failed')"
    };
}
/// Since when the conversation `c` waits, on texts written from `?1` on: the first text in, but
/// one that came while its number was blocked, after the latest text out that didn't fail and
/// after it was marked done; none when it waits on nothing written from then.
macro_rules! waiting_since {
    () => {
        concat!(
            "(SELECT min(t.at) FROM communications_texts t WHERE t.conversation_id=c.id \
             AND t.direction='in' AND t.blocked=0 AND t.at>=?1 AND t.at>coalesce(c.done_at,-1) \
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
    fn communications_texts_between(&self, dsp: &str, from: i64, to: i64) -> Result<Vec<Text>> {
        self.dsp(dsp)?.query_as(
            "SELECT * FROM communications_texts WHERE at>=?1 AND at<?2 ORDER BY at,id",
            [from, to],
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
                let was = filed_in(&db, &text.id)?;
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
                touched.extend(was);
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
            match filed_in(&db, id)? {
                Some(conversation) => changed_conversation(&db, &conversation),
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
                let Some(was) = filed_in(&db, text)? else {
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
            "SELECT DISTINCT t.conversation_id,t.number FROM communications_texts t \
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
    fn changed_communications_work(
        &self,
        dsp: &str,
        from: i64,
        drivers: &[(String, String)],
    ) -> Result<()> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let changed: Vec<(String,)> = db.query_as(
                "SELECT id FROM communications_text_conversations WHERE kind='work' \
                 AND ((state='waiting' AND last_at>=?1) OR (source,driver_id) IN \
                 (SELECT json_extract(value,'$[0]'),json_extract(value,'$[1]') FROM json_each(?2)))",
                params![from, serde_json::to_string(drivers)?],
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

/// The conversation the text `id` is kept in, if it's kept.
fn filed_in(db: &Db, id: &str) -> Result<Option<String>> {
    let found: Option<(String,)> = db.one_as(
        "SELECT conversation_id FROM communications_texts WHERE id=?",
        [id],
    )?;
    Ok(found.map(|(conversation,)| conversation))
}

/// Keeps whether the conversation waits on an answer, and since when, from its texts and when
/// it was marked done, and notes it changed: by all its texts, whichever day they were written
/// (`waiting_since`, from the earliest).
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
         last_at=(SELECT max(at) FROM communications_texts WHERE conversation_id=?1) \
         WHERE id=?1",
        params![conversation, state, since],
    )?;
    changed_conversation(db, conversation)
}
