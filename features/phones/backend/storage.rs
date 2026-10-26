//! Phones' storage, in each DSP's own database: its phones, the code that pairs the next one,
//! and every text and picture they relay. Only this crate writes SQL against its tables.
use super::numbers;
use dispatch_core::{
    Error, Result,
    db::{self, Db, Store, n, s},
    ensure,
    foundation::crypto,
    server::cache::DataDomain,
};
use dispatch_phones_contract::{Changes, Direction, File, Origin, Outgoing, Phone, Status, Text};
use rusqlite::params;
use serde_json::Value;
use std::collections::BTreeMap;

/// What Phones' writes change, for the read cache.
pub const DOMAIN: DataDomain = DataDomain::new("phones");
/// How lately a phone must have checked in to be online: twice the longest it waits on Dispatch,
/// and then some.
pub const ONLINE_MS: i64 = 2 * 60 * 1000;
/// How long a pairing code works.
pub const PAIRING_MS: i64 = 10 * 60 * 1000;
/// The most texts one call reads or reports.
pub const MOST_TEXTS: usize = 100;

/// One of the DSP's phones, as kept.
#[derive(Clone, Debug, PartialEq)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub number: String,
    pub model: String,
    pub app_version: String,
    pub paired_at: i64,
    pub last_seen_at: Option<i64>,
}
impl Device {
    fn of(row: &Value) -> Self {
        Self {
            id: s(row, "id").into(),
            name: s(row, "name").into(),
            number: s(row, "number").into(),
            model: s(row, "model").into(),
            app_version: s(row, "app_version").into(),
            paired_at: n(row, "paired_at"),
            last_seen_at: row["last_seen_at"].as_i64(),
        }
    }
    pub fn online(&self, now: i64) -> bool {
        self.last_seen_at.is_some_and(|at| now - at <= ONLINE_MS)
    }
    pub fn phone(&self, now: i64) -> Phone {
        Phone {
            id: self.id.clone(),
            name: self.name.clone(),
            number: self.number.clone(),
            online: self.online(now),
            last_seen_at: self.last_seen_at,
        }
    }
}

/// What a phone tells Dispatch of itself as it pairs.
pub struct Pairing<'a> {
    pub number: &'a str,
    pub model: &'a str,
    pub app_version: &'a str,
}

/// A text as the phone reports it, by its own ID of it.
pub struct Reported {
    pub client_id: String,
    pub direction: Direction,
    pub number: String,
    pub body: String,
    pub at: i64,
    pub files: Vec<ReportedFile>,
}
pub struct ReportedFile {
    pub name: String,
    pub mime: String,
    pub size: i64,
}
/// What a reported text became: its ID, and its files' in the order reported.
pub struct Kept {
    pub client_id: String,
    pub id: String,
    pub files: Vec<String>,
}

/// What a phone says of an attempt at a text Dispatch sent.
pub struct StatusReport {
    pub id: String,
    pub attempt: i64,
    pub status: Status,
    pub at: i64,
    pub error: Option<String>,
}

/// A picture or file as kept, with where its bytes are.
pub struct StoredFile {
    pub file: File,
    pub sha256: Option<String>,
    pub text_id: Option<String>,
    pub added_by: Option<String>,
}

