//! Every text the DSP's phones relay: those a phone reports, each kept once by its own ID of it,
//! and those Dispatch sends from one, waiting for it until it says how each attempt went. Of each
//! text's row, `phones_texts`, the phone side keeps the transport's columns; the conversation it's
//! filed in, and what filing keeps beside it, are `texts::storage`'s.
use super::{
    devices::ONLINE_MS,
    files::{FileStore, ReportedFile},
    next_change,
};
use crate::backend::phone::{
    LIMITS,
    model::{Direction, EXPIRY_MS, Origin, Outgoing, PHONE_OFFLINE, PHONE_REMOVED, Status, Text},
    numbers::normal,
};
use dispatch_core::{
    Error, Result,
    db::{Db, Store, n, s},
    ensure,
    foundation::crypto,
};
use rusqlite::params;
use serde_json::Value;

/// The most texts one call reads or reports: a phone's outbox, what a phone reports at once, or a
/// page of the texts filing places.
pub const MOST_TEXTS: usize = 100;
/// How long after its time a text may come into a phone, as its app looks back that far for one
/// (`Inbox.LOOK_BACK_MS`), in milliseconds: a phone reports none from further before it paired.
const LOOK_BACK_MS: i64 = 24 * 60 * 60 * 1000;
/// How far ahead of the server's a phone's clock may be, in milliseconds.
const SKEW_MS: i64 = 5 * 60 * 1000;

/// A text as the phone reports it, by its own ID of it.
pub struct Reported {
    pub client_id: String,
    pub direction: Direction,
    pub number: String,
    pub body: String,
    pub at: i64,
    pub files: Vec<ReportedFile>,
}
/// When a text a phone paired at `paired` reports as written at `at` was, as it's kept: as the
/// phone says, from its app's look-back before it paired, however long a phone offline held it,
/// until its clock's skew after `received`, when the report came; else `received` itself, the
/// text kept all the same.
pub fn written(at: i64, paired: i64, received: i64) -> i64 {
    if (paired - LOOK_BACK_MS..=received + SKEW_MS).contains(&at) {
        at
    } else {
        received
    }
}

/// What a reported text became: its ID, and its files' in the order reported.
pub struct Kept {
    pub client_id: String,
    pub id: String,
    pub files: Vec<String>,
    /// Whether this report kept it: not one reported before.
    pub new: bool,
}

/// What a phone says of an attempt at a text Dispatch sent.
pub struct StatusReport {
    pub id: String,
    pub attempt: i64,
    pub status: Status,
    pub at: i64,
    pub error: Option<String>,
}

/// What the phone side keeps of the texts its phones relay.
pub trait TextStore {
    /// Keeps the texts a phone reports, each once by its own ID of it.
    fn phones_report(&self, dsp: &str, device: &str, texts: Vec<Reported>) -> Result<Vec<Kept>>;
    /// The texts Dispatch sent that wait for the phone, oldest first, each with its attempt.
    fn phones_outbox(&self, dsp: &str, device: &str) -> Result<Vec<(Text, i64)>>;
    /// Notes how the attempt `report.attempt` at a text Dispatch sent from the phone went: a
    /// status only moves on, waiting to sent to delivered, waiting or sent to failed, and to
    /// delivered from any other, as a delivery report proves it got there; and to sent from
    /// failed as `PHONE_OFFLINE`, as the phone sent it after all. Sent or failed counts only for
    /// the latest attempt, so a late report of an earlier one never cancels the text tried again;
    /// delivered, for any attempt of the phone the text waits on, or an earlier attempt of the
    /// phone that had it. Answers whether it moved.
    fn phones_status(&self, dsp: &str, device: &str, report: &StatusReport) -> Result<bool>;
    fn phones_text(&self, dsp: &str, id: &str) -> Result<Option<Text>>;
    /// Queues `outgoing` from the phone it names, else the one that last texted its number, else
    /// the one seen last, one online at `at` before any offline: refused as `invalid_input` for a
    /// number that isn't one, or a text with nothing in it or past `LIMITS`.
    fn phones_send(&self, dsp: &str, outgoing: &Outgoing, at: i64) -> Result<Text>;
    /// Queues again the text `id`, one Dispatch sent that failed, as a new attempt from the phone
    /// a new text to its number would go from: refused as `phones_none` while none is paired.
    fn phones_retry(&self, dsp: &str, id: &str, at: i64) -> Result<Text>;
    /// The texts Dispatch queued that wait past their expiry at `at` on a phone offline then: not
    /// checked in within `ONLINE_MS`.
    fn phones_expired(&self, dsp: &str, at: i64) -> Result<Vec<String>>;
    /// Fails each of `phones_expired` as `PHONE_OFFLINE`: those it failed.
    fn phones_expire(&self, dsp: &str, at: i64) -> Result<Vec<String>>;
    /// Has the demo's stand-in phone say the text it was given delivered, at once.
    fn phones_stand_in_delivered(&self, dsp: &str, id: &str, at: i64) -> Result<()>;
}

