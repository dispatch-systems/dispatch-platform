//! One of a DSP's backups as the platform owner saves it: a zip of its files as they were, each
//! at its place in the DSP's folder, a picture under `objects/`, with `backup.json` naming each
//! and what it holds. Put together aside, then sent as it's read.
use super::{FILES, Vault, history, schedule, seal, take};
use crate::{
    Error, Result, State,
    db::{self, s},
    foundation::crypto,
    server::http::input::Reply,
};
use axum::body::{Body, Bytes};
use futures_util::stream;
use serde_json::{Value, json};
use std::{fs::File, io::Write, path::PathBuf, sync::Arc};
use tokio::io::AsyncReadExt;
use zip::{ZipWriter, write::SimpleFileOptions};

pub async fn download(state: &Arc<State>, vault: &Vault, dsp: &str, id: &str) -> Result<Reply> {
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
    let made = zipped(vault, &dsp, &files, &path).await;
    let file = made.and_then(|()| Ok(std::fs::File::open(&path)?));
    // Sent from the open file, gone from the folder already.
    let _ = std::fs::remove_file(&path);
    let file = tokio::fs::File::from_std(file?);
    let length = file.metadata().await?.len();
    let chunks = stream::unfold(file, |mut file| async move {
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
    Ok(Reply::download(
        "application/zip",
        &name,
        Some(length),
        Body::from_stream(chunks),
    ))
}

/// Writes the zip of `files` at `path`.
async fn zipped(vault: &Vault, dsp: &str, files: &[Value], path: &PathBuf) -> Result<()> {
    let mut zip = ZipWriter::new(File::create(path)?);
    let options = SimpleFileOptions::default().large_file(true);
    let sealed = path.with_extension("sealed");
    let mut listed = Vec::new();
    for file in files {
        let (object, sha256, at) = (s(file, "object"), s(file, "sha256"), s(file, "path"));
        let fetched = vault
            .objects
            .get(dsp, FILES, object)
            .await?
            .ok_or_else(|| Error::new("backup_incomplete", 502))?;
        take::written(fetched, &sealed).await?;
        let (key, binding, sha256, at, from) = (
            vault.key,
            dsp.to_owned(),
            sha256.to_owned(),
            at.to_owned(),
            sealed.clone(),
        );
        zip = tokio::task::spawn_blocking(move || -> Result<ZipWriter<File>> {
            zip.start_file(at, options)
                .map_err(|_| Error::new("backup_failed", 500))?;
            seal::open(&key, &binding, &from, &mut zip, &sha256)?;
            Ok(zip)
        })
        .await
        .map_err(|_| Error::new("backup_failed", 500))??;
        listed.push(json!({
            "path": file["path"],
            "item": file["item"],
            "sha256": file["sha256"],
            "size": file["size"],
        }));
    }
    let _ = std::fs::remove_file(&sealed);
    tokio::task::spawn_blocking(move || -> Result<()> {
        zip.start_file("backup.json", options)
            .map_err(|_| Error::new("backup_failed", 500))?;
        zip.write_all(&serde_json::to_vec_pretty(&json!({ "files": listed }))?)?;
        zip.finish()
            .map_err(|_| Error::new("backup_failed", 500))?
            .sync_all()?;
        Ok(())
    })
    .await
    .map_err(|_| Error::new("backup_failed", 500))?
}
