//! Documents's storage: the DSP's Google connection and the sign-ins started for it, in each
//! DSP's own database, and the account's refresh token, encrypted in the DSP's secrets. Only
//! this crate writes SQL against its tables.
use super::google::Account;
use dispatch_core::{
    Error, Result,
    db::{self, Db, FromRow, Row, Store, iso, migrations::add_column, now},
    foundation::crypto,
};
use dispatch_documents_contract::Author;
use rusqlite::params;
use serde_json::json;
use std::collections::BTreeMap;

/// How long a sign-in started at Google may take to come back.
const SIGN_IN_MS: i64 = 10 * 60 * 1000;
const SECRET: &str = "google.enc";

/// Documents' migration 2: whether the connection reaches everything in the account's Drive,
/// as Dispatch asks of every connection since. One made before reaches only the files Dispatch
/// made or was given, until someone reconnects it.
pub(crate) fn add_full_access(db: &Db) -> Result<()> {
    add_column(
        db,
        "documents_connection",
        "full_access",
        "INTEGER NOT NULL DEFAULT 0",
    )
}

/// Documents' migration 3: the name of who added a file, or last changed it, when it came from
/// someone who isn't a member, as a Slack member no member matches, whose user ID is then blank.
pub(crate) fn add_outside_names(db: &Db) -> Result<()> {
    add_column(db, "documents_files", "added_name", "TEXT")?;
    add_column(db, "documents_files", "changed_name", "TEXT")
}

/// The DSP's Google connection, as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Connection {
    pub broken: bool,
    /// Whether it reaches everything in the account's Drive, rather than only the files
    /// Dispatch made or was given, as one connected before Dispatch asked for that does.
    pub full: bool,
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
            full: row.get::<i64>("full_access")? == 1,
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

/// Who added a file through Dispatch, and who last changed it there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub added_by: Author,
    pub changed_by: Author,
}
impl FromRow for Record {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        let author = |id: &str, name: &str| -> Result<Author> {
            Ok(match row.get::<Option<String>>(name)? {
                Some(name) => Author::Named(name),
                None => Author::Member(row.get(id)?),
            })
        };
        Ok(Self {
            added_by: author("added_by", "added_name")?,
            changed_by: author("changed_by", "changed_name")?,
        })
    }
}
/// A file's record, with the file's ID.
struct Recorded(String, Record);
impl FromRow for Recorded {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self(row.get("file_id")?, Record::from_row(row)?))
    }
}
/// How a record keeps who: a member's user ID, or else a blank one and their name.
fn columns(by: &Author) -> (&str, Option<&str>) {
    match by {
        Author::Member(user) => (user, None),
        Author::Named(name) => ("", Some(name)),
    }
}

