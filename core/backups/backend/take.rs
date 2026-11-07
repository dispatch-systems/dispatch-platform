//! Taking one backup of a DSP: each file it holds copied aside, a database as SQLite copies one
//! while it's in use, then sealed; sent unless a file holding the same is kept already, and read
//! back once sent to check it opens as it was; then the pictures the DSP keeps in the object
//! store, the same way; then the backup's description, which names each file, what it belongs
//! to and what it's kept as, with the DSP as the platform knows it and the release that took it,
//! which is all a restore needs beside the bucket and the key.
use super::{FILES, MANIFESTS, Vault, contents, seal};
use crate::{
    Error, Result, State,
    db::{self, at, now, objects::Fetched},
};
use futures_util::StreamExt;
use rusqlite::OpenFlags;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

/// The description a backup's files are kept under, as it's written.
const FORMAT: i64 = 1;

/// A backup to take: of which DSP, and what it leaves out.
pub struct Asked {
    pub id: String,
    pub dsp: String,
    pub skipped: Vec<String>,
    /// What the DSP's backups' files are named for.
    pub generation: String,
}

/// What a backup took: each file, and how many bytes they hold together.
pub struct Taken {
    pub files: Vec<Value>,
    pub size: i64,
}

/// Where a backup's files are copied and sealed on their way, emptied when it ends.
pub fn staging(state: &State) -> PathBuf {
    state.config.environment_root().join("backups")
}

/// Takes the backup `asked` for, naming in `sent` each file it sends as it's sent, so a backup
/// that fails part way lets go of them when it expires.
pub async fn take(
    state: &Arc<State>,
    vault: &Vault,
    asked: &Asked,
    sent: &mut Vec<String>,
) -> Result<Taken> {
    let dsp = asked.dsp.clone();
    let (folder, row, features, sizes) = state
        .run_bookkeeping(move |db| {
            let row = db
                .platform
                .one("SELECT * FROM dsps WHERE id=?", [&dsp])?
                .ok_or_else(|| Error::new("dsp_not_found", 404))?;
            let features = db.platform.all(
                "SELECT * FROM dsp_features WHERE dsp_id=? ORDER BY feature",
                [&dsp],
            )?;
            Ok((db.dsp_folder(&dsp)?, row, features, super::history::sizes(&db.platform, &dsp)?))
        })
        .await?;
    let held: Vec<String> = contents::items(&folder)
        .into_iter()
        .map(|item| item.id)
        .filter(|id| !asked.skipped.contains(id))
        .collect();
    let here = db::private_dir(&staging(state).join(&asked.id))?;
    let taken = async {
        let files = send(state, vault, asked, &folder, &here, &held, &sizes, sent).await?;
        let manifest = json!({
            "format": FORMAT,
            "backup": asked.id,
            "dsp": row,
            "features": features,
            "release": state.config.release,
            "generation": asked.generation,
            "takenAt": at(now()),
            "holds": held,
            "files": files,
        });
        describe(vault, &asked.dsp, &asked.id, &manifest, &here).await?;
        Ok::<_, Error>(files)
    }
    .await;
    let _ = fs::remove_dir_all(&here);
    let files: Vec<Value> = taken?;
    let size = files.iter().map(|file| file["size"].as_i64().unwrap_or(0)).sum();
    Ok(Taken { files, size })
}

