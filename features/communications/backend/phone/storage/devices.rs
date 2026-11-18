//! The DSP's phones, each by the SHA-256 of the key it was given, and the code that pairs the next
//! one, by its own: pairing, checking in, renaming and removing, and the demo's stand-in phone.
use super::texts::{self, Reported, TextStore};
use crate::{
    api::types::PairingStatus,
    backend::phone::{model::Health, numbers::normal},
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
/// How often a phone checking in is noted: at most once in this long, so its last check-in kept
/// lags by no more than this and a wait, well within `ONLINE_MS`.
pub const SEEN_MS: i64 = 30 * 1000;
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
/// Communications' migration 11: the `versionCode` of the app each phone runs, as it says with
/// each wait (`app_version_code`), beside its version; none until it says it, as an app older than
/// that never does.
pub(crate) fn add_app_version_codes(db: &Db) -> Result<()> {
    add_column(db, "phones_devices", "app_version_code", "INTEGER")
}
/// Communications' migration 14: how each phone last said it stands (`health`), as JSON, with
/// each wait; none until it says it, as an app older than that never does.
pub(crate) fn add_health(db: &Db) -> Result<()> {
    add_column(db, "phones_devices", "health", "TEXT")
}
/// Communications' migration 23: the line each phone serves (`line`), and the line each code
/// pairs a phone to, with the name its phones are given (`line_name`): none for Dispatch Phone's
/// own, as every phone and code before was.
pub(crate) fn add_lines(db: &Db) -> Result<()> {
    add_column(db, "phones_devices", "line", "TEXT")?;
    add_column(db, "phones_pairings", "line", "TEXT")?;
    add_column(db, "phones_pairings", "line_name", "TEXT")
}

/// A line of the DSP's phones other than Dispatch Phone's own: another feature's, by its name
/// (`id`), whose phones are named for it as they pair (`name`, then `name 2` and so on). Dispatch
/// Phone's own line is none, its phones named Dispatch, then Phone 2 and so on. Each phone serves
/// the line of the code that paired it, always: only that line's texts are filed, counted and
/// answered from it, and only that line sends from it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Line<'a> {
    pub id: &'a str,
    pub name: &'a str,
    /// Whether its texts are filed, in conversations of its own its page reads and answers, and
    /// it may text a number first. Off, its phones keep their texts as the phone side's alone, as
    /// before Dispatch Phone filed another line's, and its page answers as a line with nothing
    /// yet: off until the release after the one that kept each line's texting apart, as a
    /// rollback to a release without lines reads every conversation as Dispatch Phone's. Each
    /// phone keeps it as its line said as it paired (`phones_devices.texting`).
    pub texting: bool,
}

/// One of the DSP's phones, as kept.
#[derive(Clone, Debug, PartialEq)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub number: String,
    pub model: String,
    pub app_version: String,
    /// The `versionCode` of the app it runs, once it said it.
    pub app_version_code: Option<i64>,
    pub paired_at: i64,
    pub last_seen_at: Option<i64>,
    /// How it last said it stands, once it said it.
    pub health: Option<Health>,
    /// Whether it's a stand-in, the demo's, which no app holds.
    pub stand_in: bool,
    /// The line it serves, by its feature's name: none for Dispatch Phone's own.
    pub line: Option<String>,
}
impl Device {
    fn of(row: &Value) -> Self {
        Self {
            id: s(row, "id").into(),
            name: s(row, "name").into(),
            number: s(row, "number").into(),
            model: s(row, "model").into(),
            app_version: s(row, "app_version").into(),
            app_version_code: row["app_version_code"].as_i64(),
            paired_at: n(row, "paired_at"),
            last_seen_at: row["last_seen_at"].as_i64(),
            // As this server wrote it; one it can't read is as none said.
            health: row["health"]
                .as_str()
                .and_then(|health| serde_json::from_str(health).ok()),
            stand_in: n(row, "stand_in") == 1,
            line: row["line"].as_str().map(str::to_owned),
        }
    }
    /// Whether it runs `app` already.
    pub fn runs(&self, app: &AppVersion) -> bool {
        self.app_version == app.version && self.app_version_code == Some(app.version_code)
    }
    /// Whether the app it runs is older than the one of `version_code`.
    pub fn outdated(&self, version_code: u32) -> bool {
        self.app_version_code
            .is_some_and(|code| code < i64::from(version_code))
    }
    pub fn online(&self, now: i64) -> bool {
        self.last_seen_at.is_some_and(|at| now - at <= ONLINE_MS)
    }
    /// Whether its checking in at `now` is noted (`phones_seen`): once `SEEN_MS` passed since the
    /// last that was.
    pub fn noting(&self, now: i64) -> bool {
        self.last_seen_at.is_none_or(|at| now - at >= SEEN_MS)
    }
    /// What the activity log calls `what` done to the phone: `phones.<what>` for one of Dispatch
    /// Phone's own line, `<line>.phones.<what>` for another feature's, which words it.
    pub fn action(&self, what: &str) -> String {
        match &self.line {
            None => format!("phones.{what}"),
            Some(line) => format!("{line}.phones.{what}"),
        }
    }
    /// How the phone reads in the activity log: its name, and its number once it said it.
    pub fn logged(&self) -> String {
        if self.number.is_empty() {
            self.name.clone()
        } else {
            format!("{} ({})", self.name, self.number)
        }
    }
}

