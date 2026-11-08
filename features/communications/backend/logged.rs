//! A change to a DSP's own data and its event in the platform's audit log, kept together: both
//! commit, or neither does, so no change is left that the log never heard of.
use dispatch_core::{Result, db::Store};

/// Runs `change` in one transaction of the DSP's database and the platform's: what it writes to
/// either, its audit event among it, commits only with the rest.
pub(crate) fn together<T>(db: &Store, dsp: &str, change: impl FnOnce() -> Result<T>) -> Result<T> {
    let people = db.dsp(dsp)?;
    db.platform.transaction(|| people.transaction(change))
}
