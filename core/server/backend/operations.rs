use crate::{
    Error, Result,
    db::{self, Store, iso, n, s},
    ensure,
    foundation::{config::Config, crypto},
    manifest::registry,
};
use fs2::FileExt;
use rusqlite::params;
use serde_json::{Value, json};
const CANCEL_RESTORED: &str = concat!(
    "UPDATE jobs SET status='cancelled',message='Cancelled after restore',error='state_restored',\
     lease_owner=NULL,lease_until=NULL,completed_at=? WHERE status IN ",
    crate::job_statuses!(active)
);
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    os::unix::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::Path,
};
pub struct Lock {
    _file: File,
}
impl Lock {
    pub fn acquire(root: &Path) -> Result<Self> {
        db::private_dir(root)?;
        let path = root.join(".platform.lock");
        db::private_file(&path, true)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)?;
        file.try_lock_exclusive()
            .map_err(|_| Error::new("stop_services_before_operation", 409))?;
        Ok(Self { _file: file })
    }
}
pub fn memory() -> (u64, u64) {
    fn rss(pid: u32) -> u64 {
        fs::read_to_string(format!("/proc/{pid}/status"))
            .ok()
            .and_then(|s| {
                s.lines()
                    .find(|l| l.starts_with("VmRSS:"))
                    .and_then(|l| l.split_whitespace().nth(1))
                    .and_then(|s| s.parse::<u64>().ok())
            })
            .unwrap_or(0)
            * 1024
    }
    fn descendants(pid: u32, seen: &mut HashSet<u32>) -> u64 {
        let mut total = 0;
        if let Ok(tasks) = fs::read_dir(format!("/proc/{pid}/task")) {
            for task in tasks.flatten() {
                if let Ok(children) = fs::read_to_string(task.path().join("children")) {
                    for child in children.split_whitespace().filter_map(|c| c.parse().ok()) {
                        if seen.insert(child) {
                            total += rss(child) + descendants(child, seen);
                        }
                    }
                }
            }
        }
        total
    }
    let pid = std::process::id();
    (rss(pid), descendants(pid, &mut HashSet::from([pid])))
}
pub fn available_space(path: &Path) -> Result<u64> {
    let path = std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|_| Error::new("unsafe_storage_path", 500))?;
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    if unsafe { libc::statvfs(path.as_ptr(), stat.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let stat = unsafe { stat.assume_init() };
    Ok(stat.f_bavail * stat.f_frsize)
}

fn table_exists(connection: &rusqlite::Connection, name: &str) -> Result<bool> {
    Ok(connection.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?",
        [name],
        |row| row.get::<_, i64>(0),
    )? > 0)
}

