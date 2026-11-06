use super::bootstrap::has_active_owner;
use crate::{
    Result,
    db::{self, Store},
    ensure,
    foundation::config::Config,
};
use std::fs;

/// What `serve` does once its state opened its store and before it listens: refuses a platform
/// with no active owner, then ends the jobs and browser runs the last server left.
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
