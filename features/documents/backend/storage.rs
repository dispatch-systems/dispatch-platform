//! Documents's storage: the DSP's Google connection and the sign-ins started for it, in each
//! DSP's own database, and the account's refresh token, encrypted in the DSP's secrets. Only
//! this crate writes SQL against its tables.
use super::google::Account;
use dispatch_core::{
    Error, Result,
    db::{self, FromRow, Row, Store, iso, now},
    foundation::crypto,
};
use rusqlite::params;
use serde_json::json;
use std::collections::BTreeMap;

/// How long a sign-in started at Google may take to come back.
const SIGN_IN_MS: i64 = 10 * 60 * 1000;
const SECRET: &str = "google.enc";

/// The DSP's Google connection, as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Connection {
    pub broken: bool,
    pub account: Account,
    pub folder_id: String,
    pub folder_name: String,
    pub connected_by: String,
    pub connected_at: String,
}
impl FromRow for Connection {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            broken: row.get::<String>("status")? == "broken",
            account: Account {
                email: row.get("account_email")?,
                workspace: row.get::<String>("account_kind")? == "workspace",
            },
            folder_id: row.get("folder_id")?,
            folder_name: row.get("folder_name")?,
            connected_by: row.get("connected_by")?,
            connected_at: row.get("connected_at")?,
        })
    }
}

/// Who added a file through Dispatch, and who last changed it there, by their user IDs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub added_by: String,
    pub changed_by: String,
}