impl TextStore for Store {
    fn phones_report(&self, dsp: &str, device: &str, texts: Vec<Reported>) -> Result<Vec<Kept>> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let mut kept = Vec::with_capacity(texts.len());
            for text in texts {
                let found = db.one(
                    "SELECT id FROM phones_texts WHERE device_id=? AND client_id=?",
                    params![device, text.client_id],
                )?;
                if let Some(found) = found {
                    let id = s(&found, "id").to_owned();
                    let files = db
                        .all(
                            "SELECT id FROM phones_files WHERE text_id=? ORDER BY added_at,rowid",
                            [&id],
                        )?
                        .iter()
                        .map(|row| s(row, "id").to_owned())
                        .collect();
                    kept.push(Kept {
                        client_id: text.client_id,
                        id,
                        files,
                        new: false,
                    });
                    continue;
                }
                let id = crypto::id("text")?;
                let change = next_change(&db)?;
                db.exec(
                    "INSERT INTO phones_texts \
                     (id,device_id,client_id,direction,origin,number,body,at,change) \
                     VALUES (?,?,?,?,'phone',?,?,?,?)",
                    params![
                        id,
                        device,
                        text.client_id,
                        direction(text.direction),
                        text.number,
                        text.body,
                        text.at,
                        change
                    ],
                )?;
                let mut files = Vec::with_capacity(text.files.len());
                for file in text.files {
                    let file_id = crypto::id("file")?;
                    db.exec(
                        "INSERT INTO phones_files (id,text_id,name,mime,size,on_phone,added_at) \
                         VALUES (?,?,?,?,?,?,?)",
                        params![
                            file_id,
                            id,
                            file.name,
                            file.mime,
                            file.size,
                            file.on_phone,
                            text.at
                        ],
                    )?;
                    files.push(file_id);
                }
                kept.push(Kept {
                    client_id: text.client_id,
                    id,
                    files,
                    new: true,
                });
            }
            Ok(kept)
        })
    }
    fn phones_outbox(&self, dsp: &str, device: &str) -> Result<Vec<(Text, i64)>> {
        let db = self.dsp(dsp)?;
        let rows = db.all(
            "SELECT * FROM phones_texts WHERE device_id=? AND origin='dispatch' \
             AND status='waiting' ORDER BY at,rowid LIMIT ?",
            params![device, MOST_TEXTS as i64],
        )?;
        drop(db);
        let attempts: Vec<i64> = rows.iter().map(|row| n(row, "attempt")).collect();
        Ok(texts(self, dsp, rows)?.into_iter().zip(attempts).collect())
    }
    fn phones_status(&self, dsp: &str, device: &str, report: &StatusReport) -> Result<bool> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let found = db.one(
                "SELECT status,attempt,error,device_id=?2 AS current FROM phones_texts t \
                 WHERE id=?1 AND origin='dispatch' AND (device_id=?2 OR EXISTS (\
                 SELECT 1 FROM phones_attempts a WHERE a.text_id=t.id AND a.device_id=?2 \
                 AND a.attempt=?3))",
                params![report.id, device, report.attempt],
            )?;
            let Some(found) = found else {
                return Ok(false);
            };
            let from = parse_status(s(&found, "status"));
            let latest = n(&found, "current") == 1 && n(&found, "attempt") == report.attempt;
            // Failed as its phone was offline, the phone may yet have sent it before it heard.
            let offline = from == Some(Status::Failed) && s(&found, "error") == PHONE_OFFLINE;
            let moves = match report.status {
                Status::Waiting => false,
                Status::Sent => latest && (from == Some(Status::Waiting) || offline),
                Status::Failed => latest && matches!(from, Some(Status::Waiting | Status::Sent)),
                // A delivery report proves it got there, whichever attempt it was.
                Status::Delivered => from != Some(Status::Delivered),
            };
            if !moves {
                return Ok(false);
            }
            let change = next_change(&db)?;
            db.exec(
                "UPDATE phones_texts SET status=?, status_at=?, error=?, change=? WHERE id=?",
                params![
                    status_text(report.status),
                    report.at,
                    report
                        .error
                        .as_deref()
                        .filter(|_| report.status == Status::Failed),
                    change,
                    report.id
                ],
            )?;
            Ok(true)
        })
    }
    fn phones_text(&self, dsp: &str, id: &str) -> Result<Option<Text>> {
        let rows = self
            .dsp(dsp)?
            .all("SELECT * FROM phones_texts WHERE id=?", [id])?;
        Ok(texts(self, dsp, rows)?.pop())
    }
    fn phones_send(&self, dsp: &str, outgoing: &Outgoing, at: i64) -> Result<Text> {
        let to = normal(&outgoing.to).ok_or_else(|| Error::new("invalid_input", 400))?;
        ensure(
            (!outgoing.body.trim().is_empty() || !outgoing.files.is_empty())
                && outgoing.body.chars().count() <= LIMITS.body
                && outgoing.files.len() <= LIMITS.files,
            "invalid_input",
            400,
        )?;
        let id = crypto::id("text")?;
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let device = match &outgoing.phone {
                Some(phone) => db
                    .one(
                        "SELECT id FROM phones_devices WHERE id=? AND removed_at IS NULL",
                        [phone],
                    )?
                    .map(|found| s(&found, "id").to_owned())
                    .ok_or_else(|| Error::new("phones_not_found", 404))?,
                None => sender(&db, &to, at)?,
            };
            let change = next_change(&db)?;
            db.exec(
                "INSERT INTO phones_texts \
                 (id,device_id,direction,origin,number,body,at,sent_by,status,status_at,attempt,\
                 change) VALUES (?1,?2,'out','dispatch',?3,?4,?5,?6,'waiting',?5,?7,?7)",
                params![
                    id,
                    device,
                    to,
                    outgoing.body,
                    at,
                    outgoing.by,
                    change
                ],
            )?;
            for file in &outgoing.files {
                let attached = db.exec(
                    "UPDATE phones_files SET text_id=? WHERE id=? AND text_id IS NULL AND added_by=?",
                    params![id, file, outgoing.by],
                )?;
                ensure(attached == 1, "phones_file_not_found", 404)?;
            }
            Ok(())
        })?;
        drop(db);
        self.phones_text(dsp, &id)?
            .ok_or_else(|| Error::new("phones_text_not_found", 404))
    }
    fn phones_retry(&self, dsp: &str, id: &str, at: i64) -> Result<Text> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let found = db
                .one(
                    "SELECT number FROM phones_texts WHERE id=? AND origin='dispatch' \
                     AND status='failed'",
                    [id],
                )?
                .ok_or_else(|| Error::new("phones_text_not_found", 404))?;
            // A new attempt, from the phone a new text to its number would go from; the one
            // before kept by the phone that had it, whose delivery report of it still counts.
            let device = sender(&db, s(&found, "number"), at)?;
            db.exec(
                "INSERT INTO phones_attempts (text_id,attempt,device_id) \
                 SELECT id,attempt,device_id FROM phones_texts WHERE id=?",
                [id],
            )?;
            let change = next_change(&db)?;
            db.exec(
                "UPDATE phones_texts SET device_id=?1, status='waiting', status_at=?2, \
                 attempt=?3, error=NULL, change=?3 WHERE id=?4",
                params![device, at, change, id],
            )?;
            Ok(())
        })?;
        drop(db);
        self.phones_text(dsp, id)?
            .ok_or_else(|| Error::new("phones_text_not_found", 404))
    }
    fn phones_expired(&self, dsp: &str, at: i64) -> Result<Vec<String>> {
        let expired: Vec<(String,)> = self.dsp(dsp)?.query_as(
            "SELECT t.id FROM phones_devices d JOIN phones_texts t ON t.device_id=d.id \
             WHERE t.status='waiting' AND t.origin='dispatch' AND t.status_at<=? \
             AND coalesce(d.last_seen_at,0)<? ORDER BY t.at,t.id",
            [at - EXPIRY_MS, at - ONLINE_MS],
        )?;
        Ok(expired.into_iter().map(|(id,)| id).collect())
    }
    fn phones_expire(&self, dsp: &str, at: i64) -> Result<Vec<String>> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let expired = self.phones_expired(dsp, at)?;
            for id in &expired {
                let change = next_change(&db)?;
                db.exec(
                    "UPDATE phones_texts SET status='failed', status_at=?, error=?, change=? \
                     WHERE id=?",
                    params![at, PHONE_OFFLINE, change, id],
                )?;
            }
            Ok(expired)
        })
    }
    fn phones_stand_in_delivered(&self, dsp: &str, id: &str, at: i64) -> Result<()> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let change = next_change(&db)?;
            db.exec(
                "UPDATE phones_texts SET status='delivered', status_at=?, change=? \
                 WHERE id=? AND device_id IN (SELECT id FROM phones_devices WHERE stand_in=1)",
                params![at, change, id],
            )?;
            Ok(())
        })
    }
}

