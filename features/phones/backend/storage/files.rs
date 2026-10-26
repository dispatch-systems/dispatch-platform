//! The texts' pictures and files, each by the SHA-256 of its bytes: one a phone reported, waiting
//! for its bytes, and one someone at the DSP added for a text not sent yet, theirs alone until it
//! goes. The bytes themselves are `backend::files`'.
use super::next_change;
use dispatch_core::{
    Error, Result,
    db::{Store, n, s},
    foundation::crypto,
};
use dispatch_phones_contract::File;
use rusqlite::params;
use serde_json::Value;

/// A picture or file as it's said to be, before its bytes are measured.
pub struct ReportedFile {
    pub name: String,
    pub mime: String,
    pub size: i64,
}

/// A picture or file as kept, with where its bytes are.
pub struct StoredFile {
    pub file: File,
    pub sha256: Option<String>,
    pub text_id: Option<String>,
    pub added_by: Option<String>,
}

/// What Phones keeps of the texts' pictures and files.
pub trait FileStore {
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
    /// Lets go of the picture `id` that `by` added for a text not sent yet: the SHA-256 of its
    /// bytes, if it was let go.
    fn phones_remove_file(&self, dsp: &str, id: &str, by: &str) -> Result<Option<String>>;
    /// Lets go of pictures added and never sent before `before`: the SHA-256 of each let go.
    fn phones_tidy_files(&self, dsp: &str, before: i64) -> Result<Vec<String>>;
    /// Whether any file is kept by `sha256`.
    fn phones_bytes_kept(&self, dsp: &str, sha256: &str) -> Result<bool>;
    /// A picture of a text Dispatch has for the phone `device` to send.
    fn phones_outgoing_file(&self, dsp: &str, device: &str, id: &str)
    -> Result<Option<StoredFile>>;
}

impl FileStore for Store {
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
    fn phones_remove_file(&self, dsp: &str, id: &str, by: &str) -> Result<Option<String>> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let Some(added) = db.one(
                "SELECT sha256 FROM phones_files WHERE id=? AND added_by=? AND text_id IS NULL",
                [id, by],
            )?
            else {
                return Ok(None);
            };
            db.exec("DELETE FROM phones_files WHERE id=?", [id])?;
            Ok(added["sha256"].as_str().map(str::to_owned))
        })
    }
    fn phones_tidy_files(&self, dsp: &str, before: i64) -> Result<Vec<String>> {
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
            Ok(loose)
        })
    }
    fn phones_bytes_kept(&self, dsp: &str, sha256: &str) -> Result<bool> {
        Ok(self
            .dsp(dsp)?
            .count("SELECT count(*) FROM phones_files WHERE sha256=?", [sha256])?
            > 0)
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
}

/// A file from its row.
pub(super) fn file_of(row: &Value) -> File {
    File {
        id: s(row, "id").into(),
        name: s(row, "name").into(),
        mime: s(row, "mime").into(),
        size: n(row, "size"),
        kept: row["sha256"].is_string(),
    }
}

#[cfg(test)]
#[path = "../../tests/backend/storage/files.rs"]
mod tests;