/// How sharing the main folder with a member went.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sharing {
    Shared,
    /// Their address is no Google account; they link one to edit.
    NeedsAccount,
    /// Google refused for another reason, by its code.
    Refused(String),
}
/// A member Documents shares the main folder with, as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Person {
    pub user: String,
    /// The Google account they linked, if they did.
    pub linked: Option<String>,
    /// The address the folder is shared with now, and Drive's ID for that share.
    pub shared: Option<(String, String)>,
    pub sharing: Sharing,
    /// When Dispatch last emailed them how to link a Google account.
    pub emailed_at: Option<String>,
}
impl FromRow for Person {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        let shared_email: Option<String> = row.get("shared_email")?;
        let share_id: Option<String> = row.get("share_id")?;
        Ok(Self {
            user: row.get("user_id")?,
            linked: row.get("linked_email")?,
            shared: shared_email.zip(share_id),
            sharing: match row.get::<String>("state")?.as_str() {
                "shared" => Sharing::Shared,
                "needs_account" => Sharing::NeedsAccount,
                _ => Sharing::Refused(row.get::<Option<String>>("refusal")?.unwrap_or_default()),
            },
            emailed_at: row.get("emailed_at")?,
        })
    }
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
    /// Records that the member `user` added the file, or changed it if it was added before.
    fn record_documents_change(&self, dsp: &str, file: &str, user: &str) -> Result<()> {
        self.record_documents_change_by(dsp, file, &Author::Member(user.to_owned()))
    }
    /// Records that `by` added the file, or changed it if it was added before.
    fn record_documents_change_by(&self, dsp: &str, file: &str, by: &Author) -> Result<()>;
    /// Every member Documents shares with, or tried to.
    fn documents_people(&self, dsp: &str) -> Result<Vec<Person>>;
    /// Keeps how sharing with a member went.
    fn save_documents_person(&self, dsp: &str, person: &Person) -> Result<()>;
    /// Forgets a member who left, or no longer uses Documents.
    fn remove_documents_person(&self, dsp: &str, user: &str) -> Result<()>;
    /// Keeps the Google account a member linked, to share the folder with it next.
    fn link_documents_google(&self, dsp: &str, user: &str, email: &str) -> Result<()>;
    /// Every share Dispatch made of a file someone else owns.
    fn documents_shared_files(&self, dsp: &str) -> Result<Vec<SharedFile>>;
    /// Keeps a share Dispatch made of such a file.
    fn save_documents_shared_file(&self, dsp: &str, shared: &SharedFile) -> Result<()>;
    /// Forgets a share Dispatch took back.
    fn remove_documents_shared_file(&self, dsp: &str, shared: &SharedFile) -> Result<()>;
}
/// A share Dispatch made of a file someone else owns, with a teammate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SharedFile {
    pub file: String,
    pub email: String,
    /// Drive's ID for the share.
    pub share: String,
}
impl FromRow for SharedFile {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            file: row.get("file_id")?,
            email: row.get("email")?,
            share: row.get("share_id")?,
        })
    }
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
             folder_id,folder_name,connected_by,connected_at,broken_at,full_access) \
             VALUES (1,'connected',?,?,?,?,?,?,NULL,?)",
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
                connection.full,
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
        // Whom it shared with, which a new connection finds again.
        db.exec("DELETE FROM documents_people", [])?;
        Ok(())
    }
    fn documents_records(&self, dsp: &str) -> Result<BTreeMap<String, Record>> {
        let rows: Vec<Recorded> = self.dsp(dsp)?.query_as(
            "SELECT file_id,added_by,added_name,changed_by,changed_name FROM documents_files",
            [],
        )?;
        Ok(rows
            .into_iter()
            .map(|Recorded(file, record)| (file, record))
            .collect())
    }
    fn documents_people(&self, dsp: &str) -> Result<Vec<Person>> {
        self.dsp(dsp)?
            .query_as("SELECT * FROM documents_people ORDER BY user_id", [])
    }
    fn save_documents_person(&self, dsp: &str, person: &Person) -> Result<()> {
        let (state, refusal) = match &person.sharing {
            Sharing::Shared => ("shared", None),
            Sharing::NeedsAccount => ("needs_account", None),
            Sharing::Refused(code) => ("refused", Some(code.as_str())),
        };
        self.dsp(dsp)?.exec(
            "INSERT OR REPLACE INTO documents_people (user_id,linked_email,shared_email,share_id,\
             state,refusal,emailed_at,updated_at) VALUES (?,?,?,?,?,?,?,?)",
            params![
                person.user,
                person.linked,
                person.shared.as_ref().map(|(email, _)| email),
                person.shared.as_ref().map(|(_, id)| id),
                state,
                refusal,
                person.emailed_at,
                iso(),
            ],
        )?;
        Ok(())
    }
    fn remove_documents_person(&self, dsp: &str, user: &str) -> Result<()> {
        self.dsp(dsp)?
            .exec("DELETE FROM documents_people WHERE user_id=?", [user])?;
        Ok(())
    }
    fn link_documents_google(&self, dsp: &str, user: &str, email: &str) -> Result<()> {
        self.dsp(dsp)?.exec(
            "INSERT INTO documents_people (user_id,linked_email,state,updated_at) \
             VALUES (?1,?2,'needs_account',?3) ON CONFLICT(user_id) DO UPDATE SET \
             linked_email=excluded.linked_email,updated_at=excluded.updated_at",
            params![user, email, iso()],
        )?;
        Ok(())
    }
    fn documents_shared_files(&self, dsp: &str) -> Result<Vec<SharedFile>> {
        self.dsp(dsp)?.query_as(
            "SELECT file_id,email,share_id FROM documents_shared_files ORDER BY file_id,email",
            [],
        )
    }
    fn save_documents_shared_file(&self, dsp: &str, shared: &SharedFile) -> Result<()> {
        self.dsp(dsp)?.exec(
            "INSERT OR REPLACE INTO documents_shared_files (file_id,email,share_id,shared_at) \
             VALUES (?,?,?,?)",
            params![shared.file, shared.email, shared.share, iso()],
        )?;
        Ok(())
    }
    fn remove_documents_shared_file(&self, dsp: &str, shared: &SharedFile) -> Result<()> {
        self.dsp(dsp)?.exec(
            "DELETE FROM documents_shared_files WHERE file_id=? AND email=?",
            params![shared.file, shared.email],
        )?;
        Ok(())
    }
    fn record_documents_change_by(&self, dsp: &str, file: &str, by: &Author) -> Result<()> {
        let db = self.dsp(dsp)?;
        let (user, name) = columns(by);
        db.exec(
            "INSERT INTO documents_files \
             (file_id,added_by,added_name,added_at,changed_by,changed_name,changed_at) \
             VALUES (?1,?2,?3,?4,?2,?3,?4) ON CONFLICT(file_id) DO UPDATE SET \
             changed_by=excluded.changed_by,changed_name=excluded.changed_name,\
             changed_at=excluded.changed_at",
            params![file, user, name, iso()],
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