/// The phone a text to `to` goes from at `at`, of the DSP's paired ones: the one that last texted
/// it, else the one seen last, one online before any offline; refused as `phones_none` with none.
fn sender(db: &Db, to: &str, at: i64) -> Result<String> {
    let found = db
        .one(
            "SELECT d.id FROM phones_devices d \
             LEFT JOIN phones_texts t ON t.device_id=d.id AND t.number=?1 \
             WHERE d.removed_at IS NULL \
             GROUP BY d.id ORDER BY coalesce(d.last_seen_at,0)>=?2 DESC, \
             max(t.at) IS NULL, max(t.at) DESC, \
             coalesce(d.last_seen_at,0) DESC, d.paired_at LIMIT 1",
            params![to, at - ONLINE_MS],
        )?
        .ok_or_else(|| Error::new("phones_none", 409))?;
    Ok(s(&found, "id").to_owned())
}

/// Fails the texts Dispatch queued that still wait on the phone `device`, which was removed:
/// those it failed. Inside the write that removed it.
pub(super) fn orphaned(db: &Db, device: &str, at: i64) -> Result<Vec<String>> {
    let waiting = db.all(
        "SELECT id FROM phones_texts WHERE device_id=? AND origin='dispatch' AND status='waiting'",
        [device],
    )?;
    let mut failed = Vec::with_capacity(waiting.len());
    for text in &waiting {
        let change = next_change(db)?;
        db.exec(
            "UPDATE phones_texts SET status='failed', status_at=?, error=?, change=? WHERE id=?",
            params![at, PHONE_REMOVED, change, s(text, "id")],
        )?;
        failed.push(s(text, "id").to_owned());
    }
    Ok(failed)
}