/// What Documents reads and writes for one DSP.
pub trait DocumentsStore {
    fn documents_connection(&self, dsp: &str) -> Result<Option<Connection>>;
    /// Records a sign-in someone started, by the hash of the state Google carries back.
    fn start_documents_sign_in(
        &self,
        dsp: &str,
        user: &str,
        state: &str,
        verifier: &str,
    ) -> Result<()>;
    /// The verifier of the sign-in `user` started with `state`, used up: each comes back once,
    /// to the person who started it, before it expires.
    fn finish_documents_sign_in(&self, dsp: &str, user: &str, state: &str) -> Result<String>;
    /// Keeps the account and its folder, and the refresh token encrypted beside the DSP's
    /// other secrets.
    fn save_documents_connection(
        &self,
        dsp: &str,
        connection: &Connection,
        refresh: &str,
    ) -> Result<()>;
    fn documents_refresh_token(&self, dsp: &str) -> Result<Option<String>>;
    /// Marks the connection broken, once: whether it was working until now.
    fn break_documents_connection(&self, dsp: &str) -> Result<bool>;
    /// Forgets the connection and its token. The folder stays in Google Drive.
    fn remove_documents_connection(&self, dsp: &str) -> Result<()>;
    /// What Dispatch recorded of each file it made or changed, by file.
    fn documents_records(&self, dsp: &str) -> Result<BTreeMap<String, Record>>;
    /// Records that `user` added the file, or changed it if it was added before.
    fn record_documents_change(&self, dsp: &str, file: &str, user: &str) -> Result<()>;
}
impl DocumentsStore for Store {
    fn documents_connection(&self, dsp: &str) -> Result<Option<Connection>> {
        let db = self.dsp(dsp)?;
        Ok(db
            .query_as("SELECT * FROM documents_connection WHERE id=1", [])?
            .into_iter()
            .next())
    }
    fn start_documents_sign_in(
        &self,
        dsp: &str,
        user: &str,
        state: &str,
        verifier: &str,
    ) -> Result<()> {
        let db = self.dsp(dsp)?;
        let at = now();
        db.transaction(|| {
            db.exec(
                "DELETE FROM documents_connect_requests WHERE expires_at<=?",
                [at],
            )?;
            db.exec(
                "INSERT INTO documents_connect_requests (state_hash,user_id,verifier,expires_at) \
                 VALUES (?,?,?,?)",
                params![crypto::sha(state), user, verifier, at + SIGN_IN_MS],
            )?;
            Ok(())
        })
    }
    fn finish_documents_sign_in(&self, dsp: &str, user: &str, state: &str) -> Result<String> {
        let db = self.dsp(dsp)?;
        let hash = crypto::sha(state);
        db.transaction(|| {
            let found: Option<(String, String, i64)> = db
                .query_as(
                    "SELECT user_id,verifier,expires_at FROM documents_connect_requests \
                     WHERE state_hash=?",
                    [&hash],
                )?
                .into_iter()
                .next();
            db.exec(
                "DELETE FROM documents_connect_requests WHERE state_hash=?",
                [&hash],
            )?;
            match found {
                Some((starter, verifier, expires)) if starter == user && expires > now() => {
                    Ok(verifier)
                }
                _ => Err(Error::new("documents_connect_expired", 409)),
            }
        })
    }
    fn save_documents_connection(
        &self,
        dsp: &str,
        connection: &Connection,
        refresh: &str,
    ) -> Result<()> {
        let area = self.area(dsp, "secrets")?;
        let key = db::key_file(&area.join("vault.key"))?;
        db::write_private(
            &area.join(SECRET),
            crypto::encrypt(&key, &binding(dsp), &json!({"refreshToken": refresh}))?.as_bytes(),
        )?;
        let db = self.dsp(dsp)?;
        db.exec(
            "INSERT OR REPLACE INTO documents_connection (id,status,account_email,account_kind,\
             folder_id,folder_name,connected_by,connected_at,broken_at) \
             VALUES (1,'connected',?,?,?,?,?,?,NULL)",
            params![
                connection.account.email,
                if connection.account.workspace {
                    "workspace"
                } else {
                    "personal"
                },
                connection.folder_id,
                connection.folder_name,
                connection.connected_by,
                connection.connected_at,
            ],
        )?;
        Ok(())
    }
    fn documents_refresh_token(&self, dsp: &str) -> Result<Option<String>> {
        let area = self.area(dsp, "secrets")?;
        let path = area.join(SECRET);
        if !path.exists() {
            return Ok(None);
        }
        db::private_file(&path, false)?;
        let key = db::key_file(&area.join("vault.key"))?;
        let value = crypto::decrypt(&key, &binding(dsp), &std::fs::read_to_string(path)?)?;
        Ok(value["refreshToken"].as_str().map(str::to_owned))
    }
    fn break_documents_connection(&self, dsp: &str) -> Result<bool> {
        let db = self.dsp(dsp)?;
        let changed = db.exec(
            "UPDATE documents_connection SET status='broken',broken_at=? \
             WHERE id=1 AND status='connected'",
            [iso()],
        )?;
        Ok(changed > 0)
    }
    fn remove_documents_connection(&self, dsp: &str) -> Result<()> {
        let path = self.area(dsp, "secrets")?.join(SECRET);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        let db = self.dsp(dsp)?;
        db.exec("DELETE FROM documents_connection", [])?;
        Ok(())
    }
    fn documents_records(&self, dsp: &str) -> Result<BTreeMap<String, Record>> {
        let db = self.dsp(dsp)?;
        let rows: Vec<(String, String, String)> = db.query_as(
            "SELECT file_id,added_by,changed_by FROM documents_files",
            [],
        )?;
        Ok(rows
            .into_iter()
            .map(|(file, added_by, changed_by)| {
                (
                    file,
                    Record {
                        added_by,
                        changed_by,
                    },
                )
            })
            .collect())
    }
    fn record_documents_change(&self, dsp: &str, file: &str, user: &str) -> Result<()> {
        let db = self.dsp(dsp)?;
        let at = iso();
        db.exec(
            "INSERT INTO documents_files (file_id,added_by,added_at,changed_by,changed_at) \
             VALUES (?1,?2,?3,?2,?3) ON CONFLICT(file_id) DO UPDATE SET \
             changed_by=excluded.changed_by,changed_at=excluded.changed_at",
            params![file, user, at],
        )?;
        Ok(())
    }
}

/// What the token's encryption is bound to: this DSP's Google token, in its first format.
fn binding(dsp: &str) -> String {
    format!("{dsp}:google:1")
}

#[cfg(test)]
#[path = "../tests/backend/storage.rs"]
mod tests;