/// What a phone tells Dispatch of itself as it pairs.
pub struct Pairing<'a> {
    pub number: &'a str,
    pub model: &'a str,
    pub app_version: &'a str,
}

/// The app a phone runs, as it says with each wait: its version, and the `versionCode` Android
/// compares.
pub struct AppVersion {
    pub version: String,
    pub version_code: i64,
}

/// What the phone side keeps of the DSP's phones and the code that pairs the next.
pub trait DeviceStore {
    /// The phones of the line `line`, by its feature's name, or of Dispatch Phone's own (none).
    fn phones_devices(&self, dsp: &str, line: Option<&str>) -> Result<Vec<Device>>;
    /// The phone `id`, of any line.
    fn phones_device(&self, dsp: &str, id: &str) -> Result<Option<Device>>;
    /// The phone holding the key `key_sha256`, not removed.
    fn phones_device_by_key(&self, dsp: &str, key_sha256: &str) -> Result<Option<Device>>;
    /// Notes that the phone checked in.
    fn phones_seen(&self, dsp: &str, device: &str, at: i64) -> Result<()>;
    /// Notes the app the phone runs.
    fn phones_app_version(&self, dsp: &str, device: &str, app: &AppVersion) -> Result<()>;
    /// Notes how the phone says it stands.
    fn phones_health(&self, dsp: &str, device: &str, health: &Health) -> Result<()>;
    /// Renames the phone `id` of the line `line`: refused as `phones_not_found` for another's.
    fn phones_rename(&self, dsp: &str, line: Option<&str>, id: &str, name: &str) -> Result<Device>;
    /// Removes the phone `id` of the line `line`: its key works no more, and the texts still
    /// waiting on it fail, as no phone would send them. The phone as it was, and the texts that
    /// failed. Refused as `phones_not_found` for another line's.
    fn phones_remove(
        &self,
        dsp: &str,
        line: Option<&str>,
        id: &str,
        at: i64,
    ) -> Result<(Device, Vec<String>)>;
    /// Keeps the code that pairs the next phone to `line`, or to Dispatch Phone's own (none),
    /// replacing any of that line's not used yet: its ID.
    fn phones_new_pairing(
        &self,
        dsp: &str,
        line: Option<Line>,
        by: &str,
        code_sha256: &str,
        at: i64,
    ) -> Result<String>;
    /// How the code `id` of the line `line` stands at `at`: one of another line's as expired.
    fn phones_pairing(
        &self,
        dsp: &str,
        line: Option<&str>,
        id: &str,
        at: i64,
    ) -> Result<PairingStatus>;
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
    /// Whether the code `code_sha256` pairs a phone at `at`: not used, replaced or expired.
    fn phones_pairing_open(&self, dsp: &str, code_sha256: &str, at: i64) -> Result<bool>;
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

/// The phone whose `column` is `value`, not removed.
fn device(store: &Store, dsp: &str, column: &str, value: &str) -> Result<Option<Device>> {
    let db = store.dsp(dsp)?;
    Ok(db
        .one(
            &format!("SELECT * FROM phones_devices WHERE {column}=? AND removed_at IS NULL"),
            [value],
        )?
        .as_ref()
        .map(Device::of))
}

impl DeviceStore for Store {
    fn phones_devices(&self, dsp: &str, line: Option<&str>) -> Result<Vec<Device>> {
        let db = self.dsp(dsp)?;
        let rows = db.all(
            "SELECT * FROM phones_devices WHERE removed_at IS NULL AND line IS ? \
             ORDER BY paired_at,id",
            [line],
        )?;
        Ok(rows.iter().map(Device::of).collect())
    }
    fn phones_device(&self, dsp: &str, id: &str) -> Result<Option<Device>> {
        device(self, dsp, "id", id)
    }
    fn phones_device_by_key(&self, dsp: &str, key_sha256: &str) -> Result<Option<Device>> {
        device(self, dsp, "key_sha256", key_sha256)
    }
    fn phones_seen(&self, dsp: &str, device: &str, at: i64) -> Result<()> {
        self.dsp(dsp)?.exec(
            "UPDATE phones_devices SET last_seen_at=?1 WHERE id=?2 AND coalesce(last_seen_at,0)<?1",
            params![at, device],
        )?;
        Ok(())
    }
    fn phones_app_version(&self, dsp: &str, device: &str, app: &AppVersion) -> Result<()> {
        self.dsp(dsp)?.exec(
            "UPDATE phones_devices SET app_version=?, app_version_code=? WHERE id=?",
            params![app.version, app.version_code, device],
        )?;
        Ok(())
    }
    fn phones_health(&self, dsp: &str, device: &str, health: &Health) -> Result<()> {
        self.dsp(dsp)?.exec(
            "UPDATE phones_devices SET health=? WHERE id=?",
            params![serde_json::to_string(health)?, device],
        )?;
        Ok(())
    }
    fn phones_rename(&self, dsp: &str, line: Option<&str>, id: &str, name: &str) -> Result<Device> {
        let db = self.dsp(dsp)?;
        let changed = db.exec(
            "UPDATE phones_devices SET name=? WHERE id=? AND removed_at IS NULL AND line IS ?",
            params![name, id, line],
        )?;
        ensure(changed == 1, "phones_not_found", 404)?;
        drop(db);
        self.phones_device(dsp, id)?
            .ok_or_else(|| Error::new("phones_not_found", 404))
    }
    fn phones_remove(
        &self,
        dsp: &str,
        line: Option<&str>,
        id: &str,
        at: i64,
    ) -> Result<(Device, Vec<String>)> {
        let device = self
            .phones_device(dsp, id)?
            .filter(|device| device.line.as_deref() == line)
            .ok_or_else(|| Error::new("phones_not_found", 404))?;
        let db = self.dsp(dsp)?;
        let failed = db.transaction(|| {
            db.exec(
                "UPDATE phones_devices SET removed_at=? WHERE id=?",
                params![at, id],
            )?;
            texts::orphaned(&db, id, at)
        })?;
        Ok((device, failed))
    }
    fn phones_new_pairing(
        &self,
        dsp: &str,
        line: Option<Line>,
        by: &str,
        code_sha256: &str,
        at: i64,
    ) -> Result<String> {
        let id = crypto::id("pairing")?;
        let (line, line_name, texting) = (
            line.map(|line| line.id),
            line.map(|line| line.name),
            line.is_some_and(|line| line.texting),
        );
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            db.exec(
                "UPDATE phones_pairings SET replaced_at=? \
                 WHERE used_at IS NULL AND replaced_at IS NULL AND line IS ?",
                params![at, line],
            )?;
            db.exec(
                "INSERT INTO phones_pairings \
                 (id,code_sha256,created_by,created_at,expires_at,line,line_name,line_texting) \
                 VALUES (?,?,?,?,?,?,?,?)",
                params![
                    id,
                    code_sha256,
                    by,
                    at,
                    at + PAIRING_MS,
                    line,
                    line_name,
                    texting
                ],
            )?;
            Ok(())
        })?;
        Ok(id)
    }
    fn phones_pairing(
        &self,
        dsp: &str,
        line: Option<&str>,
        id: &str,
        at: i64,
    ) -> Result<PairingStatus> {
        let found = self.dsp(dsp)?.one(
            "SELECT used_at,replaced_at,expires_at FROM phones_pairings WHERE id=? AND line IS ?",
            params![id, line],
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
                    "SELECT created_by,line,line_name,line_texting FROM phones_pairings \
                     WHERE code_sha256=? \
                     AND used_at IS NULL AND replaced_at IS NULL AND expires_at>?",
                    params![code_sha256, at],
                )?
                .ok_or_else(|| Error::new("phones_pairing_invalid", 401))?;
            let line = code["line"].as_str();
            let count = db.count(
                "SELECT count(*) FROM phones_devices WHERE removed_at IS NULL AND line IS ?",
                [line],
            )?;
            let name = match (code["line_name"].as_str(), count) {
                (None, 0) => "Dispatch".to_owned(),
                (None, _) => format!("Phone {}", count + 1),
                (Some(name), 0) => name.to_owned(),
                (Some(name), _) => format!("{name} {}", count + 1),
            };
            db.exec(
                "INSERT INTO phones_devices \
                 (id,name,number,model,app_version,key_sha256,paired_at,paired_by,last_seen_at,\
                 line,texting) VALUES (?,?,?,?,?,?,?,?,?,?,?)",
                params![
                    id,
                    name,
                    number,
                    pairing.model,
                    pairing.app_version,
                    key_sha256,
                    at,
                    s(&code, "created_by"),
                    at,
                    line,
                    n(&code, "line_texting") == 1
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
    fn phones_pairing_open(&self, dsp: &str, code_sha256: &str, at: i64) -> Result<bool> {
        Ok(self.dsp(dsp)?.count(
            "SELECT count(*) FROM phones_pairings WHERE code_sha256=? \
             AND used_at IS NULL AND replaced_at IS NULL AND expires_at>?",
            params![code_sha256, at],
        )? > 0)
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
