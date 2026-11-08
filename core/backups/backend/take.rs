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
    ensure,
};
use base64::{Engine, engine::general_purpose::STANDARD as B64};
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

/// Takes the backup `asked` for, noting each file it sends as it's sent, so a backup that stops
/// part way, by failing or by the server stopping, lets go of them when it expires.
pub async fn take(state: &Arc<State>, vault: &Vault, asked: &Asked) -> Result<Taken> {
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
            Ok((
                db.dsp_folder(&dsp)?,
                row,
                features,
                super::history::sizes(&db.platform, &dsp)?,
            ))
        })
        .await?;
    let held: Vec<String> = contents::items(&folder)
        .into_iter()
        .map(|item| item.id)
        .filter(|id| !asked.skipped.contains(id))
        .collect();
    let here = db::private_dir(&staging(state).join(&asked.id))?;
    let taken = async {
        let holds = Holds {
            folder: &folder,
            items: &held,
            sizes: &sizes,
        };
        let files = send(state, vault, asked, &holds, &here).await?;
        let manifest = json!({
            "format": FORMAT,
            "backup": asked.id,
            "dsp": row,
            "features": features,
            "release": state.config.release,
            "generation": asked.generation,
            // What the DSP's people's authenticator apps are sealed with, which a platform
            // started over doesn't have.
            "platformKey": B64.encode(&state.key),
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
    let size = files
        .iter()
        .map(|file| file["size"].as_i64().unwrap_or(0))
        .sum();
    Ok(Taken { files, size })
}

/// What a backup holds: the DSP's folder, the items of it it holds, and how many bytes each file
/// its earlier backups hold holds, by what it's kept as.
struct Holds<'a> {
    folder: &'a Path,
    items: &'a [String],
    sizes: &'a HashMap<String, i64>,
}

/// A file copied aside, still to be sealed and sent: where it is in the DSP's folder, and what it
/// belongs to.
struct Copied {
    path: PathBuf,
    item: String,
    plain: PathBuf,
}

/// Copies the DSP's files aside: its databases first, each as it is at one moment, then the files
/// beside them, found once those are copied, so a file a database names is there too.
async fn copied(folder: &Path, held: &[String], here: &Path) -> Result<Vec<Copied>> {
    let (folder, held, here) = (folder.to_owned(), held.to_vec(), here.to_owned());
    tokio::task::spawn_blocking(move || -> Result<Vec<Copied>> {
        let mut copied = Vec::new();
        let mut copy_each = |found: Vec<contents::Found>| -> Result<()> {
            for file in found {
                let plain = here.join(format!("{}", copied.len()));
                copy(&folder.join(&file.path), &plain, file.database)?;
                copied.push(Copied {
                    path: file.path,
                    item: file.item,
                    plain,
                });
            }
            Ok(())
        };
        let databases: Vec<_> = contents::files(&folder, &held)?
            .into_iter()
            .filter(|file| file.database)
            .collect();
        // Every backup holds the DSP's own database: one that can't is no backup.
        ensure(
            databases
                .iter()
                .any(|file| file.path == Path::new(contents::DSP_DATABASE)),
            "backup_incomplete",
            500,
        )?;
        copy_each(databases)?;
        copy_each(
            contents::files(&folder, &held)?
                .into_iter()
                .filter(|file| !file.database)
                .collect(),
        )?;
        Ok(copied)
    })
    .await
    .map_err(|_| Error::new("backup_failed", 500))?
}

async fn send(
    state: &Arc<State>,
    vault: &Vault,
    asked: &Asked,
    holds: &Holds<'_>,
    here: &Path,
) -> Result<Vec<Value>> {
    let Holds {
        folder,
        items: held,
        sizes,
    } = *holds;
    let mut kept: HashSet<String> = vault
        .objects
        .listed(&asked.dsp, FILES)
        .await?
        .into_iter()
        .collect();
    let mut files = Vec::new();
    for file in copied(folder, held, here).await? {
        let sealed = file.plain.with_extension("sealed");
        let (sha256, size) = {
            let (plain, sealed, key, dsp) = (
                file.plain.clone(),
                sealed.clone(),
                vault.key,
                asked.dsp.clone(),
            );
            tokio::task::spawn_blocking(move || -> Result<(String, u64)> {
                let sealed = seal::seal(&key, &dsp, &plain, &sealed);
                fs::remove_file(&plain)?;
                sealed
            })
            .await
            .map_err(|_| Error::new("backup_failed", 500))??
        };
        let object = vault.file(&asked.generation, &sha256);
        if !kept.contains(&object) {
            uploading(state, &asked.id, &object).await?;
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
                // Listed, then gone: let go of while it was taken, so the backup isn't whole.
                let fetched = pictures
                    .get(&asked.dsp, &owner, &sha256)
                    .await?
                    .ok_or_else(|| Error::new("backup_incomplete", 502))?;
                written(fetched, &plain).await?;
                let (key, dsp, from, to) =
                    (vault.key, asked.dsp.clone(), plain.clone(), sealed.clone());
                let (read, size) =
                    tokio::task::spawn_blocking(move || seal::seal(&key, &dsp, &from, &to))
                        .await
                        .map_err(|_| Error::new("backup_failed", 500))??;
                fs::remove_file(&plain)?;
                // A picture is kept by the SHA-256 of what it holds: one that came down otherwise
                // isn't the picture.
                if read != sha256 {
                    fs::remove_file(&sealed)?;
                    return Err(Error::new("object_store_mismatch", 502));
                }
                uploading(state, &asked.id, &object).await?;
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

/// Notes that the backup `id` is sending `object`, before it's sent, so whatever ends the backup,
/// it's let go of once the backup expires.
async fn uploading(state: &Arc<State>, id: &str, object: &str) -> Result<()> {
    let (id, object) = (id.to_owned(), object.to_owned());
    state
        .run_bookkeeping(move |db| super::history::uploading(&db.platform, &id, &object))
        .await
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
        let (key, binding, manifest, to) =
            (vault.key, dsp.to_owned(), manifest.clone(), sealed.clone());
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
