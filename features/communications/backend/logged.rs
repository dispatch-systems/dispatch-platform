//! A change to a DSP's own data and its event in the platform's audit log, kept together: an
//! event that can't be written takes its change back with it.
use dispatch_core::{Result, db::Store};

/// Runs `change` in a transaction of the DSP's database inside one of the platform's, as core's
/// `Store::across` does: either commits with the other, unless the platform's own commit fails
/// after the DSP's.
pub(crate) fn together<T>(db: &Store, dsp: &str, change: impl FnOnce() -> Result<T>) -> Result<T> {
    let people = db.dsp(dsp)?;
    db.platform.transaction(|| people.transaction(change))
}