async fn send(
    state: &Arc<State>,
    vault: &Vault,
    asked: &Asked,
    folder: &Path,
    here: &Path,
    held: &[String],
    sizes: &HashMap<String, i64>,
    sending: &mut Vec<String>,
) -> Result<Vec<Value>> {
    let mut kept: HashSet<String> = vault.objects.listed(&asked.dsp, FILES).await?.into_iter().collect();
    let mut files = Vec::new();
    let found = {
        let folder = folder.to_owned();
        let held = held.to_vec();
        tokio::task::spawn_blocking(move || contents::files(&folder, &held))
            .await
            .map_err(|_| Error::new("backup_failed", 500))??
    };
    for (index, file) in found.into_iter().enumerate() {
        let plain = here.join(format!("{index}"));
        let sealed = here.join(format!("{index}.sealed"));
        let (sha256, size) = {
            let (from, plain, sealed, key, dsp) = (
                folder.join(&file.path),
                plain.clone(),
                sealed.clone(),
                vault.key,
                asked.dsp.clone(),
            );
            let database = file.database;
            tokio::task::spawn_blocking(move || -> Result<(String, u64)> {
                copy(&from, &plain, database)?;
                let sealed = seal::seal(&key, &dsp, &plain, &sealed);
                fs::remove_file(&plain)?;
                sealed
            })
            .await
            .map_err(|_| Error::new("backup_failed", 500))??
        };
        let object = vault.file(&asked.generation, &sha256);
        if !kept.contains(&object) {
            sending.push(object.clone());
            sent(vault, &asked.dsp, &object, &sealed, &sha256, here).await?;
            kept.insert(object.clone());
        }
        fs::remove_file(&sealed)?;
        files.push(json!({
            "path": file.path.to_string_lossy(),
            "item": file.item,
            "sha256": sha256,
            "size": size,
            "object": object,
        }));
    }
    if held.iter().any(|item| item == contents::PICTURES)
        && let Some(pictures) = &state.objects
    {
        for (owner, sha256) in pictures.kept(&asked.dsp).await? {
            let object = vault.file(&asked.generation, &sha256);
            let path = format!("objects/{owner}/{sha256}");
            let size = if kept.contains(&object) {
                sizes.get(&object).copied().unwrap_or(0) as u64
            } else {
                let plain = here.join("picture");
                let sealed = here.join("picture.sealed");
                let Some(fetched) = pictures.get(&asked.dsp, &owner, &sha256).await? else {
                    continue;
                };
                written(fetched, &plain).await?;
                let (key, dsp, from, to) = (vault.key, asked.dsp.clone(), plain.clone(), sealed.clone());
                let (read, size) = tokio::task::spawn_blocking(move || seal::seal(&key, &dsp, &from, &to))
                    .await
                    .map_err(|_| Error::new("backup_failed", 500))??;
                fs::remove_file(&plain)?;
                // A picture is kept by the SHA-256 of what it holds: one that came down otherwise
                // isn't the picture.
                if read != sha256 {
                    fs::remove_file(&sealed)?;
                    return Err(Error::new("object_store_mismatch", 502));
                }
                sending.push(object.clone());
                sent(vault, &asked.dsp, &object, &sealed, &sha256, here).await?;
                fs::remove_file(&sealed)?;
                kept.insert(object.clone());
                size
            };
            files.push(json!({
                "path": path,
                "item": contents::PICTURES,
                "sha256": sha256,
                "size": size,
                "object": object,
            }));
        }
    }
    Ok(files)
}

/// Copies the file `from` to `to`: a database as SQLite copies one while it's written to, all of
/// it as it was at one moment.
fn copy(from: &Path, to: &Path, database: bool) -> Result<()> {
    if database {
        let source = rusqlite::Connection::open_with_flags(from, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        source.busy_timeout(Duration::from_secs(30))?;
        source.execute("VACUUM INTO ?", [to.to_string_lossy().as_ref()])?;
    } else {
        fs::copy(from, to)?;
    }
    Ok(())
}

/// Sends the sealed file `sealed` as `object`, then reads it back to check it opens as what has
/// the SHA-256 `sha256`.
async fn sent(
    vault: &Vault,
    dsp: &str,
    object: &str,
    sealed: &Path,
    sha256: &str,
    here: &Path,
) -> Result<()> {
    vault.objects.put(dsp, FILES, object, sealed).await?;
    let back = here.join("back.sealed");
    let fetched = vault
        .objects
        .get(dsp, FILES, object)
        .await?
        .ok_or_else(|| Error::new("backup_not_kept", 502))?;
    written(fetched, &back).await?;
    let (key, dsp, sha256, from) = (vault.key, dsp.to_owned(), sha256.to_owned(), back.clone());
    tokio::task::spawn_blocking(move || {
        seal::open(&key, &dsp, &from, &mut std::io::sink(), &sha256).map(drop)
    })
    .await
    .map_err(|_| Error::new("backup_failed", 500))??;
    fs::remove_file(back)?;
    Ok(())
}

/// Keeps the backup's description, and reads it back to check it opens.
async fn describe(vault: &Vault, dsp: &str, id: &str, manifest: &Value, here: &Path) -> Result<()> {
    let sealed = here.join("manifest.sealed");
    {
        let (key, binding, manifest, to) = (vault.key, dsp.to_owned(), manifest.clone(), sealed.clone());
        tokio::task::spawn_blocking(move || seal::seal_json(&key, &binding, &manifest, &to))
            .await
            .map_err(|_| Error::new("backup_failed", 500))??;
    }
    let object = vault.manifest(id);
    vault.objects.put(dsp, MANIFESTS, &object, &sealed).await?;
    let back = here.join("manifest.back");
    let fetched = vault
        .objects
        .get(dsp, MANIFESTS, &object)
        .await?
        .ok_or_else(|| Error::new("backup_not_kept", 502))?;
    written(fetched, &back).await?;
    let (key, binding) = (vault.key, dsp.to_owned());
    let read = tokio::task::spawn_blocking(move || seal::open_json(&key, &binding, &back))
        .await
        .map_err(|_| Error::new("backup_failed", 500))??;
    crate::ensure(&read == manifest, "backup_not_kept", 502)
}

/// Writes what came down to `to`.
pub async fn written(mut fetched: Fetched, to: &Path) -> Result<()> {
    let mut file = File::create(to)?;
    while let Some(chunk) = fetched.chunks.next().await {
        file.write_all(&chunk.map_err(|_| Error::new("object_store_unavailable", 503))?)?;
    }
    file.sync_all()?;
    Ok(())
}
