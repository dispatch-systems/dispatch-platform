//! Startup's ordered phases. Bootstrap, seed and fixtures prepare storage alone. State and
//! restored-copy upgrade checking prepare a server store; serve recovers through that State
//! before listening, while upgrade checking recovers its store without building a State.
use crate::{
    Result,
    collection::registry::Provider,
    db::{self, Store, s},
    ensure,
    foundation::config::Config,
    manifest::{self, retirement},
    server::operations::has_active_owner,
    tenancy::roles,
};
use std::fs;

/// Constructs storage, then upgrades platform policy, active/suspended DSPs, every kept
/// directory and address codes, in that order. Bootstrap, seed and fixtures stop here: no
/// environment admission, verification expiry, owner requirement or runtime recovery.
pub fn prepare_storage(config: Config) -> Result<Store> {
    let store = Store::create(config)?;
    upgrade_platform(&store)?;
    upgrade_dsps(&store)?;
    upgrade_directories(&store)?;
    store.backfill_codes()?;
    Ok(store)
}

fn upgrade_platform(store: &Store) -> Result<()> {
    retirement::platform(store)?;
    store.keep_switches()?;
    roles::backfill(&store.platform)
}

fn upgrade_dsps(store: &Store) -> Result<()> {
    for row in store.platform.all(
        "SELECT id FROM dsps WHERE status IN ('active','suspended')",
        [],
    )? {
        let id = s(&row, "id");
        store.migrate_dsp(id)?;
        store.open_collectors(id)?;
        store.initialize_schedules(id)?;
    }
    Ok(())
}

fn upgrade_directories(store: &Store) -> Result<()> {
    // Move people once, then repair legacy memberships and retire permissions in each directory.
    for id in store.kept_dsps()? {
        store.move_people(&id)?;
        let people = store.dsp(&id)?;
        roles::backfill_directory(&people, &id)?;
        retirement::roles(&people)?;
    }
    Ok(())
}

/// Admits the manifest environment before any storage write, then prepares storage and ends
/// provider sign-ins whose browser went with the last server. State and upgrade-check share
/// this phase; owner admission and recovery wait until `recover`.
pub fn prepare_server_store(config: &Config) -> Result<Store> {
    manifest::registry().check_env(config)?;
    let store = prepare_storage(config.clone())?;
    expire_verifications(&store)?;
    Ok(store)
}

fn expire_verifications(store: &Store) -> Result<()> {
    for dsp in store.platform.all(
        "SELECT id FROM dsps WHERE status IN ('active','suspended')",
        [],
    )? {
        for provider in Provider::all() {
            store.collector(db::s(&dsp, "id"), provider)?.exec("UPDATE connections SET \
                status='error',error='verification_expired' WHERE status IN ('signing_in','needs_verification')",[])?;
        }
    }
    Ok(())
}

/// Under the environment lock (or on upgrade-check's restored copy), refuses a platform
/// with no active owner, then ends jobs and browser runs the last server left. Serve calls
/// this through `State::run` after constructing State and before listening.
pub fn recover(db: &Store) -> Result<()> {
    ensure(has_active_owner(db)?, "run_bootstrap_before_starting", 503)?;
    db.recover_jobs(true)?;
    clean_browser_runs(&db.config)
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
