//! The texts' pictures and files, each by the SHA-256 of its bytes: one a phone reported, waiting
//! for its bytes, and one someone at the DSP added for a text not sent yet, theirs alone until it
//! goes. The bytes themselves are `phone::files`'.
use super::next_change;
use crate::backend::phone::model::File;
use dispatch_core::{
    Error, Result,
    db::{Store, n, s},
    foundation::crypto,
};
use rusqlite::params;
use serde_json::Value;
use std::collections::HashMap;

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

/// What the phone side keeps of the texts' pictures and files.
pub trait FileStore {
    /// Keeps the bytes of a file a phone reported, by their SHA-256, and their size as they
    /// arrived, whatever the report said: refused as `phones_file_not_found` for one not of its
    /// own texts. Answers the text it's of.
    fn phones_file_arrived(
        &self,
        dsp: &str,
        device: &str,
        id: &str,
        sha256: &str,
        size: i64,
    ) -> Result<String>;
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
    /// The pictures and files of each of the texts `texts`, by the text, in the order added.
    fn phones_files_of(&self, dsp: &str, texts: &[String]) -> Result<HashMap<String, Vec<File>>>;
    /// Lets go of pictures added and never sent before `before`, whose bytes upkeep then lets go
    /// of once no file keeps them.
    fn phones_tidy_files(&self, dsp: &str, before: i64) -> Result<()>;
    /// Whether any file is kept by `sha256`.
    fn phones_bytes_kept(&self, dsp: &str, sha256: &str) -> Result<bool>;
    /// The SHA-256 of every bytes a file keeps.
    fn phones_kept_bytes(&self, dsp: &str) -> Result<Vec<String>>;
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
        size: i64,
    ) -> Result<String> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let file = db
                .one(
                    "SELECT f.text_id FROM phones_files f \
                     JOIN phones_texts t ON t.id=f.text_id \
                     WHERE f.id=? AND t.device_id=? AND t.origin='phone'",
                    params![id, device],
                )?
                .ok_or_else(|| Error::new("phones_file_not_found", 404))?;
            db.exec(
                "UPDATE phones_files SET sha256=?, size=? WHERE id=?",
                params![sha256, size, id],
            )?;
            let text = s(&file, "text_id").to_owned();
            let change = next_change(&db)?;
            db.exec(
                "UPDATE phones_texts SET change=? WHERE id=?",
                params![change, text],
            )?;
            Ok(text)
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
    fn phones_files_of(&self, dsp: &str, texts: &[String]) -> Result<HashMap<String, Vec<File>>> {
        let rows = self.dsp(dsp)?.all(
            "SELECT * FROM phones_files WHERE text_id IN (SELECT value FROM json_each(?)) \
             ORDER BY added_at,rowid",
            [serde_json::to_string(texts)?],
        )?;
        let mut files: HashMap<String, Vec<File>> = HashMap::new();
        for row in &rows {
            files
                .entry(s(row, "text_id").to_owned())
                .or_default()
                .push(file_of(row));
        }
        Ok(files)
    }
    fn phones_tidy_files(&self, dsp: &str, before: i64) -> Result<()> {
        self.dsp(dsp)?.exec(
            "DELETE FROM phones_files WHERE text_id IS NULL AND added_at<?",
            [before],
        )?;
        Ok(())
    }
    fn phones_bytes_kept(&self, dsp: &str, sha256: &str) -> Result<bool> {
        Ok(self
            .dsp(dsp)?
            .count("SELECT count(*) FROM phones_files WHERE sha256=?", [sha256])?
            > 0)
    }
    fn phones_kept_bytes(&self, dsp: &str) -> Result<Vec<String>> {
        Ok(self
            .dsp(dsp)?
            .all(
                "SELECT DISTINCT sha256 FROM phones_files WHERE sha256 IS NOT NULL",
                [],
            )?
            .iter()
            .map(|row| s(row, "sha256").to_owned())
            .collect())
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
fn file_of(row: &Value) -> File {
    File {
        id: s(row, "id").into(),
        name: s(row, "name").into(),
        mime: s(row, "mime").into(),
        size: n(row, "size"),
        kept: row["sha256"].is_string(),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/phone/storage/files.rs"]
mod tests;