/// What Phones reads and writes for one DSP.
pub trait PhonesStore {
    fn phones_devices(&self, dsp: &str) -> Result<Vec<Device>>;
    fn phones_device(&self, dsp: &str, id: &str) -> Result<Option<Device>>;
    /// The phone holding the key `key_sha256`, not removed.
    fn phones_device_by_key(&self, dsp: &str, key_sha256: &str) -> Result<Option<Device>>;
    /// Notes that the phone checked in.
    fn phones_seen(&self, dsp: &str, device: &str, at: i64) -> Result<()>;
    fn phones_rename(&self, dsp: &str, id: &str, name: &str) -> Result<Device>;
    fn phones_remove(&self, dsp: &str, id: &str, at: i64) -> Result<Device>;
    /// Keeps the code that pairs the next phone, in place of any not used yet.
    fn phones_new_pairing(&self, dsp: &str, by: &str, code_sha256: &str, at: i64) -> Result<()>;
    /// Pairs a phone by the code `code_sha256`, giving it the key `key_sha256`: refused as
    /// `phones_pairing_invalid` for a code used, expired or unknown.
    fn phones_pair(
        &self,
        dsp: &str,
        code_sha256: &str,
        key_sha256: &str,
        pairing: &Pairing,
        at: i64,
    ) -> Result<Device>;
    /// Keeps the texts a phone reports, each once by its own ID of it.
    fn phones_report(&self, dsp: &str, device: &str, texts: Vec<Reported>) -> Result<Vec<Kept>>;
    /// Keeps the bytes of a file a phone reported, by their SHA-256: refused as
    /// `phones_file_not_found` for one not of its own texts. Answers the SHA-256 it kept before,
    /// for bytes sent again.
    fn phones_file_arrived(
        &self,
        dsp: &str,
        device: &str,
        id: &str,
        sha256: &str,
    ) -> Result<Option<String>>;
    /// The texts Dispatch sent that wait for the phone, oldest first, each with its attempt.
    fn phones_outbox(&self, dsp: &str, device: &str) -> Result<Vec<(Text, i64)>>;
    /// Notes how the attempt `report.attempt` at a text Dispatch sent from the phone went: a
    /// status only moves on, waiting to sent to delivered, waiting or sent to failed, and to
    /// delivered from any other, as a delivery report proves it got there. Sent or failed counts
    /// only for the latest attempt, so a late report of an earlier one never cancels the text
    /// tried again. Answers whether it moved.
    fn phones_status(&self, dsp: &str, device: &str, report: &StatusReport) -> Result<bool>;
    fn phones_changes(&self, dsp: &str, after: i64, most: usize) -> Result<Changes>;
    fn phones_text(&self, dsp: &str, id: &str) -> Result<Option<Text>>;
    fn phones_send(&self, dsp: &str, outgoing: &Outgoing, at: i64) -> Result<Text>;
    fn phones_retry(&self, dsp: &str, id: &str, at: i64) -> Result<Text>;
    /// Keeps a picture someone added for a text not sent yet.
    fn phones_add_file(
        &self,
        dsp: &str,
        by: &str,
        file: &ReportedFile,
        sha256: &str,
        at: i64,
    ) -> Result<File>;
    fn phones_file(&self, dsp: &str, id: &str) -> Result<Option<StoredFile>>;
    /// Lets go of pictures added and never sent, and pairing codes expired, before `before`:
    /// the SHA-256 of each picture let go.
    fn phones_tidy(&self, dsp: &str, before: i64) -> Result<Vec<String>>;
    /// Whether any file is kept by `sha256`.
    fn phones_bytes_kept(&self, dsp: &str, sha256: &str) -> Result<bool>;
    /// Whether the key `key_sha256` was a phone's that was removed.
    fn phones_key_removed(&self, dsp: &str, key_sha256: &str) -> Result<bool>;
    /// Who made the code that paired the phone.
    fn phones_paired_by(&self, dsp: &str, device: &str) -> Result<Option<String>>;
    /// A picture of a text Dispatch has for the phone `device` to send.
    fn phones_outgoing_file(&self, dsp: &str, device: &str, id: &str)
    -> Result<Option<StoredFile>>;
    /// Whether the phone is a stand-in, the demo's, which no app holds.
    fn phones_stand_in(&self, dsp: &str, device: &str) -> Result<bool>;
    /// Pairs a stand-in phone, `number`, as the demo's, with texts it received.
    fn phones_demo(&self, dsp: &str, number: &str, received: Vec<Reported>, at: i64) -> Result<()>;
}