/// The DSP's texts from their rows, each with its files.
fn texts(db: &Store, dsp: &str, rows: Vec<Value>) -> Result<Vec<Text>> {
    let ids: Vec<String> = rows.iter().map(|row| s(row, "id").to_owned()).collect();
    let mut files = db.phones_files_of(dsp, &ids)?;
    Ok(rows
        .iter()
        .map(|row| Text {
            id: s(row, "id").into(),
            phone: s(row, "device_id").into(),
            direction: if s(row, "direction") == "in" {
                Direction::In
            } else {
                Direction::Out
            },
            origin: if s(row, "origin") == "dispatch" {
                Origin::Dispatch
            } else {
                Origin::Phone
            },
            number: s(row, "number").into(),
            body: s(row, "body").into(),
            at: n(row, "at"),
            sent_by: row["sent_by"].as_str().map(str::to_owned),
            status: parse_status(s(row, "status")),
            status_at: row["status_at"].as_i64(),
            error: row["error"].as_str().map(str::to_owned),
            files: files.remove(s(row, "id")).unwrap_or_default(),
            change: n(row, "change"),
        })
        .collect())
}
fn direction(direction: Direction) -> &'static str {
    match direction {
        Direction::In => "in",
        Direction::Out => "out",
    }
}
fn status_text(status: Status) -> &'static str {
    match status {
        Status::Waiting => "waiting",
        Status::Sent => "sent",
        Status::Delivered => "delivered",
        Status::Failed => "failed",
    }
}
fn parse_status(text: &str) -> Option<Status> {
    match text {
        "waiting" => Some(Status::Waiting),
        "sent" => Some(Status::Sent),
        "delivered" => Some(Status::Delivered),
        "failed" => Some(Status::Failed),
        _ => None,
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/phone/storage/texts.rs"]
mod tests;
