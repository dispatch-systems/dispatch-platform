//! The DSP's phones, each by the SHA-256 of the key it was given, and the code that pairs the next
//! one, by its own: pairing, checking in, renaming and removing, and the demo's stand-in phone.
use super::texts::{self, Reported, TextStore};
use crate::{
    api::types::PairingStatus,
    backend::phone::{model::Phone, numbers::normal},
};
use dispatch_core::{
    Error, Result,
    db::{Db, Store, migrations::add_column, n, s},
    ensure,
    foundation::crypto,
};
use rusqlite::params;
use serde_json::Value;

/// How lately a phone must have checked in to be online: twice the longest it waits on Dispatch,
/// and then some.
pub const ONLINE_MS: i64 = 2 * 60 * 1000;
/// How long a pairing code works.
pub const PAIRING_MS: i64 = 10 * 60 * 1000;

/// Communications' migration 8, as Phones' migration 2 was before it, which a database Phones
/// migrated has already: each pairing code by an ID of its own, which the card that shows it asks
/// after, and when a newer code replaced it before it paired (`replaced_at`), kept until it
/// expires so that card hears it no longer works. Codes made before have no ID, and expire within
/// ten minutes. Each step is taken only if it wasn't.
pub(crate) fn add_pairing_ids(db: &Db) -> Result<()> {
    add_column(db, "phones_pairings", "id", "TEXT")?;
    add_column(db, "phones_pairings", "replaced_at", "INTEGER")?;
    db.exec(
        "CREATE UNIQUE INDEX IF NOT EXISTS phones_pairings_id ON phones_pairings (id)",
        [],
    )?;
    Ok(())
}

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
    /// How the phone reads in the activity log: its name, and its number once it said it.
    pub fn logged(&self) -> String {
        if self.number.is_empty() {
            self.name.clone()
        } else {
            format!("{} ({})", self.name, self.number)
        }
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

/// What the phone side keeps of the DSP's phones and the code that pairs the next.
pub trait DeviceStore {
    fn phones_devices(&self, dsp: &str) -> Result<Vec<Device>>;
    fn phones_device(&self, dsp: &str, id: &str) -> Result<Option<Device>>;
    /// The phone holding the key `key_sha256`, not removed.
    fn phones_device_by_key(&self, dsp: &str, key_sha256: &str) -> Result<Option<Device>>;
    /// Notes that the phone checked in.
    fn phones_seen(&self, dsp: &str, device: &str, at: i64) -> Result<()>;
    fn phones_rename(&self, dsp: &str, id: &str, name: &str) -> Result<Device>;
    /// Removes the phone: its key works no more, and the texts still waiting on it fail, as no
    /// phone would send them.
    fn phones_remove(&self, dsp: &str, id: &str, at: i64) -> Result<Device>;
    /// Keeps the code that pairs the next phone, replacing any not used yet: its ID.
    fn phones_new_pairing(&self, dsp: &str, by: &str, code_sha256: &str, at: i64)
    -> Result<String>;
    /// How the code `id` stands at `at`.
    fn phones_pairing(&self, dsp: &str, id: &str, at: i64) -> Result<PairingStatus>;
    /// Pairs a phone by the code `code_sha256`, giving it the key `key_sha256`: refused as
    /// `phones_pairing_invalid` for a code used, replaced, expired or unknown.
    fn phones_pair(
        &self,
        dsp: &str,
        code_sha256: &str,
        key_sha256: &str,
        pairing: &Pairing,
        at: i64,
    ) -> Result<Device>;
    /// Lets go of the pairing codes expired, never used.
    fn phones_tidy_pairings(&self, dsp: &str, at: i64) -> Result<()>;
    /// Whether the key `key_sha256` was a phone's that was removed.
    fn phones_key_removed(&self, dsp: &str, key_sha256: &str) -> Result<bool>;
    /// Who made the code that paired the phone.
    fn phones_paired_by(&self, dsp: &str, device: &str) -> Result<Option<String>>;
    /// Whether the phone is a stand-in, the demo's, which no app holds.
    fn phones_stand_in(&self, dsp: &str, device: &str) -> Result<bool>;
    /// Pairs a stand-in phone, `number`, as the demo's, with texts it received.
    fn phones_demo(&self, dsp: &str, number: &str, received: Vec<Reported>, at: i64) -> Result<()>;
}

impl DeviceStore for Store {
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
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            db.exec(
                "UPDATE phones_devices SET removed_at=? WHERE id=?",
                params![at, id],
            )?;
            texts::orphaned(&db, id, at)
        })?;
        Ok(device)
    }
    fn phones_new_pairing(
        &self,
        dsp: &str,
        by: &str,
        code_sha256: &str,
        at: i64,
    ) -> Result<String> {
        let id = crypto::id("pairing")?;
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            db.exec(
                "UPDATE phones_pairings SET replaced_at=? \
                 WHERE used_at IS NULL AND replaced_at IS NULL",
                [at],
            )?;
            db.exec(
                "INSERT INTO phones_pairings (id,code_sha256,created_by,created_at,expires_at) \
                 VALUES (?,?,?,?,?)",
                params![id, code_sha256, by, at, at + PAIRING_MS],
            )?;
            Ok(())
        })?;
        Ok(id)
    }
    fn phones_pairing(&self, dsp: &str, id: &str, at: i64) -> Result<PairingStatus> {
        let found = self.dsp(dsp)?.one(
            "SELECT used_at,replaced_at,expires_at FROM phones_pairings WHERE id=?",
            [id],
        )?;
        Ok(match found {
            Some(code) if code["used_at"].is_i64() => PairingStatus::Paired,
            Some(code) if code["replaced_at"].is_i64() => PairingStatus::Replaced,
            Some(code) if n(&code, "expires_at") > at => PairingStatus::Waiting,
            _ => PairingStatus::Expired,
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
        let number = normal(pairing.number).unwrap_or_default();
        db.transaction(|| {
            let code = db
                .one(
                    "SELECT created_by FROM phones_pairings WHERE code_sha256=? \
                     AND used_at IS NULL AND replaced_at IS NULL AND expires_at>?",
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
    fn phones_tidy_pairings(&self, dsp: &str, at: i64) -> Result<()> {
        self.dsp(dsp)?.exec(
            "DELETE FROM phones_pairings WHERE used_at IS NULL AND expires_at<?",
            [at],
        )?;
        Ok(())
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

#[cfg(test)]
#[path = "../../../tests/backend/phone/storage/devices.rs"]
mod tests;