impl PhonesStore for Store {
    fn phones_devices(&self, dsp: &str) -> Result<Vec<Device>> {
        let db = self.dsp(dsp)?;
        let rows = db.all(
            "SELECT * FROM phones_devices WHERE removed_at IS NULL ORDER BY paired_at,id",
            [],
        )?;
        Ok(rows.iter().map(Device::of).collect())
    }
    fn phones_device(&self, dsp: &str, id: &str) -> Result<Option<Device>> {
        let db = self.dsp(dsp)?;
        Ok(db
            .one(
                "SELECT * FROM phones_devices WHERE id=? AND removed_at IS NULL",
                [id],
            )?
            .as_ref()
            .map(Device::of))
    }
    fn phones_device_by_key(&self, dsp: &str, key_sha256: &str) -> Result<Option<Device>> {
        let db = self.dsp(dsp)?;
        Ok(db
            .one(
                "SELECT * FROM phones_devices WHERE key_sha256=? AND removed_at IS NULL",
                [key_sha256],
            )?
            .as_ref()
            .map(Device::of))
    }
    fn phones_seen(&self, dsp: &str, device: &str, at: i64) -> Result<()> {
        self.dsp(dsp)?.exec(
            "UPDATE phones_devices SET last_seen_at=?1 WHERE id=?2 AND coalesce(last_seen_at,0)<?1",
            params![at, device],
        )?;
        Ok(())
    }
    fn phones_rename(&self, dsp: &str, id: &str, name: &str) -> Result<Device> {
        let db = self.dsp(dsp)?;
        let changed = db.exec(
            "UPDATE phones_devices SET name=? WHERE id=? AND removed_at IS NULL",
            params![name, id],
        )?;
        ensure(changed == 1, "phones_not_found", 404)?;
        drop(db);
        self.phones_device(dsp, id)?
            .ok_or_else(|| Error::new("phones_not_found", 404))
    }
    fn phones_remove(&self, dsp: &str, id: &str, at: i64) -> Result<Device> {
        let device = self
            .phones_device(dsp, id)?
            .ok_or_else(|| Error::new("phones_not_found", 404))?;
        self.dsp(dsp)?.exec(
            "UPDATE phones_devices SET removed_at=? WHERE id=?",
            params![at, id],
        )?;
        Ok(device)
    }
    fn phones_new_pairing(&self, dsp: &str, by: &str, code_sha256: &str, at: i64) -> Result<()> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            db.exec("DELETE FROM phones_pairings WHERE used_at IS NULL", [])?;
            db.exec(
                "INSERT INTO phones_pairings (code_sha256,created_by,created_at,expires_at) \
                 VALUES (?,?,?,?)",
                params![code_sha256, by, at, at + PAIRING_MS],
            )?;
            Ok(())
        })
    }
    fn phones_pair(
        &self,
        dsp: &str,
        code_sha256: &str,
        key_sha256: &str,
        pairing: &Pairing,
        at: i64,
    ) -> Result<Device> {
        let db = self.dsp(dsp)?;
        let id = crypto::id("phone")?;
        let number = numbers::normal(pairing.number).unwrap_or_default();
        db.transaction(|| {
            let code = db
                .one(
                    "SELECT created_by FROM phones_pairings \
                     WHERE code_sha256=? AND used_at IS NULL AND expires_at>?",
                    params![code_sha256, at],
                )?
                .ok_or_else(|| Error::new("phones_pairing_invalid", 401))?;
            let count = db.count(
                "SELECT count(*) FROM phones_devices WHERE removed_at IS NULL",
                [],
            )?;
            let name = if count == 0 {
                "Dispatch".to_owned()
            } else {
                format!("Phone {}", count + 1)
            };
            db.exec(
                "INSERT INTO phones_devices \
                 (id,name,number,model,app_version,key_sha256,paired_at,paired_by,last_seen_at) \
                 VALUES (?,?,?,?,?,?,?,?,?)",
                params![
                    id,
                    name,
                    number,
                    pairing.model,
                    pairing.app_version,
                    key_sha256,
                    at,
                    s(&code, "created_by"),
                    at
                ],
            )?;
            db.exec(
                "UPDATE phones_pairings SET used_at=?, device_id=? WHERE code_sha256=?",
                params![at, id, code_sha256],
            )?;
            Ok(())
        })?;
        drop(db);
        self.phones_device(dsp, &id)?
            .ok_or_else(|| Error::new("phones_not_found", 404))
    }
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
                        "INSERT INTO phones_files (id,text_id,name,mime,size,added_at) \
                         VALUES (?,?,?,?,?,?)",
                        params![file_id, id, file.name, file.mime, file.size, text.at],
                    )?;
                    files.push(file_id);
                }
                kept.push(Kept {
                    client_id: text.client_id,
                    id,
                    files,
                });
            }
            Ok(kept)
        })
    }
    fn phones_file_arrived(
        &self,
        dsp: &str,
        device: &str,
        id: &str,
        sha256: &str,
    ) -> Result<Option<String>> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let file = db
                .one(
                    "SELECT f.text_id,f.sha256 FROM phones_files f \
                     JOIN phones_texts t ON t.id=f.text_id \
                     WHERE f.id=? AND t.device_id=? AND t.origin='phone'",
                    params![id, device],
                )?
                .ok_or_else(|| Error::new("phones_file_not_found", 404))?;
            db.exec(
                "UPDATE phones_files SET sha256=? WHERE id=?",
                params![sha256, id],
            )?;
            let change = next_change(&db)?;
            db.exec(
                "UPDATE phones_texts SET change=? WHERE id=?",
                params![change, s(&file, "text_id")],
            )?;
            Ok(file["sha256"]
                .as_str()
                .map(str::to_owned)
                .filter(|before| before != sha256))
        })
    }
    fn phones_outbox(&self, dsp: &str, device: &str) -> Result<Vec<(Text, i64)>> {
        let db = self.dsp(dsp)?;
        let rows = db.all(
            "SELECT * FROM phones_texts WHERE device_id=? AND origin='dispatch' \
             AND status='waiting' ORDER BY at,rowid LIMIT ?",
            params![device, MOST_TEXTS as i64],
        )?;
        let attempts: Vec<i64> = rows.iter().map(|row| n(row, "attempt")).collect();
        Ok(texts(&db, rows)?.into_iter().zip(attempts).collect())
    }
    fn phones_status(&self, dsp: &str, device: &str, report: &StatusReport) -> Result<bool> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let found = db.one(
                "SELECT status,attempt FROM phones_texts \
                 WHERE id=? AND device_id=? AND origin='dispatch'",
                params![report.id, device],
            )?;
            let Some(found) = found else {
                return Ok(false);
            };
            let from = parse_status(s(&found, "status"));
            let latest = found["attempt"].as_i64() == Some(report.attempt);
            let moves = match report.status {
                Status::Waiting => false,
                Status::Sent => latest && from == Some(Status::Waiting),
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
    fn phones_changes(&self, dsp: &str, after: i64, most: usize) -> Result<Changes> {
        let db = self.dsp(dsp)?;
        let most = most.clamp(1, MOST_TEXTS);
        let rows = db.all(
            "SELECT * FROM phones_texts WHERE change>? ORDER BY change LIMIT ?",
            params![after, most as i64],
        )?;
        // A full page goes on from its own last change; else every change there is was read.
        let last = match rows.last() {
            Some(row) if rows.len() == most => n(row, "change"),
            _ => n(
                &db.one("SELECT last FROM phones_change WHERE id=1", [])?
                    .unwrap_or_default(),
                "last",
            ),
        };
        Ok(Changes {
            texts: texts(&db, rows)?,
            last,
        })
    }
    fn phones_text(&self, dsp: &str, id: &str) -> Result<Option<Text>> {
        let db = self.dsp(dsp)?;
        let rows = db.all("SELECT * FROM phones_texts WHERE id=?", [id])?;
        Ok(texts(&db, rows)?.pop())
    }
    fn phones_send(&self, dsp: &str, outgoing: &Outgoing, at: i64) -> Result<Text> {
        let to = numbers::normal(&outgoing.to).ok_or_else(|| Error::new("invalid_input", 400))?;
        let id = crypto::id("text")?;
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let device = match &outgoing.phone {
                Some(phone) => db
                    .one(
                        "SELECT id FROM phones_devices WHERE id=? AND removed_at IS NULL",
                        [phone],
                    )?
                    .ok_or_else(|| Error::new("phones_not_found", 404))?,
                None => db
                    .one(
                        "SELECT d.id FROM phones_devices d \
                         LEFT JOIN phones_texts t ON t.device_id=d.id AND t.number=? \
                         WHERE d.removed_at IS NULL \
                         GROUP BY d.id ORDER BY max(t.at) IS NULL, max(t.at) DESC, \
                         coalesce(d.last_seen_at,0) DESC, d.paired_at LIMIT 1",
                        [&to],
                    )?
                    .ok_or_else(|| Error::new("phones_none", 409))?,
            };
            let change = next_change(&db)?;
            db.exec(
                "INSERT INTO phones_texts \
                 (id,device_id,direction,origin,number,body,at,sent_by,status,status_at,attempt,\
                 change) VALUES (?1,?2,'out','dispatch',?3,?4,?5,?6,'waiting',?5,?5,?7)",
                params![
                    id,
                    s(&device, "id"),
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
            let change = next_change(&db)?;
            // A new attempt, from the phone it went from, while that phone is still paired.
            let changed = db.exec(
                "UPDATE phones_texts SET status='waiting', status_at=?1, attempt=?1, error=NULL, \
                 change=?2 WHERE id=?3 AND origin='dispatch' AND status='failed' \
                 AND device_id IN (SELECT id FROM phones_devices WHERE removed_at IS NULL)",
                params![at, change, id],
            )?;
            ensure(changed == 1, "phones_text_not_found", 404)
        })?;
        drop(db);
        self.phones_text(dsp, id)?
            .ok_or_else(|| Error::new("phones_text_not_found", 404))
    }
    fn phones_add_file(
        &self,
        dsp: &str,
        by: &str,
        file: &ReportedFile,
        sha256: &str,
        at: i64,
    ) -> Result<File> {
        let id = crypto::id("file")?;
        self.dsp(dsp)?.exec(
            "INSERT INTO phones_files (id,added_by,name,mime,size,sha256,added_at) \
             VALUES (?,?,?,?,?,?,?)",
            params![id, by, file.name, file.mime, file.size, sha256, at],
        )?;
        Ok(File {
            id,
            name: file.name.clone(),
            mime: file.mime.clone(),
            size: file.size,
            kept: true,
        })
    }
    fn phones_file(&self, dsp: &str, id: &str) -> Result<Option<StoredFile>> {
        let db = self.dsp(dsp)?;
        Ok(db
            .one("SELECT * FROM phones_files WHERE id=?", [id])?
            .map(|row| StoredFile {
                file: file_of(&row),
                sha256: row["sha256"].as_str().map(str::to_owned),
                text_id: row["text_id"].as_str().map(str::to_owned),
                added_by: row["added_by"].as_str().map(str::to_owned),
            }))
    }
    fn phones_tidy(&self, dsp: &str, before: i64) -> Result<Vec<String>> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let loose: Vec<String> = db
                .all(
                    "SELECT sha256 FROM phones_files WHERE text_id IS NULL AND added_at<?",
                    [before],
                )?
                .iter()
                .filter_map(|row| row["sha256"].as_str().map(str::to_owned))
                .collect();
            db.exec(
                "DELETE FROM phones_files WHERE text_id IS NULL AND added_at<?",
                [before],
            )?;
            db.exec(
                "DELETE FROM phones_pairings WHERE used_at IS NULL AND expires_at<?",
                [db::now()],
            )?;
            Ok(loose)
        })
    }
    fn phones_bytes_kept(&self, dsp: &str, sha256: &str) -> Result<bool> {
        Ok(self
            .dsp(dsp)?
            .count("SELECT count(*) FROM phones_files WHERE sha256=?", [sha256])?
            > 0)
    }
    fn phones_key_removed(&self, dsp: &str, key_sha256: &str) -> Result<bool> {
        Ok(self.dsp(dsp)?.count(
            "SELECT count(*) FROM phones_devices WHERE key_sha256=? AND removed_at IS NOT NULL",
            [key_sha256],
        )? > 0)
    }
    fn phones_paired_by(&self, dsp: &str, device: &str) -> Result<Option<String>> {
        Ok(self
            .dsp(dsp)?
            .one("SELECT paired_by FROM phones_devices WHERE id=?", [device])?
            .map(|row| s(&row, "paired_by").to_owned())
            .filter(|by| !by.is_empty()))
    }
    fn phones_outgoing_file(
        &self,
        dsp: &str,
        device: &str,
        id: &str,
    ) -> Result<Option<StoredFile>> {
        let found = self.dsp(dsp)?.count(
            "SELECT count(*) FROM phones_files f JOIN phones_texts t ON t.id=f.text_id \
             WHERE f.id=? AND t.device_id=? AND t.origin='dispatch'",
            params![id, device],
        )? > 0;
        if !found {
            return Ok(None);
        }
        self.phones_file(dsp, id)
    }
    fn phones_stand_in(&self, dsp: &str, device: &str) -> Result<bool> {
        Ok(self.dsp(dsp)?.count(
            "SELECT count(*) FROM phones_devices WHERE id=? AND stand_in=1",
            [device],
        )? > 0)
    }
    fn phones_demo(&self, dsp: &str, number: &str, received: Vec<Reported>, at: i64) -> Result<()> {
        let id = crypto::id("phone")?;
        self.dsp(dsp)?.exec(
            "INSERT INTO phones_devices \
             (id,name,number,model,app_version,key_sha256,paired_at,paired_by,last_seen_at,stand_in) \
             VALUES (?,'Dispatch',?,'Stand-in','',?,?,'',?,1)",
            params![id, number, crypto::sha(crypto::random::<32>()?), at, at],
        )?;
        self.phones_report(dsp, &id, received)?;
        Ok(())
    }
}

