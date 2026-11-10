//! One of a DSP's backups as the platform owner saves it: a zip of its files as they were, each
//! at its place in the DSP's folder, a picture under `objects/`, with `backup.json`, the backup's
//! own description, but the platform's key. Put together aside, then sent as it's read.
use super::{FILES, MANIFESTS, Vault, blocking, history, schedule, seal, take};
use crate::{
    Error, Result, State,
    db::{self, s},
    foundation::crypto,
    server::http::input::Reply,
};
use axum::body::{Body, Bytes};
use futures_util::stream;
use serde_json::Value;
use std::{fs::File, io::Write, path::PathBuf, sync::Arc};
use tokio::io::AsyncReadExt;
use zip::{ZipWriter, write::SimpleFileOptions};

/// A backup put together as a zip, ready to send: open, and gone from its folder already.
pub struct Zipped {
    name: String,
    file: tokio::fs::File,
    length: u64,
}
impl Zipped {
    pub fn reply(self) -> Reply {
        let chunks = stream::unfold(self.file, |mut file| async move {
            let mut chunk = vec![0; 64 * 1024];
            match file.read(&mut chunk).await {
                Ok(0) => None,
                Ok(read) => {
                    chunk.truncate(read);
                    Some((Ok::<_, std::io::Error>(Bytes::from(chunk)), file))
                }
                Err(error) => Some((Err(error), file)),
            }
        });
        Reply::download(
            "application/zip",
            &self.name,
            Some(self.length),
            Body::from_stream(chunks),
        )
    }
}

pub async fn download(state: &Arc<State>, vault: &Vault, dsp: &str, id: &str) -> Result<Zipped> {
    let (dsp, id) = (dsp.to_owned(), id.to_owned());
    let (name, files) = {
        let (dsp, id) = (dsp.clone(), id.clone());
        state
            .run_bookkeeping(move |db| {
                let (taken, files) = history::files(&db.platform, &dsp, &id)?;
                let tz = schedule::timezone(db, &dsp)?;
                let row = db
                    .platform
                    .one("SELECT * FROM dsps WHERE id=?", [&dsp])?
                    .ok_or_else(|| Error::new("dsp_not_found", 404))?;
                let code = Some(s(&row, "code")).filter(|code| !code.is_empty());
                let when = chrono::DateTime::from_timestamp_millis(taken)
                    .ok_or_else(|| Error::new("backup_not_found", 404))?
                    .with_timezone(&tz)
                    .format("%Y-%m-%d-%H%M");
                Ok((format!("{}-backup-{when}.zip", code.unwrap_or(&dsp)), files))
            })
            .await?
    };
    let here = db::private_dir(&take::staging(state))?;
    let path = here.join(format!(
        "download-{}.zip",
        crypto::hex(&crypto::random::<8>()?)
    ));
    let made = zipped(vault, &dsp, &id, &files, &path).await;
    let file = made.and_then(|()| Ok(std::fs::File::open(&path)?));
    // Sent from the open file, gone from the folder already.
    let _ = std::fs::remove_file(&path);
    let file = tokio::fs::File::from_std(file?);
    let length = file.metadata().await?.len();
    Ok(Zipped { name, file, length })
}

/// Writes the zip of the backup `id`'s `files` at `path`, with its description.
async fn zipped(vault: &Vault, dsp: &str, id: &str, files: &[Value], path: &PathBuf) -> Result<()> {
    let mut zip = ZipWriter::new(File::create(path)?);
    let options = SimpleFileOptions::default().large_file(true);
    let sealed = path.with_extension("sealed");
    let mut manifest = {
        let object = vault.manifest(id);
        take::fetch(vault, dsp, MANIFESTS, &object, "backup_incomplete", &sealed).await?;
        let (key, binding, from) = (vault.key, Vault::described(dsp, id), sealed.clone());
        blocking(move || seal::open_json(&key, &binding, &from)).await?
    };
    // The description is this backup's: of its DSP, by its ID, naming the files it holds.
    let named: Vec<&str> = manifest["files"]
        .as_array()
        .map(|named| named.iter().map(|file| s(file, "object")).collect())
        .unwrap_or_default();
    let held: Vec<&str> = files.iter().map(|file| s(file, "object")).collect();
    crate::ensure(
        manifest["format"] == take::FORMAT
            && manifest["backup"] == id
            && manifest["dsp"]["id"] == dsp
            && named == held,
        "invalid_backup",
        502,
    )?;
    // The platform's key goes nowhere but a restore.
    if let Some(manifest) = manifest.as_object_mut() {
        manifest.remove("platformKey");
    }
    for file in files {
        let (object, sha256, at) = (s(file, "object"), s(file, "sha256"), s(file, "path"));
        take::fetch(vault, dsp, FILES, object, "backup_incomplete", &sealed).await?;
        let (key, binding, sha256, at, from) = (
            vault.key,
            dsp.to_owned(),
            sha256.to_owned(),
            at.to_owned(),
            sealed.clone(),
        );
        zip = blocking(move || {
            zip.start_file(at, options)
                .map_err(|_| Error::new("backup_failed", 500))?;
            seal::open(&key, &binding, &from, &mut zip, &sha256)?;
            Ok(zip)
        })
        .await?;
    }
    let _ = std::fs::remove_file(&sealed);
    blocking(move || {
        zip.start_file("backup.json", options)
            .map_err(|_| Error::new("backup_failed", 500))?;
        zip.write_all(&serde_json::to_vec_pretty(&manifest)?)?;
        zip.finish()
            .map_err(|_| Error::new("backup_failed", 500))?
            .sync_all()?;
        Ok(())
    })
    .await
}
