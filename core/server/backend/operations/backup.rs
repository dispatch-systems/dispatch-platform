use super::diagnostics::table_exists;
use crate::{
    Error, Result,
    db::{self, iso, n, s},
    ensure,
    foundation::{config::Config, crypto},
};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::Path,
};

const CANCEL_RESTORED: &str = concat!(
    "UPDATE jobs SET status='cancelled',message='Cancelled after restore',error='state_restored',\
     lease_owner=NULL,lease_until=NULL,completed_at=? WHERE status IN ",
    crate::job_statuses!(active)
);

pub fn backup(config: &Config, destination: &Path) -> Result<Value> {
    ensure(
        destination.is_absolute() && !destination.starts_with(&config.root),
        "backup_outside_state_required",
        400,
    )?;
    ensure(!destination.try_exists()?, "backup_destination_exists", 409)?;
    db::private_dir(destination)?;
    let mut files = Vec::new();
    fn copy(
        source: &Path,
        destination: &Path,
        relative: &Path,
        files: &mut Vec<Value>,
    ) -> Result<()> {
        let stat = fs::symlink_metadata(source)?;
        ensure(
            !stat.file_type().is_symlink() && stat.uid() == unsafe { libc::geteuid() },
            "invalid_backup_file",
            400,
        )?;
        if stat.is_dir() {
            db::private_dir(&destination.join(relative))?;
            let parts: Vec<_> = relative.iter().filter_map(|p| p.to_str()).collect();
            let browser_pulse = parts.len() >= 5
                && parts[0] == "dsps"
                && db::identifier(parts[1], "dsp_")
                && parts[2..4] == ["state", "browsers"]
                && parts.last() == Some(&"pulse");
            // What a DSP's owners can collect again stays out: a restore collects it anew.
            let dsp = parts.len() == 2 && parts[0] == "dsps" && db::identifier(parts[1], "dsp_");
            for item in fs::read_dir(source)? {
                let item = item?;
                let name = item.file_name().to_string_lossy().into_owned();
                // Chromium retains this disposable OS runtime link after shutdown.
                let pulse_runtime =
                    browser_pulse && name.ends_with("-runtime") && item.file_type()?.is_symlink();
                if pulse_runtime
                    || (dsp && name == db::CACHE)
                    || name.ends_with("-wal")
                    || name.ends_with("-shm")
                    || name.ends_with(".lock")
                    || [
                        "browser-runs",
                        "releases",
                        "activation-staging",
                        "activation-backups",
                    ]
                    .contains(&name.as_str())
                {
                    continue;
                }
                copy(&item.path(), destination, &relative.join(name), files)?;
            }
        } else {
            ensure(
                stat.is_file() && stat.nlink() == 1,
                "invalid_backup_file",
                400,
            )?;
            let target = destination.join(relative);
            if source.extension().is_some_and(|s| s == "sqlite") {
                let source = rusqlite::Connection::open_with_flags(
                    source,
                    rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                )?;
                source.execute("VACUUM INTO ?", [target.to_string_lossy().as_ref()])?;
                fs::set_permissions(&target, fs::Permissions::from_mode(0o600))?;
            } else {
                db::write_private(&target, &fs::read(source)?)?;
            }
            let bytes = fs::read(&target)?;
            files.push(json!({"path":relative.to_string_lossy(),"sha256":crypto::sha(&bytes),"size":bytes.len()}));
        }
        Ok(())
    }
    for area in ["data", "dsps"] {
        let source = config.root.join(area);
        if source.exists() {
            copy(&source, destination, Path::new(area), &mut files)?;
        }
    }
    db::write_private(
        &destination.join("backup.json"),
        &serde_json::to_vec_pretty(&json!({"format":2,"createdAt":iso(),"files":files}))?,
    )?;
    Ok(json!({"destination":destination,"files":files.len()}))
}
pub fn restore(source: &Path, target: &Path) -> Result<Value> {
    ensure(
        source.is_absolute() && target.is_absolute() && !target.starts_with(source),
        "invalid_restore_path",
        400,
    )?;
    ensure(
        !target.try_exists()? || fs::read_dir(target)?.next().is_none(),
        "restore_empty_target_required",
        409,
    )?;
    db::private_file(&source.join("backup.json"), false)?;
    let manifest: Value = serde_json::from_slice(&fs::read(source.join("backup.json"))?)?;
    ensure(n(&manifest, "format") == 2, "invalid_backup", 400)?;
    let files = manifest["files"]
        .as_array()
        .ok_or_else(|| Error::new("invalid_backup", 400))?;
    let mut seen = HashSet::new();
    for file in files {
        let path = s(file, "path");
        let parts: Vec<_> = path.split('/').collect();
        ensure(
            parts.len() > 1
                && ["data", "dsps"].contains(&parts[0])
                && parts
                    .iter()
                    .all(|p| !p.is_empty() && *p != "." && *p != ".." && !p.contains('\\'))
                && seen.insert(path),
            "invalid_backup_path",
            400,
        )?;
        let filename = source.join(path);
        db::private_file(&filename, false)?;
        ensure(
            fs::canonicalize(&filename)? == filename,
            "invalid_backup_file",
            400,
        )?;
        let bytes = fs::read(filename)?;
        ensure(
            bytes.len() as u64 == file["size"].as_u64().unwrap_or(u64::MAX)
                && crypto::sha(&bytes) == s(file, "sha256"),
            "backup_checksum_failed",
            400,
        )?;
    }
    db::private_dir(target)?;
    for file in files {
        let path = s(file, "path");
        db::write_private(&target.join(path), &fs::read(source.join(path))?)?;
    }
    let accounts = target.join("data/platform/accounts.sqlite");
    if accounts.exists() {
        let db = rusqlite::Connection::open(accounts)?;
        db.execute_batch(
            "DELETE FROM sessions;DELETE FROM resets;DELETE FROM invitations;DELETE FROM outbox;",
        )?;
        if table_exists(&db, "invitation_routes")? {
            db.execute("DELETE FROM invitation_routes", [])?;
        }
        // Agents' credentials end like sessions do.
        if let Some(agents) = crate::manifest::registry().agents {
            (agents.restored)(&db)?;
        }
    }
    // Each DSP's own sign-ins and grants end the same way. A backup older than DSPs keeping
    // their people has none.
    let dsps = target.join("dsps");
    if dsps.is_dir() {
        for entry in fs::read_dir(dsps)? {
            let database = entry?.path().join("data/dispatch.sqlite");
            if !database.is_file() {
                continue;
            }
            let db = rusqlite::Connection::open(database)?;
            if table_exists(&db, "sessions")? {
                db.execute_batch(
                    "DELETE FROM security_challenges;DELETE FROM session_security;\
                     DELETE FROM session_metadata;DELETE FROM sessions;DELETE FROM resets;\
                     DELETE FROM invitations;",
                )?;
            }
        }
    }
    for env in ["preview", "production"] {
        let jobs = target.join("data").join(env).join("jobs.sqlite");
        if jobs.exists() {
            let db = rusqlite::Connection::open(jobs)?;
            db.execute(CANCEL_RESTORED, [iso()])?;
        }
    }
    Ok(json!({"target":target,"files":files.len()}))
}