/// The next change, after every one given before it.
fn next_change(db: &Db) -> Result<i64> {
    let row = db
        .one(
            "UPDATE phones_change SET last=last+1 WHERE id=1 RETURNING last",
            [],
        )?
        .ok_or_else(|| Error::new("phones_change_missing", 500))?;
    Ok(n(&row, "last"))
}

/// Texts from their rows, each with its files.
fn texts(db: &Db, rows: Vec<Value>) -> Result<Vec<Text>> {
    let mut files: BTreeMap<String, Vec<File>> = BTreeMap::new();
    for row in &rows {
        let id = s(row, "id");
        let kept = db.all(
            "SELECT * FROM phones_files WHERE text_id=? ORDER BY added_at,rowid",
            [id],
        )?;
        files.insert(id.to_owned(), kept.iter().map(file_of).collect());
    }
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
fn file_of(row: &Value) -> File {
    File {
        id: s(row, "id").into(),
        name: s(row, "name").into(),
        mime: s(row, "mime").into(),
        size: n(row, "size"),
        kept: row["sha256"].is_string(),
    }
}
fn direction(direction: Direction) -> &'static str {
    match direction {
        Direction::In => "in",
        Direction::Out => "out",
    }
}
pub fn status_text(status: Status) -> &'static str {
    match status {
        Status::Waiting => "waiting",
        Status::Sent => "sent",
        Status::Delivered => "delivered",
        Status::Failed => "failed",
    }
}
pub fn parse_status(text: &str) -> Option<Status> {
    match text {
        "waiting" => Some(Status::Waiting),
        "sent" => Some(Status::Sent),
        "delivered" => Some(Status::Delivered),
        "failed" => Some(Status::Failed),
        _ => None,
    }
}
#[cfg(test)]
#[path = "../tests/backend/storage.rs"]
mod tests;
