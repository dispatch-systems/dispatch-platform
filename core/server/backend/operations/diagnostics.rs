use super::lock::LOCK;
use crate::{
    Result,
    db::{self, s},
    ensure,
    foundation::{config::Config, observability},
    server::startup,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub(super) fn table_exists(connection: &rusqlite::Connection, name: &str) -> Result<bool> {
    Ok(connection.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?",
        [name],
        |row| row.get::<_, i64>(0),
    )? > 0)
}

/// What starting this release moves into each DSP's own directory, read beside a running
/// server without changing anything: per DSP, whether it already holds its people and what
/// moving them copies.
pub fn directories(config: &Config) -> Result<Value> {
    let read_only = |path: &Path| -> Result<db::Db> {
        ensure(path.is_file(), "platform_not_initialized", 404)?;
        db::private_file(path, false)?;
        Ok(db::Db(db::read_only(path)?))
    };
    let platform = read_only(&config.platform().join("accounts.sqlite"))?;
    let mut dsps = Vec::new();
    // Every column, as a release before short codes has no `code` yet.
    for row in platform.all(
        "SELECT * FROM dsps WHERE environment=? ORDER BY name",
        [config.environment],
    )? {
        let id = s(&row, "id");
        let database = config
            .root
            .join("dsps")
            .join(id)
            .join("data/dispatch.sqlite");
        let held = match read_only(&database) {
            Ok(dsp) if table_exists(&dsp.0, "settings")? => dsp
                .setting(crate::tenancy::dsps::DIRECTORY, Value::Null)?
                .as_str()
                .map(str::to_owned),
            _ => None,
        };
        dsps.push(json!({
            "id": id,
            "name": row["name"],
            "code": row["code"],
            "status": row["status"],
            "directory": held,
            "moves": if held.is_none() {
                json!(crate::accounts::directories::people(&platform, id)?)
            } else {
                Value::Null
            },
        }));
    }
    Ok(json!({"environment": config.environment.as_str(), "dsps": dsps}))
}
/// Read-only CLI inspection is safe while the serving process owns the write lock.
pub fn status(config: &Config) -> Result<Value> {
    let path = config.platform().join("accounts.sqlite");
    ensure(path.is_file(), "platform_not_initialized", 404)?;
    db::private_file(&path, false)?;
    let db = db::Db(db::read_only(path)?);
    Ok(
        json!({"environment":config.environment.as_str(),"runtime":"rust","dsps":db.all("SELECT name,environment,status FROM dsps",[])?}),
    )
}

/// Starts the platform restored at `root` as `serve` does before it listens, under this binary
/// and `config` but at `root`: every database brought up to its migrations, with what carries
/// over at startup, and what the last server left recovered (`startup`). It
/// serves nothing and runs no job, reader or mailer, and reads and writes only under `root`. A
/// root a server ever started on holds its lock file and is refused, so only a copy is ever
/// changed. Answers what it found, with how startup ended: how many databases it migrated, the
/// migrations applied by kind of database, each carry-over that failed, every error startup
/// logged, and the error that stopped it, as a failed migration does.
pub fn upgrade_check(config: &Config, root: &Path) -> Result<(Value, Result<()>)> {
    ensure(
        root.is_absolute() && !root.join(LOCK).try_exists()?,
        "upgrade_check_restored_copy_required",
        409,
    )?;
    let config = Config {
        root: root.into(),
        ..config.clone()
    };
    let before = migrations_recorded(root)?;
    let (started, events) =
        observability::recorded(|| startup::recover(&startup::prepare_server_store(&config)?));
    let after = migrations_recorded(root)?;
    let databases = after
        .iter()
        .filter(|(path, count)| before.get(*path).is_none_or(|was| was < count))
        .count();
    let mut migrations = BTreeMap::<&str, u64>::new();
    for event in events
        .iter()
        .filter(|e| e["event"] == "storage.migration_applied")
    {
        *migrations.entry(s(&event["fields"], "kind")).or_default() += 1;
    }
    let carried: Vec<&Value> = events
        .iter()
        .filter(|e| e["event"] == "settings.carry_over_failed")
        .map(|e| &e["fields"])
        .collect();
    let errors: Vec<&Value> = events.iter().filter(|e| e["level"] == "error").collect();
    let report = json!({"root":root,"databases":databases,"migrations":migrations,
        "carryOverFailures":carried,"errors":errors,
        "stopped":started.as_ref().err().map(|error| &error.code)});
    Ok((report, started))
}
/// How many migrations each database under `root` records, by its path.
fn migrations_recorded(root: &Path) -> Result<BTreeMap<PathBuf, i64>> {
    let mut found = BTreeMap::new();
    let mut folders = vec![root.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(folder)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                folders.push(path);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "sqlite")
            {
                let db = db::read_only(&path)?;
                let mut count = 0;
                for table in ["schema_migrations", "owner_migrations"] {
                    if table_exists(&db, table)? {
                        count +=
                            db.query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                                row.get::<_, i64>(0)
                            })?;
                    }
                }
                found.insert(path, count);
            }
        }
    }
    Ok(found)
}