pub fn bootstrap(
    db: &Store,
    email: &str,
    first: &str,
    last: &str,
    password: &str,
) -> Result<Value> {
    ensure(
        db.platform
            .one("SELECT id FROM users LIMIT 1", [])?
            .is_none(),
        "bootstrap_requires_empty_platform",
        409,
    )?;
    let owner = db.create_user(email, first, last, password, true)?;
    let dsp = if db.config.env().is_preview() {
        let dsp = db.new_dsp("Dev DSP", "UTC", &owner.id, true)?;
        db.enable_all_features(&dsp.id)?;
        Some(dsp)
    } else {
        None
    };
    Ok(json!({"owner":owner,"dsp":dsp}))
}
/// Whether the platform has an active owner, as bootstrapping makes one. The server starts
/// only once it has.
pub fn has_active_owner(db: &Store) -> Result<bool> {
    Ok(db
        .platform
        .one(
            "SELECT id FROM users WHERE platform_owner=1 AND status='active' LIMIT 1",
            [],
        )?
        .is_some())
}
pub fn seed(db: &Store) -> Result<()> {
    ensure(
        db.config.development && db.config.fixture,
        "fixtures_require_development",
        403,
    )?;
    if db.config.platform().join("development-seeded").exists() {
        return Ok(());
    }
    ensure(
        db.platform
            .one("SELECT id FROM users LIMIT 1", [])?
            .is_none(),
        "fixture_state_not_empty",
        409,
    )?;
    let owner = db.create_user(
        "owner@dispatch.test",
        "Platform",
        "Owner",
        "Dispatch-demo-2026!",
        true,
    )?;
    let dev = db.new_dsp("Dev DSP", "America/Chicago", &owner.id, true)?;
    let north = db.new_dsp("Northline Logistics", "America/Chicago", &owner.id, false)?;
    let summit = db.new_dsp("Summit Delivery", "America/Denver", &owner.id, false)?;
    for dsp in [&dev, &north, &summit] {
        db.enable_all_features(&dsp.id)?;
    }
    // Each DSP but Dev's has an address of its own, as a DSP has once it is set up.
    db.set_code(&north.id, "nll")?;
    db.set_code(&summit.id, "sum")?;
    let member = db.create_user(
        "member@dispatch.test",
        "Jordan",
        "Ellis",
        "Dispatch-demo-2026!",
        false,
    )?;
    db.platform.exec(
        "INSERT INTO memberships(id,user_id,dsp_id,role,role_id) VALUES (?,?,?,'member',?)",
        params![
            crypto::id("mem")?,
            member.id,
            north.id,
            crate::tenancy::roles::default_role(&db.platform, &north.id, "member")?
        ],
    )?;
    for dsp in [&dev, &north] {
        let id = dsp.id.as_str();
        for collector in registry().collectors {
            collector.demo(db, id)?;
        }
        demo(db, id, &dsp.timezone)?;
        db.audit(Some(&owner.id), Some(id), "development.fixtures_loaded", "")?;
    }
    db::write_private(
        &db.config.platform().join("development-seeded"),
        b"synthetic fixtures initialized\n",
    )
}
/// Fills a new DSP with every feature's demo data, given its timezone, and gives its default
/// roles what they hold in a demo.
pub fn demo(db: &Store, dsp: &str, timezone: &str) -> Result<()> {
    for demo in registry().features.iter().filter_map(|f| f.demo) {
        demo(db, dsp, timezone)?;
    }
    crate::tenancy::roles::demo(&db.platform, dsp)
}
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
            for item in fs::read_dir(source)? {
                let item = item?;
                let name = item.file_name().to_string_lossy().into_owned();
                // Chromium retains this disposable OS runtime link after shutdown.
                let pulse_runtime =
                    browser_pulse && name.ends_with("-runtime") && item.file_type()?.is_symlink();
                if pulse_runtime
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
        // Agent keys end like sessions do. A backup older than them has none.
        if table_exists(&db, "agent_keys")? {
            db.execute(
                "UPDATE agent_keys SET revoked_at=?1 WHERE revoked_at IS NULL",
                [iso()],
            )?;
        }
        // An unspent OAuth code is a short-lived bearer capability that can mint a new key.
        // Old backups predate OAuth, so invalidate it only when the table was present.
        if table_exists(&db, "oauth_codes")? {
            db.execute("DELETE FROM oauth_codes WHERE used_at IS NULL", [])?;
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

/// Read-only CLI inspection is safe while the serving process owns the write lock.
pub fn status(config: &Config) -> Result<Value> {
    let path = config.platform().join("accounts.sqlite");
    ensure(path.is_file(), "platform_not_initialized", 404)?;
    db::private_file(&path, false)?;
    let connection =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let db = db::Db(connection);
    Ok(
        json!({"environment":config.environment,"runtime":"rust","dsps":db.all("SELECT name,environment,status FROM dsps",[])?}),
    )
}

/// The environment lock proves no previous core can own these ephemeral runs.
pub fn clean_browser_runs(config: &Config) -> Result<()> {
    let root = db::private_dir(&config.environment_root().join("browser-runs"))?;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        ensure(
            db::identifier(&entry.file_name().to_string_lossy(), "run_")
                || db::identifier(&entry.file_name().to_string_lossy(), "browseros_"),
            "unexpected_browser_run",
            503,
        )?;
        db::private_dir(&entry.path())?;
        fs::remove_dir_all(entry.path())?;
    }
    Ok(())
}
